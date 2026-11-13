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
    assert_eq!(m.title.items.len(), 4);
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
