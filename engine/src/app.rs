//! El juego completo: pantallas del screenpack (título, selección, versus,
//! victoria, storyboards) alrededor de las peleas.
//!
//! La plataforma llama a [`App::tick`] con las teclas de cada jugador y a
//! [`App::draw`] para obtener la lista de dibujo. Los sprites y sonidos a los
//! que se refiere esa lista son los de [`App::sffs`] y [`App::snds`], que
//! cambian al pasar de las pantallas a una pelea: [`App::epoch`] avisa.

use crate::cmd::*;
use crate::game::Game;
use crate::gfx::{Frame, PalRef, SoundCmd, SpriteCmd, Trans};
use crate::motif::{Mode, Motif, Placed, Slot, SndNo, SprNo};
use crate::sff::Sff;
use crate::snd::Snd;
use crate::story::{fade, Story};

/// A dónde se va al salir de la pantalla actual.
#[derive(Clone, Debug)]
enum Next {
    /// Arranque: 0 = logo, 1 = intro, 2 = título.
    Boot(u8),
    Title,
    Select,
    Versus,
    Fight,
    AfterFight,
    AfterVictory,
    /// Siguiente combate del modo arcade (o el final).
    Arcade,
    Story(String, Box<Next>),
    WinText,
}

struct SelectState {
    cursor: [(i32, i32); 2],
    done: [bool; 2],
    /// Personaje que enseña ahora mismo una casilla aleatoria.
    random: [usize; 2],
    /// Posición en la lista de escenarios; 0 = al azar.
    stage: usize,
    stage_done: bool,
    wait: i32,
}

enum Screen {
    Story(Story, Next),
    Title { sel: usize, idle: i32 },
    Select(SelectState),
    Versus,
    Fight,
    Victory { winner: usize, name: String, quote: Vec<String> },
    WinText,
}

pub struct App {
    root: String,
    pub motif: Option<Motif>,
    sffs: Vec<Sff>,
    snds: Vec<Snd>,
    /// Cuántos SFF y SND son del screenpack (el resto son de un storyboard).
    base: (usize, usize),
    pub game: Option<Game>,
    screen: Screen,
    /// Ticks en la pantalla actual y duración de su fundido de entrada.
    t: i32,
    fadein: i32,
    /// Salida en curso: ticks transcurridos, duración y destino.
    leaving: Option<(i32, i32, Next)>,
    epoch: u32,
    sounds: Vec<SoundCmd>,
    prev: [u16; 2],
    mode: Mode,
    demo: bool,
    /// Personajes y paletas elegidos, y escenario.
    chosen: [usize; 2],
    pals: [i32; 2],
    stage: usize,
    /// Rivales que quedan en el modo arcade y combate actual.
    queue: Vec<usize>,
    match_no: i32,
    winner: usize,
    rng: u32,
    /// Ticks con START + SELECT mantenidos (salir de la pelea).
    quit_hold: i32,
    select_alone: bool,
    pub pad2: bool,
    pub error: Option<String>,
}

fn any(keys: u16) -> bool {
    keys & IN_ANY != 0
}

impl App {
    fn blank(root: &str) -> App {
        App {
            root: root.to_string(),
            motif: None,
            sffs: Vec::new(),
            snds: Vec::new(),
            base: (0, 0),
            game: None,
            screen: Screen::Fight,
            t: 0,
            fadein: 0,
            leaving: None,
            epoch: 0,
            sounds: Vec::new(),
            prev: [0; 2],
            mode: Mode::Versus,
            demo: false,
            chosen: [0, 0],
            pals: [1, 1],
            stage: 0,
            queue: Vec::new(),
            match_no: 0,
            winner: 0,
            rng: 0x9e37_79b9,
            quit_hold: 0,
            select_alone: false,
            pad2: false,
            error: None,
        }
    }

    /// Sin screenpack: una pelea que se repite, como el primer MUGEN.
    pub fn direct(root: &str, p1: &str, p2: &str, stage: &str) -> Result<App, String> {
        let mut app = App::blank(root);
        app.game = Some(Game::new(root, p1, p2, stage)?);
        Ok(app)
    }

    /// Con screenpack: `motif` es la ruta de su `system.def` dentro de `root`.
    pub fn with_motif(root: &str, motif: &str) -> Result<App, String> {
        let mut app = App::blank(root);
        let m = Motif::load(root, &format!("{root}/{motif}"), &mut app.sffs, &mut app.snds)?;
        app.base = (app.sffs.len(), app.snds.len());
        app.motif = Some(m);
        app.go(Next::Boot(0));
        Ok(app)
    }

    /// Sprites a los que se refiere la lista de dibujo actual.
    pub fn sffs(&self) -> &[Sff] {
        match (&self.screen, &self.game) {
            (Screen::Fight, Some(g)) => &g.sffs,
            _ => &self.sffs,
        }
    }

    pub fn snds(&self) -> &[Snd] {
        match (&self.screen, &self.game) {
            (Screen::Fight, Some(g)) => &g.snds,
            _ => &self.snds,
        }
    }

    /// Cambia cada vez que cambian las listas de sprites y sonidos.
    pub fn epoch(&self) -> u32 {
        self.epoch
    }

    fn rand(&mut self, n: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as usize % n.max(1)
    }

    fn play(&mut self, snd: SndNo) {
        let (Some((g, n)), Some(m)) = (snd, &self.motif) else { return };
        if let Some(idx) = self.snds[m.snd as usize].find(g, n) {
            self.sounds.push(SoundCmd { bank: m.snd, idx, chan: -1, vol: 1.0, pan: 0.0, freq: 1.0, stop: false });
        }
    }

    /// Empieza a salir de la pantalla actual con un fundido.
    fn leave(&mut self, time: i32, next: Next) {
        if self.leaving.is_none() {
            self.leaving = Some((0, time.max(1), next));
        }
    }

    /// Deja sólo los recursos del screenpack.
    fn drop_extras(&mut self) {
        if self.game.is_some() || self.sffs.len() != self.base.0 {
            self.epoch += 1;
        }
        self.game = None;
        self.sffs.truncate(self.base.0);
        self.snds.truncate(self.base.1);
    }

    fn start_story(&mut self, path: &str, next: Next) {
        self.drop_extras();
        match Story::load(path, &mut self.sffs) {
            Ok(s) => {
                self.epoch += 1;
                self.screen = Screen::Story(s, next);
            }
            // Un storyboard que no se puede leer se salta.
            Err(_) => self.go(next),
        }
    }

    /// Cola de rivales del modo arcade, por niveles de `order`.
    fn arcade_queue(&mut self) -> Vec<usize> {
        let Some(m) = &self.motif else { return Vec::new() };
        let (levels, orders): (Vec<i32>, Vec<i32>) = (m.max_matches.clone(), m.chars.iter().map(|c| c.order).collect());
        let mut queue = Vec::new();
        for (level, &max) in levels.iter().enumerate() {
            let mut pool: Vec<usize> = (0..orders.len()).filter(|&i| orders[i] == level as i32 + 1).collect();
            for _ in 0..max.max(0) {
                if pool.is_empty() {
                    break;
                }
                let k = self.rand(pool.len());
                queue.push(pool.swap_remove(k));
            }
        }
        queue
    }

    /// Cambia de pantalla.
    fn go(&mut self, next: Next) {
        self.t = 0;
        self.leaving = None;
        self.fadein = 0;
        let Some(m) = &self.motif else { return };
        match next {
            Next::Boot(step) => {
                let (logo, intro) = (m.logo.clone(), m.intro.clone());
                match (step, logo, intro) {
                    (0, Some(p), _) => self.start_story(&p, Next::Boot(1)),
                    (0, None, _) => self.go(Next::Boot(1)),
                    (1, _, Some(p)) => self.start_story(&p, Next::Title),
                    _ => self.go(Next::Title),
                }
            }
            Next::Title => {
                self.fadein = m.title.fadein;
                self.drop_extras();
                self.demo = false;
                self.screen = Screen::Title { sel: 0, idle: 0 };
            }
            Next::Select => {
                self.fadein = m.select.fadein;
                // Un cursor que empezaría en una casilla vacía va a la primera ocupada.
                let cols = m.select.columns;
                let first = m.slots.iter().position(|s| *s != Slot::Empty).unwrap_or(0) as i32;
                let cursor = m.select.cursor_start.map(|c| {
                    let empty = c.0 < 0 || c.1 < 0 || c.1 >= cols || m.slots.get((c.0 * cols + c.1) as usize).map_or(true, |s| *s == Slot::Empty);
                    if empty && !m.select.move_over_empty {
                        (first / cols, first % cols)
                    } else {
                        c
                    }
                });
                self.drop_extras();
                self.screen = Screen::Select(SelectState { cursor, done: [false; 2], random: [0, 0], stage: 0, stage_done: false, wait: 0 });
            }
            Next::Versus => {
                self.fadein = m.vs.fadein;
                self.drop_extras();
                self.screen = Screen::Versus;
            }
            Next::Fight => {
                let (fight, p1, p2) = (m.fight_def.clone(), m.chars[self.chosen[0]].def.clone(), m.chars[self.chosen[1]].def.clone());
                let stage = m.stages[self.stage].path.clone();
                let bars = m.demo.bars;
                self.drop_extras();
                match Game::load(&self.root, &fight, &p1, &p2, &stage, self.pals) {
                    Ok(mut g) => {
                        g.auto_rematch = false;
                        g.training = self.mode == Mode::Training && !self.demo;
                        g.show_bars = !self.demo || bars;
                        g.chars[0].ai = self.mode == Mode::Watch || self.demo;
                        self.game = Some(g);
                        self.epoch += 1;
                        self.quit_hold = 0;
                        self.screen = Screen::Fight;
                    }
                    Err(e) => {
                        self.error = Some(e);
                        self.go(Next::Title);
                    }
                }
            }
            Next::AfterFight => {
                let Some(g) = &self.game else { return self.go(Next::Title) };
                let w = if g.round.wins[1] > g.round.wins[0] { 1 } else { 0 };
                self.winner = w;
                let def = &g.chars[w].def;
                let quote = if def.quotes.is_empty() { None } else { Some(def.quotes[(self.rng >> 4) as usize % def.quotes.len()].clone()) };
                let name = def.display.clone();
                if self.demo {
                    return self.go(Next::Title);
                }
                if !m.victory.enabled {
                    return self.after_victory();
                }
                let text = quote.unwrap_or_else(|| m.victory.quote_text.clone());
                let width = m.victory.quote_window.map_or(280, |w| w[2] - w[0]);
                let lines = m.fonts.wrap(m.victory.quote_font, &text, width);
                self.fadein = m.victory.fadein;
                self.drop_extras();
                self.screen = Screen::Victory { winner: self.chosen[w], name, quote: lines };
            }
            Next::Arcade => {
                if self.queue.is_empty() {
                    // Sin más rivales: final del personaje, o el texto de victoria.
                    let after = match m.credits.clone() {
                        Some(c) => Next::Story(c, Box::new(Next::Title)),
                        None => Next::Title,
                    };
                    match m.chars[self.chosen[0]].ending.clone() {
                        Some(e) => self.start_story(&e, after),
                        None => self.go(Next::WinText),
                    }
                    return;
                }
                let rival = self.queue.remove(0);
                self.chosen[1] = rival;
                self.pals[1] = 1;
                self.match_no += 1;
                let listed = m.listed_stages();
                let own = m.chars[rival].stage;
                self.stage = match own {
                    Some(s) => s,
                    None => listed[self.rand(listed.len())],
                };
                self.go(Next::Versus);
            }
            Next::AfterVictory => self.after_victory(),
            Next::Story(path, then) => self.start_story(&path, *then),
            Next::WinText => {
                self.fadein = 32;
                self.drop_extras();
                self.screen = Screen::WinText;
            }
        }
    }

    fn after_victory(&mut self) {
        let gameover = self.motif.as_ref().and_then(|m| m.gameover.clone());
        match self.mode {
            Mode::Arcade if self.winner == 0 => self.go(Next::Arcade),
            Mode::Arcade => match gameover {
                Some(p) => self.go(Next::Story(p, Box::new(Next::Title))),
                None => self.go(Next::Title),
            },
            _ => self.go(Next::Select),
        }
    }

    // --------------------------------------------------------------- lógica

    /// Avanza un tick. `keys[i]` son las teclas del jugador `i`.
    pub fn tick(&mut self, keys: [u16; 2]) {
        self.sounds.clear();
        let pressed = [keys[0] & !self.prev[0], keys[1] & !self.prev[1]];
        self.prev = keys;
        self.t += 1;

        // La pelea sigue corriendo durante su fundido de salida.
        if matches!(self.screen, Screen::Fight) {
            self.tick_fight(keys, pressed);
        }
        if let Some((t, time, next)) = self.leaving.take() {
            if t + 1 >= time {
                self.go(next);
            } else {
                self.leaving = Some((t + 1, time, next));
            }
            return;
        }
        let accept = any(pressed[0]) || any(pressed[1]);
        let back = (pressed[0] | pressed[1]) & IN_SELECT != 0;
        match &mut self.screen {
            Screen::Fight => {}
            Screen::Story(story, next) => {
                if story.tick(accept && self.t > 10) {
                    let next = next.clone();
                    self.go(next);
                }
            }
            Screen::Title { .. } => self.tick_title(pressed, accept),
            Screen::Select(_) => self.tick_select(keys, pressed, back),
            Screen::Versus => {
                let m = self.motif.as_ref().unwrap();
                let (time, fadein, fadeout) = (m.vs.time, m.vs.fadein, m.vs.fadeout);
                if self.t >= time || (accept && self.t > fadein) {
                    self.leave(fadeout, Next::Fight);
                }
            }
            Screen::Victory { .. } => {
                let m = self.motif.as_ref().unwrap();
                let (time, fadein, fadeout) = (m.victory.time, m.victory.fadein, m.victory.fadeout);
                if self.t >= time || (accept && self.t > fadein + 20) {
                    self.leave(fadeout, Next::AfterVictory);
                }
            }
            Screen::WinText => {
                let m = self.motif.as_ref().unwrap();
                if self.t >= m.win.time || (accept && self.t > 60) {
                    let next = match m.credits.clone() {
                        Some(c) => Next::Story(c, Box::new(Next::Title)),
                        None => Next::Title,
                    };
                    self.leave(64, next);
                }
            }
        }
    }

    fn tick_fight(&mut self, keys: [u16; 2], pressed: [u16; 2]) {
        let has_motif = self.motif.is_some();
        let (mode, demo, pad2) = (self.mode, self.demo, self.pad2);
        let Some(g) = self.game.as_mut() else { return };
        if has_motif {
            g.chars[1].ai = match mode {
                _ if demo => true,
                Mode::Training => false,
                Mode::Versus => !pad2,
                _ => true,
            };
        }
        g.tick(keys);

        // SELECT solo: cajas de colisión. START + SELECT mantenidos: salir.
        let (sel, start) = (keys[0] & IN_SELECT != 0, keys[0] & IN_S != 0);
        if pressed[0] & IN_SELECT != 0 {
            self.select_alone = true;
        }
        if sel && start {
            self.select_alone = false;
            self.quit_hold += 1;
        } else {
            self.quit_hold = 0;
        }
        if !sel && self.select_alone {
            self.select_alone = false;
            g.show_clsn = !g.show_clsn;
        }
        if !has_motif || self.leaving.is_some() {
            return;
        }
        let demo_end = self.motif.as_ref().map_or(0, |m| m.demo.endtime);
        if g.finished {
            self.leave(30, Next::AfterFight);
        } else if self.quit_hold >= 30 || (demo && (self.t >= demo_end || (self.t > 30 && (any(pressed[0]) || any(pressed[1]))))) {
            self.leave(20, Next::Title);
        }
    }

    fn tick_title(&mut self, pressed: [u16; 2], accept: bool) {
        let m = self.motif.as_ref().unwrap();
        let (n, fadeout) = (m.title.items.len(), m.title.fadeout);
        let (snd_move, snd_done) = (m.title.snd_move, m.title.snd_done);
        let (demo, wait) = (m.demo.enabled, m.demo.waittime);
        let (chars, listed) = (m.chars.len(), m.listed_stages());
        let Screen::Title { sel, idle } = &mut self.screen else { return };
        let p = pressed[0] | pressed[1];
        *idle = if p != 0 { 0 } else { *idle + 1 };
        let mut moved = false;
        if p & IN_UP != 0 {
            *sel = (*sel + n - 1) % n;
            moved = true;
        }
        if p & IN_DOWN != 0 {
            *sel = (*sel + 1) % n;
            moved = true;
        }
        let (sel, idle) = (*sel, *idle);
        if moved {
            self.play(snd_move);
        }
        if accept && self.t > 5 {
            self.mode = self.motif.as_ref().unwrap().title.items[sel].0;
            self.play(snd_done);
            self.leave(fadeout, Next::Select);
        } else if demo && idle >= wait {
            // Modo demostración: una pelea entre dos personajes al azar.
            self.demo = true;
            self.mode = Mode::Watch;
            self.chosen = [self.rand(chars), self.rand(chars)];
            self.pals = [1, 2];
            self.stage = listed[self.rand(listed.len())];
            self.leave(fadeout, Next::Fight);
        }
    }

    /// Casilla de la cuadrícula de selección.
    fn slot(&self, cell: (i32, i32)) -> Slot {
        let m = self.motif.as_ref().unwrap();
        let (rows, cols) = (m.select.rows, m.select.columns);
        if cell.0 < 0 || cell.1 < 0 || cell.0 >= rows || cell.1 >= cols {
            return Slot::Empty;
        }
        m.slots.get((cell.0 * cols + cell.1) as usize).copied().unwrap_or(Slot::Empty)
    }

    fn tick_select(&mut self, keys: [u16; 2], pressed: [u16; 2], back: bool) {
        let m = self.motif.as_ref().unwrap();
        let sel = &m.select;
        let (rows, cols, wrap, over_empty) = (sel.rows, sel.columns, sel.wrapping, sel.move_over_empty);
        let (snd_move, snd_done, snd_cancel) = (sel.snd_move, sel.snd_done, sel.snd_cancel);
        let (snd_stage_move, snd_stage_done, fadeout, switch) = (sel.snd_stage_move, sel.snd_stage_done, sel.fadeout, sel.random_switch);
        let (chars, stages) = (m.chars.len(), m.listed_stages());
        let _ = keys;
        if back {
            self.play(snd_cancel);
            self.leave(fadeout, Next::Title);
            return;
        }
        // Quién elige: en arcade sólo el jugador 1; en versus con dos mandos,
        // cada uno lo suyo; si no, el jugador 1 elige a los dos por turno.
        let two_pads = self.mode == Mode::Versus && self.pad2;
        let players = if self.mode == Mode::Arcade { 1 } else { 2 };
        let (t, mode) = (self.t, self.mode);
        let mut sounds: Vec<SndNo> = Vec::new();
        let mut picks: Vec<(usize, usize, i32)> = Vec::new();

        let Screen::Select(st) = &self.screen else { return };
        let (mut cursor, mut done, mut random) = (st.cursor, st.done, st.random);
        let (mut stage, mut stage_done, mut wait) = (st.stage, st.stage_done, st.wait);

        for p in 0..players {
            if done[p] {
                continue;
            }
            // Sin segundo mando, el jugador 2 se elige cuando acabó el 1.
            let input = if p == 0 || two_pads {
                pressed[p]
            } else if st.done[0] {
                pressed[0]
            } else {
                continue;
            };
            let (dr, dc) = (
                (input & IN_DOWN != 0) as i32 - (input & IN_UP != 0) as i32,
                (input & IN_RIGHT != 0) as i32 - (input & IN_LEFT != 0) as i32,
            );
            if dr != 0 || dc != 0 {
                // Avanza en esa dirección hasta una casilla ocupada.
                let mut c = cursor[p];
                for _ in 0..rows.max(cols) {
                    c = (c.0 + dr, c.1 + dc);
                    if wrap {
                        c = (c.0.rem_euclid(rows), c.1.rem_euclid(cols));
                    }
                    if c.0 < 0 || c.1 < 0 || c.0 >= rows || c.1 >= cols {
                        break;
                    }
                    if over_empty || self.slot(c) != Slot::Empty {
                        cursor[p] = c;
                        sounds.push(snd_move[p]);
                        break;
                    }
                }
            }
            if t % switch == 0 {
                random[p] = (random[p] + 1 + (t as usize / 7) % chars.max(1)) % chars.max(1);
            }
            if any(input) && t > 10 {
                let ch = match self.slot(cursor[p]) {
                    Slot::Char(i) => Some(i),
                    Slot::Random => Some(random[p]),
                    Slot::Empty => None,
                };
                if let Some(i) = ch {
                    // El botón elige la paleta: x, y, z, a, b, c = 1 a 6.
                    let pal = [IN_X, IN_Y, IN_Z, IN_A, IN_B, IN_C].iter().position(|&b| input & b != 0).map_or(1, |n| n as i32 + 1);
                    picks.push((p, i, pal));
                    done[p] = true;
                    sounds.push(snd_done[p]);
                }
            }
        }

        let all = (0..players).all(|p| done[p]);
        let had_all = (0..players).all(|p| st.done[p]);
        if all && had_all && mode != Mode::Arcade && !stage_done {
            // Escenario: izquierda y derecha, y un botón para confirmar.
            let input = pressed[0] | if two_pads { pressed[1] } else { 0 };
            let n = stages.len() + 1;
            if input & IN_RIGHT != 0 {
                stage = (stage + 1) % n;
                sounds.push(snd_stage_move);
            }
            if input & IN_LEFT != 0 {
                stage = (stage + n - 1) % n;
                sounds.push(snd_stage_move);
            }
            if any(input) {
                stage_done = true;
                sounds.push(snd_stage_done);
            }
        }
        let ready = all && (mode == Mode::Arcade || stage_done);
        if ready {
            wait += 1;
        }

        if let Screen::Select(st) = &mut self.screen {
            *st = SelectState { cursor, done, random, stage, stage_done, wait };
        }
        for (p, i, pal) in picks {
            self.chosen[p] = i;
            self.pals[p] = pal;
        }
        for s in sounds {
            self.play(s);
        }
        if ready && wait == 20 {
            if mode == Mode::Arcade {
                let intro = self.motif.as_ref().and_then(|m| m.chars[self.chosen[0]].intro.clone());
                self.queue = self.arcade_queue();
                self.match_no = 0;
                let first = match intro {
                    Some(p) => Next::Story(p, Box::new(Next::Arcade)),
                    None => Next::Arcade,
                };
                self.leave(fadeout, first);
            } else {
                self.stage = if stage == 0 { stages[self.rand(stages.len())] } else { stages[stage - 1] };
                self.leave(fadeout, Next::Versus);
            }
        }
    }

    // --------------------------------------------------------------- dibujo

    fn spr(&self, frame: &mut Frame, spr: SprNo, x: f32, y: f32, facing: f32) {
        let (Some((g, n)), Some(m)) = (spr, &self.motif) else { return };
        let sff = &self.sffs[m.sff as usize];
        if let Some(si) = sff.find(g, n) {
            let pal = PalRef::Sff(m.sff, sff.sprites[si as usize].pal);
            frame.sprite(SpriteCmd { sff: m.sff, spr: si, x, y, sx: facing, sy: 1.0, pal, trans: Trans::None, clip: None, src: None });
        }
    }

    /// Retrato de un personaje. `p.spr` es 9000,n: el retrato número n.
    fn portrait(&self, frame: &mut Frame, ch: usize, p: &Placed, x: f32, y: f32) {
        let m = self.motif.as_ref().unwrap();
        let kind = p.spr.map_or(0, |s| s.1.clamp(0, 2) as usize);
        let c = &m.chars[ch];
        // Si falta ese retrato se usa el grande, y si no el pequeño.
        let Some(si) = c.portraits[kind].or(c.portraits[1]).or(c.portraits[0]) else { return };
        let sff = &self.sffs[m.portraits as usize];
        frame.sprite(SpriteCmd {
            sff: m.portraits,
            spr: si,
            x: x + p.offset.0,
            y: y + p.offset.1,
            sx: p.facing * p.scale.0,
            sy: p.scale.1,
            pal: PalRef::Sff(m.portraits, sff.sprites[si as usize].pal),
            trans: Trans::None,
            clip: p.window,
            src: None,
        });
    }

    fn draw_title(&self, frame: &mut Frame, sel: usize) {
        let m = self.motif.as_ref().unwrap();
        let t = &m.title;
        frame.clear = m.title_bg.clear & 0x00ff_ffff;
        m.title_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 0);
        // La lista se desplaza para que la opción elegida quede a la vista.
        let first = sel.saturating_sub(t.visible - 1);
        for (i, (_, name)) in t.items.iter().enumerate().skip(first).take(t.visible) {
            let row = (i - first) as f32;
            let (x, y) = (t.menu_pos.0 + row * t.spacing.0, t.menu_pos.1 + row * t.spacing.1);
            if i == sel {
                if t.box_visible {
                    let b = t.box_coords;
                    let pulse = 40 + (self.t % 32 - 16).abs() * 3;
                    frame.rect((x + b[0]) as i32, (y + b[1]) as i32, (b[2] - b[0]) as i32 + 1, (b[3] - b[1]) as i32 + 1, 0xffff_ffff, Trans::Add { src: pulse as u16, dst: 256 });
                }
                m.fonts.draw(frame, t.active_font.or(t.item_font), x, y, name);
            } else {
                m.fonts.draw(frame, t.item_font, x, y, name);
            }
        }
        m.title_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 1);
    }

    fn mode_name(&self) -> &str {
        let m = self.motif.as_ref().unwrap();
        m.title.items.iter().find(|(mode, _)| *mode == self.mode).map_or("", |(_, n)| n.as_str())
    }

    fn draw_select(&self, frame: &mut Frame, st: &SelectState) {
        let m = self.motif.as_ref().unwrap();
        let s = &m.select;
        frame.clear = m.select_bg.clear & 0x00ff_ffff;
        m.select_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 0);
        m.fonts.draw(frame, s.title_font, s.title_offset.0, s.title_offset.1, self.mode_name());
        let cell_pos = |r: i32, c: i32| (s.pos.0 + c as f32 * (s.cell_size.0 + s.cell_spacing), s.pos.1 + r as f32 * (s.cell_size.1 + s.cell_spacing));
        for r in 0..s.rows {
            for c in 0..s.columns {
                let (x, y) = cell_pos(r, c);
                let slot = self.slot((r, c));
                if slot != Slot::Empty || s.show_empty {
                    self.spr(frame, s.cell_bg, x, y, 1.0);
                }
                match slot {
                    Slot::Char(i) => self.portrait(frame, i, &s.portrait, x, y),
                    Slot::Random => self.spr(frame, s.cell_random, x, y, 1.0),
                    Slot::Empty => {}
                }
            }
        }
        let players = if self.mode == Mode::Arcade { 1 } else { 2 };
        let two_pads = self.mode == Mode::Versus && self.pad2;
        for p in 0..players {
            // El cursor del jugador 2 aparece cuando le toca elegir.
            if p == 1 && !two_pads && !st.done[0] {
                continue;
            }
            let (x, y) = cell_pos(st.cursor[p].0, st.cursor[p].1);
            let spr = if st.done[p] { s.cursor_done[p] } else { s.cursor_active[p] };
            let hidden = p == 1 && s.p2_blink && !st.done[1] && st.cursor[0] == st.cursor[1] && self.t / 4 % 2 == 0;
            if !hidden {
                self.spr(frame, spr, x, y, 1.0);
            }
            let ch = match self.slot(st.cursor[p]) {
                Slot::Char(i) => Some(i),
                Slot::Random => Some(st.random[p]),
                Slot::Empty => None,
            };
            let ch = if st.done[p] { Some(self.chosen[p]) } else { ch };
            if let Some(i) = ch {
                self.portrait(frame, i, &s.face[p], 0.0, 0.0);
                m.fonts.draw(frame, s.name_font[p], s.name_offset[p].0, s.name_offset[p].1, &m.chars[i].name);
            }
        }
        if self.mode != Mode::Arcade && (0..players).all(|p| st.done[p]) {
            let stages = m.listed_stages();
            let text = if st.stage == 0 { "Stage: Random".to_string() } else { format!("Stage {}: {}", st.stage, m.stages[stages[st.stage - 1]].name) };
            let font = if st.stage_done {
                s.stage_done_font
            } else if self.t / 6 % 2 == 0 {
                s.stage_active_font
            } else {
                s.stage_active2_font.or(s.stage_active_font)
            };
            m.fonts.draw(frame, font, s.stage_pos.0, s.stage_pos.1, &text);
        }
        m.select_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 1);
    }

    fn draw_versus(&self, frame: &mut Frame) {
        let m = self.motif.as_ref().unwrap();
        let v = &m.vs;
        frame.clear = m.vs_bg.clear & 0x00ff_ffff;
        m.vs_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 0);
        for p in 0..2 {
            self.portrait(frame, self.chosen[p], &v.p[p], 0.0, 0.0);
            m.fonts.draw(frame, v.name_font[p], v.name_offset[p].0, v.name_offset[p].1, &m.chars[self.chosen[p]].name);
        }
        if self.mode == Mode::Arcade && !v.match_text.is_empty() {
            let text = v.match_text.replace("%i", &self.match_no.to_string());
            m.fonts.draw(frame, v.match_font, v.match_offset.0, v.match_offset.1, &text);
        }
        m.vs_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 1);
    }

    fn draw_victory(&self, frame: &mut Frame, winner: usize, name: &str, quote: &[String]) {
        let m = self.motif.as_ref().unwrap();
        let v = &m.victory;
        frame.clear = m.victory_bg.clear & 0x00ff_ffff;
        m.victory_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 0);
        self.portrait(frame, winner, &v.p1, 0.0, 0.0);
        m.fonts.draw(frame, v.name_font, v.name_offset.0, v.name_offset.1, name);
        let line_h = v.quote_font.and_then(|f| m.fonts.get(f.0)).map_or(10, |f| f.size.1 + f.spacing.1.max(1)) as f32;
        for (i, line) in quote.iter().enumerate() {
            m.fonts.draw(frame, v.quote_font, v.quote_offset.0, v.quote_offset.1 + i as f32 * line_h, line);
        }
        m.victory_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 1);
    }

    /// Genera la lista de dibujo y de sonidos del tick actual.
    pub fn draw(&self, frame: &mut Frame) {
        match &self.screen {
            Screen::Fight => {
                if let Some(g) = &self.game {
                    g.draw(frame);
                }
            }
            other => {
                frame.reset();
                frame.clear = 0;
                match other {
                    Screen::Story(s, _) => s.draw(frame, &self.sffs),
                    Screen::Title { sel, .. } => self.draw_title(frame, *sel),
                    Screen::Select(st) => self.draw_select(frame, st),
                    Screen::Versus => self.draw_versus(frame),
                    Screen::Victory { winner, name, quote } => self.draw_victory(frame, *winner, name, quote),
                    Screen::WinText => {
                        let m = self.motif.as_ref().unwrap();
                        frame.clear = m.victory_bg.clear & 0x00ff_ffff;
                        m.victory_bg.draw(frame, &self.sffs, &m.anims, [0.0, 0.0], 0.0, self.t, 0);
                        m.fonts.draw(frame, m.win.font, m.win.offset.0, m.win.offset.1, &m.win.text);
                    }
                    Screen::Fight => {}
                }
                frame.sounds.extend(self.sounds.iter().copied());
            }
        }
        // Fundidos de entrada y de salida, en negro.
        if self.fadein > 0 && self.t < self.fadein && !matches!(self.screen, Screen::Story(..)) {
            fade(frame, 0xff00_0000, 256 - 256 * self.t / self.fadein);
        }
        if let Some((t, time, _)) = &self.leaving {
            fade(frame, 0xff00_0000, 256 * (t + 1) / time);
        }
    }

    /// Nombre de la pantalla actual (para registros y pruebas).
    pub fn screen_name(&self) -> &'static str {
        match self.screen {
            Screen::Story(..) => "story",
            Screen::Title { .. } => "title",
            Screen::Select(_) => "select",
            Screen::Versus => "versus",
            Screen::Fight => "fight",
            Screen::Victory { .. } => "victory",
            Screen::WinText => "wintext",
        }
    }

    pub fn summary(&self) -> String {
        match (&self.screen, &self.game) {
            (Screen::Fight, Some(g)) => g.summary(),
            _ => format!("{} t{}", self.screen_name(), self.t),
        }
    }
}
