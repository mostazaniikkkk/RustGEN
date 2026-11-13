//! Barras, retratos, reloj y mensajes de ronda (`data/fight.def`), y el
//! desarrollo de las rondas: presentación, pelea, K.O. y victoria.

use crate::air::{self, AnimTable};
use crate::fnt::{font_spec, FontSet, FontSpec};
use crate::game::{Game, AS_INTRO, AS_NOBARDISPLAY, AS_NOKOSLOW, AS_ROUNDNOTOVER, AS_TIMERFREEZE};
use crate::gfx::{Frame, PalRef, SpriteCmd, Trans};
use crate::ini::{self, Section};
use crate::sff::Sff;
use crate::snd::Snd;

/// Elemento de la interfaz: una animación, un sprite o un texto.
#[derive(Default, Clone)]
struct Elem {
    anim: Option<i32>,
    spr: Option<(i32, i32)>,
    offset: (f32, f32),
    facing: f32,
    font: Option<FontSpec>,
    text: String,
    displaytime: i32,
    snd: Option<(i32, i32)>,
}

impl Elem {
    fn read(s: &Section, prefix: &str) -> Elem {
        let key = |k: &str| format!("{prefix}.{k}");
        let pair = |k: &str| s.get(&key(k)).map(|_| s.pair(&key(k), 0.0, 0.0));
        let font = font_spec(s, &key("font"));
        Elem {
            anim: s.get(&key("anim")).and_then(ini::num).map(|v| v as i32),
            spr: pair("spr").map(|(g, i)| (g as i32, i as i32)),
            offset: pair("offset").unwrap_or((0.0, 0.0)),
            facing: if s.i32(&key("facing"), 1) < 0 { -1.0 } else { 1.0 },
            font,
            text: s.str(&key("text")).unwrap_or_default(),
            displaytime: s.i32(&key("displaytime"), 0),
            snd: pair("snd").map(|(g, i)| (g as i32, i as i32)),
        }
    }
}

#[derive(Default, Clone)]
struct Bar {
    pos: (f32, f32),
    bg0: Elem,
    bg1: Elem,
    mid: Elem,
    front: Elem,
    counter: Elem,
    range: (f32, f32),
}

impl Bar {
    fn read(s: &Section, p: &str) -> Bar {
        Bar {
            pos: s.pair(&format!("{p}.pos"), 0.0, 0.0),
            bg0: Elem::read(s, &format!("{p}.bg0")),
            bg1: Elem::read(s, &format!("{p}.bg1")),
            mid: Elem::read(s, &format!("{p}.mid")),
            front: Elem::read(s, &format!("{p}.front")),
            counter: Elem::read(s, &format!("{p}.counter")),
            range: s.pair(&format!("{p}.range.x"), 0.0, 0.0),
        }
    }
}

pub struct Fight {
    pub sff: u16,
    pub snd: u16,
    pub common_snd: u16,
    pub fx_sff: u16,
    pub fx_anims: AnimTable,
    anims: AnimTable,
    fonts: FontSet,
    life: [Bar; 2],
    power: [Bar; 2],
    face_pos: [(f32, f32); 2],
    face_bg: [Elem; 2],
    face: [Elem; 2],
    name_pos: [(f32, f32); 2],
    name: [Elem; 2],
    time_pos: (f32, f32),
    time: Elem,
    frames_per_count: i32,
    combo_pos: [(f32, f32); 2],
    combo_counter: [Elem; 2],
    combo_text: [Elem; 2],
    combo_time: i32,
    icon_pos: [(f32, f32); 2],
    icon_offset: [(f32, f32); 2],
    icon_spr: [Elem; 2],
    match_wins: i32,
    start_waittime: i32,
    round_default: Elem,
    round_snd: Vec<Option<(i32, i32)>>,
    fight: Elem,
    ctrl_time: i32,
    ko: Elem,
    dko: Elem,
    to: Elem,
    slow_time: i32,
    over_waittime: i32,
    pub over_hittime: i32,
    over_wintime: i32,
    over_time: i32,
    win_time: i32,
    win: Elem,
    draw: Elem,
}

#[derive(Clone, Copy, PartialEq, Default, Debug)]
enum Phase {
    #[default]
    Intro,
    Announce,
    Fight,
    Over,
}

#[derive(Default)]
pub struct Round {
    /// Trigger `RoundState`: 0 antes de la intro, 1 intro, 2 pelea, 3 K.O.,
    /// 4 poses de victoria.
    pub state: i32,
    pub no: i32,
    phase: Phase,
    /// Ticks en la fase actual.
    pt: i32,
    /// Ticks de reloj que quedan.
    pub timer: i32,
    pub wins: [i32; 2],
    pub winner: Option<usize>,
    pub decided: bool,
    pub match_over: bool,
    pub slow: i32,
    pub over_time: i32,
    pub ko_by_hit: bool,
    /// 0 = K.O., 1 = doble K.O., 2 = tiempo.
    ending: u8,
    win_started: Option<i32>,
    combo_shown: [i32; 2],
    combo_t: [i32; 2],
    /// Peleas completas jugadas.
    pub matches: i32,
}

impl Fight {
    /// Carga el marcador de `path` (normalmente `data/fight.def`).
    pub fn load(root: &str, path: &str, sffs: &mut Vec<Sff>, snds: &mut Vec<Snd>) -> Result<Fight, String> {
        let text = ini::read_text(path).map_err(|e| format!("{path}: {e}"))?;
        let secs = ini::parse(&text);
        let empty = Section::default();
        let sec = |n: &str| secs.iter().find(|s| s.name == n).unwrap_or(&empty);
        let files = sec("files");
        let find = |f: &str| ini::find_file(root, path, f);
        let add_sff = |key: &str, sffs: &mut Vec<Sff>| -> Result<u16, String> {
            let f = files.str(key).ok_or(format!("fight.def: falta {key}"))?;
            sffs.push(Sff::load(&find(&f))?);
            Ok(sffs.len() as u16 - 1)
        };
        let sff = add_sff("sff", sffs)?;
        let fx_sff = add_sff("fightfx.sff", sffs)?;
        let add_snd = |key: &str, snds: &mut Vec<Snd>| -> u16 {
            let s = files.str(key).and_then(|f| Snd::load(&find(&f)).ok()).unwrap_or_default();
            snds.push(s);
            snds.len() as u16 - 1
        };
        let snd = add_snd("snd", snds);
        let common_snd = add_snd("common.snd", snds);
        let fx_anims = files.str("fightfx.air").and_then(|f| air::load(&find(&f)).ok()).unwrap_or_default();

        let fonts = FontSet::load(files, &find, sffs);

        let (lb, pb, fc, nm, tm, cb, wi, rd) =
            (sec("lifebar"), sec("powerbar"), sec("face"), sec("name"), sec("time"), sec("combo"), sec("winicon"), sec("round"));
        let two = |f: &dyn Fn(&str) -> Elem| [f("p1"), f("p2")];
        let mut round_snd = vec![None];
        for n in 1..=9 {
            let k = format!("round{n}.snd");
            round_snd.push(rd.get(&k).map(|_| rd.pair(&k, 0.0, 0.0)).map(|(g, i)| (g as i32, i as i32)));
        }
        Ok(Fight {
            sff,
            snd,
            common_snd,
            fx_sff,
            fx_anims,
            anims: air::parse(&text),
            fonts,
            life: [Bar::read(lb, "p1"), Bar::read(lb, "p2")],
            power: [Bar::read(pb, "p1"), Bar::read(pb, "p2")],
            face_pos: [fc.pair("p1.pos", 0.0, 0.0), fc.pair("p2.pos", 0.0, 0.0)],
            face_bg: two(&|p| Elem::read(fc, &format!("{p}.bg"))),
            face: two(&|p| Elem::read(fc, &format!("{p}.face"))),
            name_pos: [nm.pair("p1.pos", 0.0, 0.0), nm.pair("p2.pos", 0.0, 0.0)],
            name: two(&|p| Elem::read(nm, &format!("{p}.name"))),
            time_pos: tm.pair("pos", 160.0, 23.0),
            time: Elem::read(tm, "counter"),
            frames_per_count: tm.i32("framespercount", 60).max(1),
            combo_pos: [cb.pair("team1.pos", 10.0, 80.0), cb.pair("team2.pos", 309.0, 80.0)],
            combo_counter: [Elem::read(cb, "team1.counter"), Elem::read(cb, "team2.counter")],
            combo_text: [Elem::read(cb, "team1.text"), Elem::read(cb, "team2.text")],
            combo_time: cb.i32("team1.displaytime", 90),
            icon_pos: [wi.pair("p1.pos", 0.0, 0.0), wi.pair("p2.pos", 0.0, 0.0)],
            icon_offset: [wi.pair("p1.iconoffset", 0.0, 0.0), wi.pair("p2.iconoffset", 0.0, 0.0)],
            icon_spr: two(&|p| Elem::read(wi, &format!("{p}.n"))),
            match_wins: rd.i32("match.wins", 2),
            start_waittime: rd.i32("start.waittime", 30),
            round_default: Elem::read(rd, "round.default"),
            round_snd,
            fight: Elem::read(rd, "fight"),
            ctrl_time: rd.i32("ctrl.time", 30),
            ko: Elem::read(rd, "ko"),
            dko: Elem::read(rd, "dko"),
            to: Elem::read(rd, "to"),
            slow_time: rd.i32("slow.time", 60),
            over_waittime: rd.i32("over.waittime", 45),
            over_hittime: rd.i32("over.hittime", 10),
            over_wintime: rd.i32("over.wintime", 45),
            over_time: rd.i32("over.time", 210),
            win_time: rd.i32("win.time", 60),
            win: Elem::read(rd, "win"),
            draw: Elem::read(rd, "draw"),
        })
    }
}

impl Game {
    pub fn start_match(&mut self) {
        self.round.wins = [0, 0];
        self.round.match_over = false;
        for ch in self.chars.iter_mut() {
            ch.power = 0;
        }
        self.start_round(1);
    }

    fn start_round(&mut self, no: i32) {
        let matches = self.round.matches;
        let wins = self.round.wins;
        self.round = Round { no, wins, matches, timer: 99 * self.fight.frames_per_count, ..Default::default() };
        self.explods.clear();
        self.cam = [0.0, 0.0];
        self.pause = Default::default();
        self.shake = Default::default();
        for c in 0..self.chars.len() {
            let (start, facing) = (self.stage.p_start[c], self.stage.p_facing[c]);
            let ch = &mut self.chars[c];
            ch.pos = [start.0, start.1];
            ch.vel = [0.0, 0.0];
            ch.facing = facing;
            ch.life = ch.def.life;
            ch.life_lag = ch.life as f32;
            ch.ctrl = false;
            ch.state_owner = c;
            ch.stype = b's';
            ch.movetype = b'i';
            ch.physics = b's';
            ch.hitpause = 0;
            ch.ghv = Default::default();
            ch.bind = None;
            ch.targets.clear();
            ch.palfx = Default::default();
            ch.afterimage.time = 0;
            ch.afterimage.clear();
            ch.nothitby = [(0, 0, false); 2];
            ch.hitoverride = Default::default();
            ch.defence_mul = 1.0;
            ch.attack_mul = 1.0;
            ch.sprpriority = 0;
            ch.combo = 0;
            ch.airjumps = 0;
            ch.cmd.clear();
            self.change_state(c, 5900, Some(c));
        }
    }

    fn fight_sound(&mut self, s: Option<(i32, i32)>) {
        if let Some((g, n)) = s {
            self.play(self.fight.snd, g, n, -1, 1.0);
        }
    }

    /// Avanza el desarrollo de la ronda. Se llama al principio de cada tick.
    pub fn round_update(&mut self) {
        let f = self.fight.clone();
        let pt = self.round.pt;
        let announce_time = f.round_default.displaytime.max(1);
        match self.round.phase {
            Phase::Intro => {
                self.round.state = if pt < 10 { 0 } else { 1 };
                let intro = self.chars.iter().any(|c| c.has(AS_INTRO));
                if pt >= f.start_waittime && (!intro || pt > 900) {
                    self.round.phase = Phase::Announce;
                    self.round.pt = -1;
                    let snd = f.round_snd.get(self.round.no as usize).copied().flatten();
                    self.fight_sound(snd);
                }
            }
            Phase::Announce => {
                self.round.state = 1;
                if pt == announce_time {
                    let snd = f.fight.snd;
                    self.fight_sound(snd);
                }
                if pt >= announce_time + f.ctrl_time {
                    self.round.phase = Phase::Fight;
                    self.round.state = 2;
                    for ch in self.chars.iter_mut() {
                        ch.ctrl = true;
                    }
                }
            }
            Phase::Fight => {
                self.round.state = 2;
                let freeze = self.chars.iter().any(|c| c.has(AS_TIMERFREEZE));
                if self.round.timer > 0 && !freeze && !self.pause.on() {
                    self.round.timer -= 1;
                }
                if self.training {
                    // Entrenamiento: sin reloj, y la vida y el poder se reponen.
                    self.round.timer = 99 * f.frames_per_count;
                    for ch in self.chars.iter_mut() {
                        if ch.movetype != b'h' {
                            ch.life = ch.def.life;
                        }
                        ch.power = 3000;
                    }
                }
                let dead = [self.chars[0].life <= 0, self.chars[1].life <= 0];
                let time_up = self.round.timer == 0;
                let hold = self.chars.iter().any(|c| c.has(AS_ROUNDNOTOVER));
                if (dead[0] || dead[1] || time_up) && !hold {
                    let (l0, l1) = (self.chars[0].life, self.chars[1].life);
                    self.round.winner = match (dead[0], dead[1]) {
                        (true, true) => None,
                        (true, false) => Some(1),
                        (false, true) => Some(0),
                        _ if l0 > l1 => Some(0),
                        _ if l1 > l0 => Some(1),
                        _ => None,
                    };
                    self.round.ending = if dead[0] && dead[1] {
                        1
                    } else if dead[0] || dead[1] {
                        0
                    } else {
                        2
                    };
                    self.round.decided = true;
                    self.round.phase = Phase::Over;
                    self.round.pt = -1;
                    self.round.state = 3;
                    if let Some(w) = self.round.winner {
                        self.round.wins[w] += 1;
                    }
                    let noslow = self.chars.iter().any(|c| c.has(AS_NOKOSLOW));
                    if self.round.ending != 2 && !noslow {
                        self.round.slow = f.slow_time;
                    }
                    let snd = match self.round.ending {
                        0 => f.ko.snd,
                        1 => f.dko.snd,
                        _ => f.to.snd,
                    };
                    self.fight_sound(snd);
                }
            }
            Phase::Over => {
                self.round.over_time = pt;
                let (waittime, wintime, over_time, win_time) = (f.over_waittime, f.over_wintime, f.over_time, f.win_time);
                if pt == waittime {
                    for ch in self.chars.iter_mut() {
                        ch.ctrl = false;
                    }
                }
                if self.round.win_started.is_none() && pt >= wintime {
                    // Espera a que todos estén quietos (o tirados) para las poses.
                    let settled = self.chars.iter().all(|c| c.life <= 0 || (c.movetype == b'i' && c.stype == b's' && c.state_owner == c.idx))
                        && self.chars.iter().all(|c| c.life > 0 || c.stateno == 5150 || c.stype == b'l');
                    if settled || pt > wintime + 300 {
                        self.round.win_started = Some(pt);
                        self.round.state = 4;
                        self.round.match_over = self.round.wins.iter().any(|&w| w >= self.fight.match_wins);
                        for c in 0..self.chars.len() {
                            if self.chars[c].life <= 0 {
                                continue;
                            }
                            let next = match self.round.winner {
                                Some(w) if w == c => 180,
                                Some(_) => 170,
                                None => 175,
                            };
                            self.chars[c].ctrl = false;
                            self.change_state(c, next, Some(c));
                        }
                    }
                }
                if let Some(start) = self.round.win_started {
                    if pt >= over_time.max(start + win_time + 90) {
                        if self.round.match_over {
                            self.round.matches += 1;
                            if self.auto_rematch {
                                self.start_match();
                            } else {
                                // Quien lleva las pantallas decide qué sigue.
                                self.finished = true;
                            }
                        } else {
                            let no = self.round.no + 1;
                            self.start_round(no);
                        }
                        return;
                    }
                }
            }
        }
        self.round.pt += 1;

        // Contador de golpes seguidos.
        for c in 0..self.chars.len() {
            let combo = self.chars[c].combo;
            let r = &mut self.round;
            if combo >= 2 && combo != r.combo_shown[c] {
                r.combo_shown[c] = combo;
                r.combo_t[c] = 0;
            } else if r.combo_shown[c] > 0 {
                r.combo_t[c] += 1;
                if r.combo_t[c] > self.fight.combo_time {
                    r.combo_shown[c] = 0;
                }
            }
            let o = 1 - c;
            if self.chars[o].movetype != b'h' && self.chars[o].state_owner == o {
                self.chars[c].combo = 0;
            }
        }
    }

    // ------------------------------------------------------------ dibujo

    fn hud_sprite(&self, frame: &mut Frame, group: i32, image: i32, x: f32, y: f32, facing: f32, trans: Trans, clip: Option<[i32; 4]>) {
        let sff = &self.sffs[self.fight.sff as usize];
        if let Some(si) = sff.find(group, image) {
            let pal = PalRef::Sff(self.fight.sff, sff.sprites[si as usize].pal);
            frame.sprite(SpriteCmd { sff: self.fight.sff, spr: si, x, y, sx: facing, sy: 1.0, pal, trans, clip, src: None });
        }
    }

    /// Dibuja un elemento; `t` es el tiempo de su animación y `looping` si
    /// se repite. Devuelve `false` si la animación ya terminó.
    #[allow(clippy::too_many_arguments)]
    fn hud_elem(&self, frame: &mut Frame, e: &Elem, x: f32, y: f32, t: i32, looping: bool, clip: Option<[i32; 4]>) -> bool {
        let (x, y) = (x + e.offset.0, y + e.offset.1);
        if let Some(no) = e.anim {
            let Some(a) = self.fight.anims.get(&no) else { return false };
            let Some(f) = a.frame_at(t, looping) else { return false };
            self.hud_sprite(frame, f.group, f.image, x + f.x as f32 * e.facing, y + f.y as f32, e.facing * if f.hflip { -1.0 } else { 1.0 }, f.trans, clip);
            return true;
        }
        if let Some((g, i)) = e.spr {
            self.hud_sprite(frame, g, i, x, y, e.facing, Trans::None, clip);
        }
        true
    }

    fn hud_text(&self, frame: &mut Frame, e: &Elem, x: f32, y: f32, text: &str) {
        self.fight.fonts.draw(frame, e.font, x + e.offset.0, y + e.offset.1, text);
    }

    fn draw_bar(&self, frame: &mut Frame, b: &Bar, frac: f32, mid_frac: f32) {
        let (x, y) = b.pos;
        let t = self.tick;
        self.hud_elem(frame, &b.bg0, x, y, t, true, None);
        self.hud_elem(frame, &b.bg1, x, y, t, true, None);
        // La parte visible va de range.x[0] a range.x[0] + (ancho * fracción).
        let window = |f: f32| -> [i32; 4] {
            let end = b.range.0 + (b.range.1 - b.range.0) * f.clamp(0.0, 1.0);
            let (lo, hi) = if b.range.0 <= end { (b.range.0, end) } else { (end, b.range.0) };
            [(x + lo).round() as i32, 0, (x + hi).round() as i32 + 1, 240]
        };
        if mid_frac > frac {
            self.hud_elem(frame, &b.mid, x, y, t, true, Some(window(mid_frac)));
        }
        if frac > 0.0 {
            self.hud_elem(frame, &b.front, x, y, t, true, Some(window(frac)));
        }
    }

    pub fn draw_hud(&self, frame: &mut Frame) {
        let f = &self.fight;
        let r = &self.round;
        if self.finished {
            return;
        }
        if self.show_bars && !self.chars.iter().any(|c| c.has(AS_NOBARDISPLAY)) {
            for (i, ch) in self.chars.iter().enumerate() {
                let max = ch.def.life.max(1) as f32;
                self.draw_bar(frame, &f.life[i], ch.life as f32 / max, ch.life_lag / max);
                let level = ch.power / 1000;
                let frac = if ch.power >= 3000 { 1.0 } else { (ch.power % 1000) as f32 / 1000.0 };
                self.draw_bar(frame, &f.power[i], frac, 0.0);
                let pb = &f.power[i];
                self.hud_text(frame, &pb.counter, pb.pos.0, pb.pos.1, &level.to_string());

                // Retrato y nombre.
                let (fx, fy) = f.face_pos[i];
                self.hud_elem(frame, &f.face_bg[i], fx, fy, self.tick, true, None);
                let face = &f.face[i];
                if let Some((g, n)) = face.spr {
                    let sff = &self.sffs[ch.def.sff as usize];
                    if let Some(si) = sff.find(g, n) {
                        let sp = sff.sprites[si as usize].pal;
                        frame.sprite(SpriteCmd {
                            sff: ch.def.sff,
                            spr: si,
                            x: fx + face.offset.0,
                            y: fy + face.offset.1,
                            sx: face.facing,
                            sy: 1.0,
                            pal: PalRef::Sff(ch.def.sff, if sp == 0 { ch.pal } else { sp }),
                            trans: Trans::None,
                            clip: None,
                            src: None,
                        });
                    }
                }
                let (nx, ny) = f.name_pos[i];
                self.hud_text(frame, &f.name[i], nx, ny, &ch.def.display);

                // Rondas ganadas.
                for k in 0..r.wins[i] {
                    let (ix, iy) = f.icon_pos[i];
                    let (ox, oy) = f.icon_offset[i];
                    self.hud_elem(frame, &f.icon_spr[i], ix + ox * k as f32, iy + oy * k as f32, self.tick, true, None);
                }
            }
            let secs = (r.timer + f.frames_per_count - 1) / f.frames_per_count;
            self.hud_text(frame, &f.time, f.time_pos.0, f.time_pos.1, &format!("{:02}", secs.clamp(0, 99)));
        }

        // Combo.
        for i in 0..2 {
            let n = r.combo_shown[i];
            if n < 2 {
                continue;
            }
            let (x, y) = f.combo_pos[i];
            let num = n.to_string();
            let width = |e: &Elem, text: &str| f.fonts.width(e.font, text) as f32;
            let text = &f.combo_text[i].text;
            if i == 0 {
                // "3 Rush!" desde el borde izquierdo.
                self.hud_text(frame, &f.combo_counter[i], x, y, &num);
                self.hud_text(frame, &f.combo_text[i], x + width(&f.combo_counter[i], &num), y, text);
            } else {
                // Lo mismo, pegado al borde derecho.
                self.hud_text(frame, &f.combo_text[i], x, y, text);
                self.hud_text(frame, &f.combo_counter[i], x - width(&f.combo_text[i], text) - f.combo_text[i].offset.0, y, &num);
            }
        }

        // Mensajes de ronda.
        let pt = r.pt;
        let pos = |e: &Elem| (e.offset.0, e.offset.1);
        let plain = |e: &Elem| Elem { offset: (0.0, 0.0), ..e.clone() };
        match r.phase {
            Phase::Intro => {}
            Phase::Announce => {
                let e = &f.round_default;
                if pt < e.displaytime {
                    let text = e.text.replace("%i", &r.no.to_string());
                    let (x, y) = pos(e);
                    self.hud_text(frame, &plain(e), x, y, &text);
                } else {
                    let (x, y) = pos(&f.fight);
                    self.hud_elem(frame, &plain(&f.fight), x, y, pt - e.displaytime, false, None);
                }
            }
            Phase::Fight => {
                // La animación de "Fight!" puede seguir tras dar el control.
                let t = f.round_default.displaytime + f.ctrl_time + pt;
                let (x, y) = pos(&f.fight);
                self.hud_elem(frame, &plain(&f.fight), x, y, t - f.round_default.displaytime, false, None);
            }
            Phase::Over => {
                let e = match r.ending {
                    0 => &f.ko,
                    1 => &f.dko,
                    _ => &f.to,
                };
                let (x, y) = pos(e);
                let shown = if e.anim.is_some() { self.hud_elem(frame, &plain(e), x, y, pt, false, None) } else { pt < e.displaytime.max(60) };
                if shown && e.anim.is_none() {
                    self.hud_text(frame, &plain(e), x, y, &e.text);
                }
                if let Some(start) = r.win_started {
                    if pt >= start + f.win_time.min(30) {
                        let (e, text) = match r.winner {
                            Some(w) => (&f.win, f.win.text.replace("%s", &self.chars[w].def.display)),
                            None => (&f.draw, f.draw.text.clone()),
                        };
                        let (x, y) = pos(e);
                        self.hud_text(frame, &plain(e), x, y, &text);
                    }
                }
            }
        }
    }
}
