//! Pruebas del motor con los datos reales de Kung Fu Man (carpeta `game/`).

use mugen::cmd::{CommandSet, CommandState};
use mugen::expr;
use mugen::game::Game;
use mugen::gfx::{Frame, Soft};
use mugen::script::{parse_keys, Script};
use mugen::sff::Sff;
use mugen::snd::Snd;
use mugen::{air, ini};

const ROOT: &str = "../game";

fn game() -> Game {
    Game::new(ROOT, "kfm", "kfm", "kfm").expect("no se pudo cargar la pelea")
}

/// Ejecuta `ticks` con un guion de teclas y devuelve los estados por los que
/// pasó cada jugador (sin repetir consecutivos).
fn run(g: &mut Game, script: &Script, ticks: i32) -> [Vec<i32>; 2] {
    let mut seen = [Vec::new(), Vec::new()];
    for t in 0..ticks {
        g.tick(script.keys(t));
        for (i, list) in seen.iter_mut().enumerate() {
            let no = g.chars[i].stateno;
            if list.last() != Some(&no) {
                list.push(no);
            }
        }
    }
    seen
}

#[test]
fn sff_v1_and_v2_decode() {
    let v1 = Sff::load(&format!("{ROOT}/data/fight.sff")).unwrap();
    assert_eq!(v1.version, 1);
    assert_eq!(v1.sprites.len(), 43);
    assert!(!v1.is_streamed());
    assert!(v1.sprites.iter().all(|s| s.pix.len() == s.w as usize * s.h as usize && !s.pix.is_empty()));

    let v2 = Sff::load(&format!("{ROOT}/chars/kfm/kfm.sff")).unwrap();
    assert_eq!(v2.version, 2);
    assert_eq!(v2.sprites.len(), 281);
    assert!(v2.sprites.iter().all(|s| s.pix.len() == s.w as usize * s.h as usize && !s.pix.is_empty()));
    // El primer cuadro de la postura de pie: tamaño y eje conocidos.
    let stand = v2.get(0, 0).unwrap();
    assert!(stand.w > 30 && stand.h > 90, "{}x{}", stand.w, stand.h);
    // Seis paletas del personaje, la (1, 1) a la (1, 6).
    for n in 1..=6 {
        assert!(v2.pal_index.contains_key(&(1, n)));
    }
    // Un sprite no puede ser todo transparente.
    assert!(stand.pix.iter().any(|&p| p != 0));
}

#[test]
fn streamed_sprites_match_the_fully_loaded_ones() {
    for file in ["chars/kfm/kfm.sff", "data/fight.sff", "data/fightfx.sff", "stages/kfm.sff", "data/system.sff"] {
        let path = format!("{ROOT}/{file}");
        let full = Sff::load_mode(&path, Some(false)).unwrap();
        let lazy = Sff::load_mode(&path, Some(true)).unwrap();
        assert!(lazy.is_streamed() && !full.is_streamed());
        assert_eq!(full.sprites.len(), lazy.sprites.len());
        assert_eq!(full.decoded_size(), lazy.decoded_size());
        // Dos pasadas: la segunda vuelve a descomprimir lo que la caché soltó.
        for _ in 0..2 {
            for (i, s) in full.sprites.iter().enumerate() {
                let l = &lazy.sprites[i];
                assert_eq!((s.w, s.h, s.ax, s.ay, s.pal), (l.w, l.h, l.ax, l.ay, l.pal));
                assert!(l.pix.is_empty());
                let same = lazy.with_pixels(i as u32, |p| p == &s.pix[..]);
                assert_eq!(same, if s.pix.is_empty() { None } else { Some(true) }, "{file} sprite {i}");
            }
            // La caché nunca pasa de su tamaño (1 MB) más un sprite.
            let (_, bytes) = lazy.cache_stats();
            assert!(bytes <= (1 << 20) + 300 * 300, "{file}: {bytes} bytes en caché");
        }
    }
    // Kung Fu Man descomprimido ocupa más que la caché: hubo que soltar y
    // volver a descomprimir.
    let lazy = Sff::load_mode(&format!("{ROOT}/chars/kfm/kfm.sff"), Some(true)).unwrap();
    for _ in 0..2 {
        for i in 0..lazy.sprites.len() as u32 {
            lazy.with_pixels(i, |_| ());
        }
    }
    assert!(lazy.decoded_size() > 1 << 20);
    assert!(lazy.cache_stats().0 > lazy.sprites.len() as u64);
}

#[test]
fn sounds_and_animations_load() {
    let snd = Snd::load(&format!("{ROOT}/chars/kfm/kfm.snd")).unwrap();
    assert_eq!(snd.samples.len(), 12);
    assert!(snd.samples.iter().all(|s| s.rate >= 8000 && s.frames > 0));
    assert_eq!(snd.with_pcm(0, |pcm| pcm.len()), Some(snd.samples[0].frames));

    let anims = air::load(&format!("{ROOT}/chars/kfm/kfm.air")).unwrap();
    assert_eq!(anims.len(), 117);
    let stand = &anims[&0];
    assert_eq!(stand.frames.len(), 11);
    assert_eq!(stand.total, 151);
    // Todos los cuadros de pie tienen cajas donde recibir golpes.
    assert!(stand.frames.iter().all(|f| !f.clsn2.is_empty()));
    // El puñetazo tiene caja de ataque en algún cuadro.
    assert!(anims[&200].frames.iter().any(|f| !f.clsn1.is_empty()));
    // Salto: se repite desde un elemento intermedio.
    assert_eq!(anims[&41].loopstart, 7);
}

#[test]
fn expression_syntax() {
    for src in [
        "Time = 0",
        "AnimElem = 2, >= 0",
        "AnimElem = 3, -1",
        "Time = command = \"holdfwd\"",
        "command != \"holddown\" && ctrl",
        "GetHitVar(animtype) != [3,5]",
        "Vel Y > Const720p(-8)",
        "ifelse(sysvar(1)=0, const(velocity.jump.neu.x), ifelse(sysvar(1)=1, const(velocity.jump.fwd.x), const(velocity.jump.back.x)))",
        "120 + (statetype = C) + (statetype = A)*2",
        "P2BodyDist X < 3 && p2statetype != A",
        "HitDefAttr = SC, NA, SA, HA",
        "var(5) := 3 + 4",
        "floor (screenpos y) - 50",
        "(pos y >= 0) && (vel y > 0)",
        "enemy, life < 100",
        "Time = [3, 9)",
        ".5 * -2 ** 2",
    ] {
        expr::parse(src).unwrap_or_else(|e| panic!("`{src}`: {e}"));
    }
    assert!(expr::parse("Time = ").is_err());
    assert!(expr::parse("noexiste(3)").is_err());
    assert_eq!(expr::split_args("1, ifelse(a, b, c), \"x,y\""), vec!["1", "ifelse(a, b, c)", "\"x,y\""]);
}

#[test]
fn kfm_loads_without_warnings() {
    let g = game();
    assert!(g.warnings.is_empty(), "{:#?}", g.warnings);
    let def = &g.chars[0].def;
    assert_eq!(def.name, "Kung Fu Man");
    assert_eq!(def.life, 1000);
    assert!(def.states.len() > 100, "{} estados", def.states.len());
    // Los comunes (common1.cns), los del personaje y los del CMD.
    for no in [-1, 0, 20, 40, 200, 1000, 3000, 5000, 5900] {
        assert!(def.states.contains_key(&no), "falta el estado {no}");
    }
    assert_eq!(def.consts["velocity.walk.fwd.x"], 2.4);
    assert_eq!(def.consts["velocity.jump.y"], -8.4);
    // Los dos jugadores comparten datos pero usan paletas distintas.
    assert_ne!(g.chars[0].pal, g.chars[1].pal);
}

#[test]
fn commands_are_recognised() {
    let (set, _) = CommandSet::load(&format!("{ROOT}/chars/kfm/kfm.cmd")).unwrap();
    let mut st = CommandState::new(&set);
    let feed = |st: &mut CommandState, keys: &str, ticks: usize| {
        for _ in 0..ticks {
            st.update(&set, CommandState::relative(parse_keys(keys), 1));
        }
    };
    // Cuarto de círculo adelante + x.
    feed(&mut st, "", 2);
    feed(&mut st, "D", 3);
    feed(&mut st, "DR", 3);
    feed(&mut st, "R", 3);
    feed(&mut st, "Rx", 1);
    assert!(st.is_active(&set, "qcf_x"));
    assert!(st.is_active(&set, "x"));
    assert!(st.is_active(&set, "holdfwd"));
    assert!(!st.is_active(&set, "qcf_y"));
    // Sin el movimiento, sólo el botón.
    st.clear();
    feed(&mut st, "", 20);
    feed(&mut st, "x", 1);
    assert!(st.is_active(&set, "x") && !st.is_active(&set, "qcf_x"));
    // Dos toques adelante.
    st.clear();
    feed(&mut st, "", 2);
    feed(&mut st, "R", 2);
    feed(&mut st, "", 2);
    feed(&mut st, "R", 1);
    assert!(st.is_active(&set, "ff"));
    // Mirando a la izquierda, "adelante" es la izquierda.
    st.clear();
    st.update(&set, CommandState::relative(parse_keys("L"), -1));
    assert!(st.is_active(&set, "holdfwd") && !st.is_active(&set, "holdback"));
}

#[test]
fn scripted_moves_reach_the_right_states() {
    let mut g = game();
    g.chars[1].ai = false;
    let script = Script::load("../tests/p1.keys").unwrap();
    let [p1, p2] = run(&mut g, &script, 720);
    // Intro, caminar, puñetazos, agachado, patada baja, salto con golpe,
    // especial, carrera y salto atrás.
    for no in [191, 0, 20, 200, 210, 10, 11, 430, 12, 40, 50, 630, 52, 1000, 100, 105] {
        assert!(p1.contains(&no), "el jugador 1 no pasó por el estado {no}: {p1:?}");
    }
    // El rival, sin moverse, recibió golpes.
    assert!(p2.iter().any(|&s| (5000..5200).contains(&s)), "{p2:?}");
    assert!(g.chars[1].life < 1000);
    assert_eq!(g.chars[0].life, 1000);
}

#[test]
fn throw_binds_and_damages() {
    let mut g = game();
    g.chars[1].ai = false;
    let script = Script::load("../tests/throw.keys").unwrap();
    let [p1, p2] = run(&mut g, &script, 520);
    assert!(p1.contains(&800) && p1.contains(&810), "{p1:?}");
    // El agarrado pasa por los estados del atacante y vuelve a los suyos.
    assert!(p2.contains(&820) && p2.contains(&821) && p2.contains(&5120), "{p2:?}");
    assert_eq!(g.chars[1].life, 922);
    assert_eq!(g.chars[1].stateno, 0);
    assert_eq!(g.chars[1].state_owner, 1);
}

#[test]
fn super_move_pauses_the_fight() {
    let mut g = game();
    g.chars[1].ai = false;
    let script = Script::load("../tests/super.keys").unwrap();
    let mut paused = 0;
    let mut seen = Vec::new();
    for t in 0..520 {
        if t == 200 {
            g.chars[0].power = 3000;
        }
        g.tick(script.keys(t));
        paused += g.pause.on() as i32;
        if seen.last() != Some(&g.chars[0].stateno) {
            seen.push(g.chars[0].stateno);
        }
    }
    assert!(seen.contains(&3000), "{seen:?}");
    // La superpausa dura 30 ticks y el especial cuesta un nivel de poder.
    assert_eq!(paused, 30);
    assert!(g.chars[0].power < 3000 && g.chars[0].power >= 2000, "poder {}", g.chars[0].power);
    assert!(g.chars[1].life < 900, "vida del rival {}", g.chars[1].life);
}

#[test]
fn rounds_ko_and_new_match() {
    let mut g = game();
    g.chars[0].ai = true;
    g.chars[1].ai = true;
    let mut rounds = vec![1];
    let mut saw_ko = false;
    let mut saw_win_pose = false;
    for _ in 0..40000 {
        g.tick([0, 0]);
        if rounds.last() != Some(&g.round.no) {
            rounds.push(g.round.no);
        }
        saw_ko |= g.round.state == 3 && g.chars.iter().any(|c| c.life == 0);
        saw_win_pose |= g.chars.iter().any(|c| c.stateno == 180 || c.stateno == 181);
        if g.round.matches >= 1 {
            break;
        }
    }
    assert!(saw_ko && saw_win_pose);
    assert!(g.round.matches >= 1, "la pelea no terminó: rondas {rounds:?}");
    // Tras la pelea se empieza de nuevo en la ronda 1.
    assert!(rounds.len() >= 3 && *rounds.last().unwrap() == 1, "{rounds:?}");
}

#[test]
fn time_over_is_a_draw() {
    let mut g = game();
    g.chars[1].ai = false;
    let mut states = Vec::new();
    for _ in 0..6400 {
        g.tick([0, 0]);
        if states.last() != Some(&g.round.state) {
            states.push(g.round.state);
        }
    }
    assert_eq!(states, vec![0, 1, 2, 3, 4, 0, 1]);
    assert_eq!(g.round.no, 2);
    assert_eq!(g.round.wins, [0, 0]);
}

#[test]
fn simulation_is_deterministic_and_draws() {
    let fight = || {
        let mut g = game();
        g.chars[0].ai = true;
        let mut frame = Frame::default();
        let mut soft = Soft::new();
        let mut log = Vec::new();
        for t in 0..1500 {
            g.tick([0, 0]);
            if t % 100 == 0 {
                g.draw(&mut frame);
                soft.render(&frame, &g.sffs);
                // Suma de control del frame.
                let sum = soft.px.iter().fold(0u64, |a, &p| a.wrapping_mul(31).wrapping_add(p as u64));
                log.push((g.summary(), sum, frame.cmds.len()));
            }
        }
        log
    };
    let (a, b) = (fight(), fight());
    assert_eq!(a, b);
    // Hay bastante que dibujar: fondo, personajes y marcador.
    assert!(a.iter().all(|(_, _, cmds)| *cmds > 60));
    // Y la pelea avanza: la vida de alguien baja.
    assert!(a.last().unwrap().0 != a[2].0);
}

#[test]
fn ini_numbers() {
    assert_eq!(ini::num("  -2.5 "), Some(-2.5));
    assert_eq!(ini::num(".75"), Some(0.75));
    assert_eq!(ini::num("12abc"), Some(12.0));
    assert_eq!(ini::num("abc"), None);
    assert_eq!(ini::clean("  value = 3 ; comentario"), "value = 3");
    assert_eq!(ini::clean("name = \"a;b\" ; fuera"), "name = \"a;b\"");
    let secs = ini::parse("[Info]\nname = \"Kung Fu Man\"\npos = 1, ,3\n");
    assert_eq!(secs[1].str("name").unwrap(), "Kung Fu Man");
    assert_eq!(secs[1].list("pos", 9.0), vec![1.0, 9.0, 3.0]);
}

// ---------------------------------------------------------------- screenpack

use mugen::app::App;
use mugen::cmd::{IN_S, IN_SELECT};

/// Ejecuta el juego completo con un guion y devuelve las pantallas visitadas.
fn screens(app: &mut App, script: &Script, ticks: i32) -> Vec<&'static str> {
    let mut seen = Vec::new();
    let mut frame = Frame::default();
    for t in 0..ticks {
        app.tick(script.keys(t));
        if t % 7 == 0 {
            // Dibujar no debe fallar en ninguna pantalla ni fundido.
            app.draw(&mut frame);
            assert!(!frame.cmds.is_empty() || app.screen_name() == "story");
        }
        if seen.last() != Some(&app.screen_name()) {
            seen.push(app.screen_name());
        }
    }
    seen
}

#[test]
fn motif_loads_roster_and_stages() {
    let app = App::with_motif(ROOT, "data/system.def").unwrap();
    let m = app.motif.as_ref().unwrap();
    assert_eq!(m.chars.len(), 1);
    assert_eq!(m.chars[0].name, "Kung Fu Man");
    // Retrato pequeño y grande del personaje.
    assert!(m.chars[0].portraits[0].is_some() && m.chars[0].portraits[1].is_some());
    assert_eq!(m.slots.len(), 2);
    assert_eq!(m.listed_stages().len(), 2);
    // ARCADE, VS MODE, TRAINING, WATCH y OPTIONS.
    assert_eq!(m.title.items.len(), 5);
    assert_eq!(app.screen_name(), "title");
}

#[test]
fn versus_mode_from_title_to_fight() {
    let mut app = App::with_motif(ROOT, "data/system.def").unwrap();
    let script = Script::load("../tests/menu.keys").unwrap();
    let seen = screens(&mut app, &script, 700);
    assert_eq!(seen, vec!["title", "select", "versus", "fight"]);
    let g = app.game.as_ref().unwrap();
    // Sin segundo mando, al jugador 2 lo lleva la máquina; el 1 es humano.
    assert!(g.chars[1].ai && !g.chars[0].ai);
    // El mismo personaje con paletas distintas (x = 1, y = 2).
    assert_ne!(g.chars[0].pal, g.chars[1].pal);
    assert_eq!(app.sffs().len(), g.sffs.len());
}

#[test]
fn watch_mode_runs_a_whole_cycle() {
    let mut app = App::with_motif(ROOT, "data/system.def").unwrap();
    let script = Script::load("../tests/watch.keys").unwrap();
    let seen = screens(&mut app, &script, 40000);
    assert!(seen.starts_with(&["title", "select", "versus", "fight", "victory", "select"]), "{seen:?}");
}

#[test]
fn kfm_motif_arcade_with_storyboards() {
    let mut app = App::with_motif(ROOT, "data/kfm/system.def").unwrap();
    assert_eq!(app.screen_name(), "story");
    let script = Script::load("../tests/arcade.keys").unwrap();
    let seen = screens(&mut app, &script, 800);
    // Logo e intro (saltados), título, selección, intro del personaje,
    // versus y pelea.
    assert_eq!(seen, vec!["story", "title", "select", "story", "versus", "fight"]);
}

#[test]
fn training_mode_never_ends_and_fight_can_be_left() {
    let mut app = App::with_motif(ROOT, "data/system.def").unwrap();
    // Título: bajar dos veces hasta TRAINING; elegir a los dos y el escenario.
    // En la pelea, caminar hasta el rival y darle tres puñetazos.
    let script = Script::parse("60 1 D 2
70 1 D 2
80 1 x 2
140 1 x 2
200 1 y 2
260 1 a 2
640 1 R 60
710 1 x 2
760 1 y 2
820 1 x 2
");
    let mut hit = false;
    for t in 0..1500 {
        app.tick(script.keys(t));
        if let Some(g) = &app.game {
            hit |= g.chars[1].movetype == b'h';
        }
    }
    assert_eq!(app.screen_name(), "fight");
    let g = app.game.as_ref().unwrap();
    assert!(g.training && !g.chars[1].ai);
    // El rival recibió golpes pero recupera la vida, y el reloj no corre.
    assert!(hit, "el rival no recibió ningún golpe");
    assert_eq!(g.chars[1].life, 1000);
    assert_eq!(g.round.timer, 99 * 60);
    assert_eq!(g.chars[0].power, 3000);
    // START + SELECT mantenidos: vuelta al título.
    for _ in 0..80 {
        app.tick([IN_S | IN_SELECT, 0]);
    }
    assert_eq!(app.screen_name(), "title");
    assert!(app.game.is_none());
}

#[test]
fn two_pads_select_at_the_same_time_and_demo_mode_starts() {
    let mut app = App::with_motif(ROOT, "data/system.def").unwrap();
    app.pad2 = true;
    // VS MODE; cada jugador elige con su mando en el mismo tick.
    let script = Script::parse("60 1 D 2\n80 1 x 2\n150 1 x 2\n150 2 b 2\n200 2 a 2\n");
    let seen = screens(&mut app, &script, 500);
    assert_eq!(seen, vec!["title", "select", "versus", "fight"]);
    assert!(!app.game.as_ref().unwrap().chars[1].ai);

    // Sin tocar nada en el título, a los 600 ticks empieza una demostración.
    let mut app = App::with_motif(ROOT, "data/system.def").unwrap();
    let seen = screens(&mut app, &Script::parse(""), 2300);
    assert_eq!(seen, vec!["title", "fight", "title"]);
}

// ------------------------------------------------- helpers y proyectiles

/// Kung Fu Man con estados de prueba: `Start` lanza un proyectil y
/// `Abajo + Start` llama a un ayudante (ver `tests/chars/kfmx`).
const KFMX: &str = "../tests/chars/kfmx/kfmx.def";

fn game_x(p2: &str) -> Game {
    let mut g = Game::new(ROOT, KFMX, p2, "kfm").expect("no se pudo cargar la pelea de prueba");
    assert!(g.warnings.is_empty(), "{:#?}", g.warnings);
    g.chars[0].ai = false;
    g.chars[1].ai = false;
    g
}

fn var(g: &Game, c: usize, i: usize) -> i32 {
    g.chars[c].ivar[i].get()
}

#[test]
fn helper_and_projectile_triggers_parse() {
    for src in [
        "ProjHit1234 = 1",
        "ProjContact = 1, < 20",
        "ProjGuarded7 = 0, >= 3",
        "!ProjContact5",
        "ProjHitTime(7) = [0, 10]",
        "NumHelper(55) = 0 && NumProjID(7) < 2 && NumProj = 0",
        "IsHelper && !IsHelper(3)",
        "parent, var(3) > 0",
        "root, StateNo = 200",
        "helper(55), Pos X > 0",
        "playerid(57), life",
        "PlayerIDExist(ID)",
        "ParentDist X < 20 && RootDist Y = 0",
        "ifelse(ProjHit7 = 1, 3, 4)",
    ] {
        expr::parse(src).unwrap_or_else(|e| panic!("`{src}`: {e}"));
    }
}

#[test]
fn projectile_flies_hits_and_reports() {
    let mut g = game_x("kfm");
    let script = Script::parse("200 1 s 2");
    let (mut max_projs, mut front, mut guard) = (0, 0.0f32, false);
    let mut p2 = Vec::new();
    for t in 0..330 {
        g.tick(script.keys(t));
        max_projs = max_projs.max(g.projs.len());
        if let Some(p) = g.projs.first() {
            front = front.max(p.pos[0]);
            assert_eq!(g.count_projs(0, 7), 1);
            assert_eq!(g.count_projs(0, 8), 0);
            // Los proyectiles son del jugador 1: el 2 no tiene ninguno.
            assert_eq!(g.count_projs(1, -1), 0);
        }
        guard |= g.in_guard_dist(1);
        if p2.last() != Some(&g.chars[1].stateno) {
            p2.push(g.chars[1].stateno);
        }
    }
    assert_eq!(max_projs, 1);
    // Salió de delante del jugador 1 (x = -70) y avanzó hacia el 2.
    assert!(front > -20.0, "el proyectil llegó hasta x = {front}");
    assert!(guard, "el rival nunca estuvo a distancia de guardia del proyectil");
    assert!(g.projs.is_empty(), "el proyectil sigue en juego");
    // 40 de daño, el rival pasó por el estado de golpeado y el jugador se
    // enteró (ProjHit7) y ganó poder sin moverse de su sitio.
    assert_eq!(g.chars[1].life, 960);
    assert!(p2.contains(&5000), "{p2:?}");
    assert_eq!(var(&g, 0, 12), 1);
    assert_eq!(var(&g, 0, 13), 0);
    assert_eq!(g.chars[0].power, 28);
    assert_eq!(g.chars[0].pos[0], -70.0);
    assert_eq!(g.chars[0].stateno, 0);
    assert!(g.chars[0].targets.is_empty());
    assert_eq!(g.proj_time(0, 7, mugen::ents::PROJ_GUARDED), -1);
    assert!(g.proj_time(0, 7, mugen::ents::PROJ_HIT) > 30);
}

#[test]
fn projectile_can_be_guarded() {
    let mut g = game_x("kfm");
    // El jugador 2 mira a la izquierda: "atrás" es la derecha.
    let script = Script::parse("200 1 s 2\n205 2 R 120");
    let [_, p2] = run(&mut g, &script, 330);
    assert!(p2.contains(&120) && p2.contains(&150), "{p2:?}");
    assert_eq!(g.chars[1].life, 995);
    assert_eq!((var(&g, 0, 12), var(&g, 0, 13)), (0, 1));
    assert!(g.projs.is_empty());
}

#[test]
fn projectile_leaves_when_its_time_runs_out() {
    let mut g = game_x("kfm");
    let script = Script::parse("200 1 s 2");
    // Con el rival fuera de su alcance sólo lo retira su tiempo (150 ticks)
    // o salirse de la pantalla.
    let mut alive = 0;
    for t in 0..420 {
        if t >= 200 {
            g.chars[1].pos = [150.0, -200.0];
            g.chars[1].vel = [0.0, 0.0];
        }
        g.tick(script.keys(t));
        alive += g.projs.len();
    }
    assert!(g.projs.is_empty());
    assert!((30..=150).contains(&alive), "el proyectil vivió {alive} ticks");
    assert_eq!(g.chars[1].life, 1000);
    assert_eq!(var(&g, 0, 12), 0);
}

#[test]
fn projectiles_of_both_sides_cancel_each_other() {
    let mut g = game_x(KFMX);
    let script = Script::parse("200 1 s 2\n200 2 s 2");
    let mut max_projs = 0;
    for t in 0..330 {
        g.tick(script.keys(t));
        max_projs = max_projs.max(g.projs.len());
    }
    assert_eq!(max_projs, 2);
    assert!(g.projs.is_empty());
    // Misma prioridad: se anulan los dos y nadie recibe daño.
    assert_eq!((g.chars[0].life, g.chars[1].life), (1000, 1000));
    assert_eq!((var(&g, 0, 15), var(&g, 1, 15)), (1, 1));
    assert_eq!((var(&g, 0, 12), var(&g, 1, 12)), (0, 0));
}

#[test]
fn helper_walks_hits_and_destroys_itself() {
    let mut g = game_x("kfm");
    let script = Script::parse("200 1 D 12\n204 1 s 2\n420 1 D 12\n424 1 s 2");
    let mut states = Vec::new();
    let (mut seen, mut first_uid, mut frame, mut soft) = (0, 0, Frame::default(), Soft::new());
    for t in 0..400 {
        g.tick(script.keys(t));
        if let Some(h) = g.chars.get(2).filter(|h| h.active) {
            seen += 1;
            first_uid = h.uid;
            assert!(h.helper && h.helper_id == 55 && h.parent == 0 && h.root == 0 && h.team == 0);
            assert_eq!(h.scale, (0.8, 0.8));
            assert_eq!(g.helpers_of(0, 55).count(), 1);
            assert_eq!(g.helpers_of(0, 3).count(), 0);
            assert_eq!(g.helpers_of(1, 0).count(), 0);
            if states.last() != Some(&h.stateno) {
                states.push(h.stateno);
            }
            // El jugador sigue al ayudante con `helper(55), StateNo` (lo lee
            // antes de que el ayudante actúe: va un tick por detrás).
            if seen > 2 {
                assert!(states.contains(&var(&g, 0, 14)), "var(14) = {}", var(&g, 0, 14));
            }
            // Se dibuja (con su sombra) sin estorbar a nadie.
            g.draw(&mut frame);
            soft.render(&frame, &g.sffs);
        }
        assert!(g.chars.len() <= 3);
    }
    assert_eq!(states, vec![7200, 7210]);
    assert!(seen > 20, "el ayudante vivió {seen} ticks");
    // Se identificó ante su creador y le fue contando el tiempo.
    assert_eq!(var(&g, 0, 10), first_uid);
    assert!(first_uid >= 58);
    assert!(var(&g, 0, 11) > 10);
    // Golpeó (30 de daño, a nombre del jugador) y se retiró.
    assert_eq!(g.chars[1].life, 970);
    assert_eq!(g.chars[0].power, 21);
    assert!(g.chars.iter().all(|c| !c.helper || !c.active));
    assert_eq!(g.helpers_of(0, 0).count(), 0);
    assert!(g.chars[0].targets.is_empty() && g.chars[1].bind.is_none());

    // El segundo ayudante ocupa el hueco del primero, con otro ID.
    for t in 400..470 {
        g.tick(script.keys(t));
    }
    assert_eq!(g.chars.len(), 3);
    assert!(g.chars[2].active && g.chars[2].uid > first_uid);
    // Una ronda nueva empieza sin ayudantes ni proyectiles.
    g.start_match();
    assert_eq!(g.chars.len(), 2);
    assert!(g.projs.is_empty());
}

#[test]
fn a_hit_helper_destroys_itself_and_the_fight_goes_on() {
    let mut g = game_x("kfm");
    // El jugador 2 recibe al ayudante a patadas.
    let script = Script::parse("200 1 D 12\n204 1 s 2\n222 2 b 2\n232 2 b 2\n242 2 b 2\n252 2 b 2\n262 2 b 2");
    let (mut helper_ticks, mut was_hit) = (0, false);
    for t in 0..520 {
        g.tick(script.keys(t));
        helper_ticks += g.chars.iter().filter(|c| c.active && c.helper).count();
        was_hit |= g.chars.iter().any(|c| c.active && c.helper && c.movetype == b'h');
    }
    assert!(helper_ticks > 0);
    assert!(was_hit, "el ayudante no llegó a recibir el golpe");
    assert_eq!(g.chars[1].life, 1000);
    assert!(g.chars.iter().all(|c| !c.helper || !c.active));
    // Nadie se quedó colgado apuntando al ayudante desaparecido.
    for c in 0..2 {
        assert_eq!(g.chars[c].stateno, 0, "jugador {c}");
        assert_eq!(g.chars[c].state_owner, c);
        assert!(g.chars[c].targets.is_empty());
    }
    assert_eq!(g.round.state, 2);
}

#[test]
fn helpers_and_projectiles_are_deterministic() {
    let script = Script::parse("200 1 s 2\n260 1 D 12\n264 1 s 2\n300 2 s 2\n380 1 s 2\n381 2 D 12\n385 2 s 2");
    let play = || {
        let mut g = game_x(KFMX);
        let mut log = Vec::new();
        for t in 0..900 {
            if t == 450 {
                g.chars[1].ai = true;
            }
            g.tick(script.keys(t));
            log.push(g.summary());
        }
        log
    };
    let (a, b) = (play(), play());
    assert_eq!(a, b);
    assert!(a.iter().any(|l| l.contains(" h1 ")), "nunca hubo un ayudante");
    assert!(a.iter().any(|l| l.ends_with("p1") || l.ends_with("p2")), "nunca hubo un proyectil");
}

// ------------------------------------------------------------- opciones

use mugen::options::{Options, PadLayout};

#[test]
fn options_menu_changes_rules_controls_and_speed() {
    let mut app = App::with_motif(ROOT, "data/system.def").unwrap();
    let script = Script::load("../tests/options.keys").unwrap();
    let mut seen = Vec::new();
    let mut saved = Vec::new();
    let mut frame = Frame::default();
    let (mut fight_frames, mut p1) = (0, Vec::new());
    for t in 0..1400 {
        app.tick(script.keys(t));
        app.draw(&mut frame);
        saved.extend(app.take_options_to_save());
        if seen.last() != Some(&app.screen_name()) {
            seen.push(app.screen_name());
        }
        if let ("fight", Some(g)) = (app.screen_name(), app.game.as_ref()) {
            fight_frames += 1;
            if p1.last() != Some(&g.chars[0].stateno) {
                p1.push(g.chars[0].stateno);
            }
        }
    }
    assert_eq!(seen, vec!["title", "options", "title", "select", "versus", "fight"]);
    let o = app.options().clone();
    assert_eq!((o.difficulty, o.life, o.time, o.speed, o.volume), (6, 80, 90, 3, 90));
    // Los botones a y b del jugador 1 quedaron intercambiados; el 2, intacto.
    assert_eq!(o.keys[0], [0, 1, 2, 3, 5, 4, 6, 7, 8, 9, 10]);
    assert_eq!(o.keys[1], PadLayout::GENERIC.default);
    // Se pidió guardar una sola vez, al salir del menú, con lo elegido.
    assert_eq!(saved.len(), 1);
    let mut back = Options::new(&PadLayout::GENERIC);
    back.apply_text(&saved[0]);
    assert_eq!(back, o);

    let g = app.game.as_ref().unwrap();
    // Vida al 80 %, reloj de 90 segundos y dificultad 6.
    assert_eq!((g.chars[0].life_max, g.chars[1].life_max), (800, 800));
    assert!(g.chars[0].life <= 800);
    assert_eq!((g.round_secs, g.ai_level), (90, 6));
    // Velocidad "Fast 3": 69 ticks de pelea por cada 60 frames.
    let expected = fight_frames * 69 / 60;
    assert!((g.tick - expected).abs() <= 2, "{} ticks en {fight_frames} frames", g.tick);
    // El botón b da la patada floja (estado 230) y el a la fuerte (240).
    let (weak, strong) = (p1.iter().position(|&s| s == 230), p1.iter().position(|&s| s == 240));
    assert!(weak.is_some() && strong.is_some() && weak < strong, "{p1:?}");
    // El volumen baja todo lo que suena.
    assert!(frame.sounds.iter().all(|s| s.vol <= 0.9001));
}

#[test]
fn options_defaults_cancel_and_other_screenpack() {
    let mut app = App::with_motif(ROOT, "data/kfm/system.def").unwrap();
    let mut frame = Frame::default();
    // Espera al título (este screenpack tiene logo e intro) y entra en OPTIONS.
    let mut step = |app: &mut App, keys: &str, n: i32| {
        for i in 0..n {
            app.tick([if i < 2 { parse_keys(keys) } else { 0 }, 0]);
            app.draw(&mut frame);
        }
    };
    for _ in 0..1000 {
        if app.screen_name() == "title" {
            break;
        }
        step(&mut app, "x", 4);
    }
    assert_eq!(app.screen_name(), "title");
    step(&mut app, "", 40);
    let items = app.motif.as_ref().unwrap().title.items.len();
    for _ in 0..items - 1 {
        step(&mut app, "D", 6);
    }
    step(&mut app, "x", 40);
    assert_eq!(app.screen_name(), "options");
    // Cambia la dificultad y empieza a asignar botones del jugador 2…
    step(&mut app, "L", 6);
    assert_eq!(app.options().difficulty, 3);
    for _ in 0..6 {
        step(&mut app, "D", 6);
    }
    step(&mut app, "x", 6);
    step(&mut app, "z", 6);
    step(&mut app, "z", 6);
    assert_eq!(app.options().keys[1][..2], [9, 9]);
    assert!(app.summary().ends_with("p2:2"), "{}", app.summary());
    // …y lo cancela con SELECT: los botones vuelven a como estaban.
    app.tick([IN_SELECT, 0]);
    step(&mut app, "", 6);
    assert_eq!(app.options().keys[1], PadLayout::GENERIC.default);
    assert_eq!(app.screen_name(), "options");
    // "Default Values" lo deja todo de fábrica, y al salir sin cambios
    // respecto a la entrada no hay nada que guardar.
    step(&mut app, "D", 6);
    step(&mut app, "x", 6);
    assert_eq!(*app.options(), Options::new(&PadLayout::GENERIC));
    app.tick([IN_SELECT, 0]);
    step(&mut app, "", 60);
    assert_eq!(app.screen_name(), "title");
    assert!(app.take_options_to_save().is_none());
}

#[test]
fn remapped_buttons_reach_the_fight_and_menus_keep_the_factory_ones() {
    let mut app = App::direct(ROOT, "kfm", "kfm", "kfm").unwrap();
    app.set_layout(PadLayout::PS2);
    let mut o = app.options().clone();
    // L1 (botón 10) pasa a ser el puñetazo flojo, en vez de cuadrado (15).
    o.keys[0][7] = 10;
    app.set_options(o);
    let mut seen = Vec::new();
    for t in 0..400 {
        let raw = match t {
            200..=201 => 1 << 15,
            260..=261 => 1 << 10,
            _ => 0,
        };
        app.tick([raw, 0]);
        let no = app.game.as_ref().unwrap().chars[0].stateno;
        if seen.last() != Some(&no) {
            seen.push((no, t).0);
        }
    }
    // Cuadrado ya no hace nada; L1 da el puñetazo (estado 200) una sola vez.
    assert_eq!(seen.iter().filter(|&&s| s == 200).count(), 1, "{seen:?}");
}

#[test]
fn life_time_and_difficulty_rules() {
    // Vida al 50 % y reloj de 30 segundos: la ronda acaba por tiempo.
    let mut g = game();
    g.set_rules(50, 30, 4);
    assert_eq!((g.chars[0].life, g.chars[0].life_max), (500, 500));
    let mut over_at = None;
    for t in 0..2400 {
        g.tick([0, 0]);
        if g.round.state == 3 && over_at.is_none() {
            over_at = Some(t);
        }
    }
    let over_at = over_at.expect("la ronda no acabó por tiempo");
    assert!((1800..2100).contains(&over_at), "acabó en el tick {over_at}");

    // Sin límite de tiempo no acaba nunca sola.
    let mut g = game();
    g.set_rules(100, 0, 4);
    for _ in 0..(100 * 60 + 400) {
        g.tick([0, 0]);
    }
    assert_eq!(g.round.state, 2);
    assert_eq!((g.round.no, g.chars[0].life), (1, 1000));

    // La máquina pega más con la dificultad alta que con la baja a un rival
    // que no se mueve; con la de fábrica (4) la pelea es la de siempre.
    let damage = |level: Option<i32>| {
        let mut g = game();
        if let Some(l) = level {
            g.set_rules(100, 99, l);
        }
        g.chars[1].ai = true;
        for _ in 0..1500 {
            g.tick([0, 0]);
            if g.round.state != 2 && g.tick > 400 {
                break;
            }
        }
        (1000 - g.chars[0].life, g.tick)
    };
    let (easy, normal, hard) = (damage(Some(1)), damage(Some(4)), damage(Some(8)));
    assert!(easy.0 < normal.0 || easy.1 > normal.1, "fácil {easy:?}, normal {normal:?}");
    assert!(hard.0 > easy.0 || hard.1 < easy.1, "fácil {easy:?}, difícil {hard:?}");
    assert_eq!(damage(None), normal);
}

#[test]
fn slow_game_speed_skips_ticks_without_repeating_sounds() {
    let mut app = App::direct(ROOT, "kfm", "kfm", "kfm").unwrap();
    let mut o = app.options().clone();
    o.speed = -9;
    app.set_options(o);
    let mut frame = Frame::default();
    let (mut sounds, mut last_tick, mut repeats) = (0, -1, 0);
    for _ in 0..600 {
        app.tick([0, 0]);
        app.draw(&mut frame);
        let tick = app.game.as_ref().unwrap().tick;
        // Un frame sin tick no vuelve a lanzar los sonidos del anterior.
        if tick == last_tick {
            repeats += frame.sounds.len();
        }
        last_tick = tick;
        sounds += frame.sounds.len();
    }
    // "Slow 9": 33 ticks de pelea por cada 60 frames.
    assert_eq!(app.game.as_ref().unwrap().tick, 600 * 33 / 60);
    assert!(sounds > 0);
    assert_eq!(repeats, 0);
}
