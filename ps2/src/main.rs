//! M.U.G.E.N en PlayStation 2.
//!
//! Los datos del juego (la carpeta `game/`) van empaquetados dentro del
//! ejecutable, así que el ELF funciona solo desde cualquier sitio: USB o
//! tarjeta de memoria con Free McBoot, un disco, o `host:` en el emulador.
//! `game/ps2.cfg` dice qué screenpack usar; sin él se arranca directo en una
//! pelea.

mod audio;
mod card;
mod gs;
mod iop;
mod pad;

use mugen::app::App;
use mugen::gfx::Frame;
use mugen::ini::{self, Section};
use mugen::options::{Options, PadLayout};
use mugen::script::Script;
use mugen::vfs;
use std::time::Instant;

/// Opciones de prueba, de `game/ps2test.cfg` (si existe): `clave = valor`.
#[derive(Default)]
struct Test {
    /// Ticks a ejecutar antes de avisar por consola; 0 = sin prueba.
    ticks: i32,
    ai1: bool,
    /// Cada cuántos ticks se imprime el resumen del estado.
    log: i32,
    script: Option<Script>,
    /// Tras la prueba, se queda mostrando el último frame.
    hold: bool,
    nosound: bool,
    progressive: bool,
    /// La prueba usa la tarjeta de memoria (las demás no la tocan, para no
    /// depender de lo que haya guardado) y, al terminar, borra lo guardado.
    card: bool,
    card_clean: bool,
    /// Screenpack de la prueba; vacío = pelea directa.
    motif: String,
    /// Personajes de la pelea directa, si la prueba los cambia.
    players: [Option<String>; 2],
    /// Ajustes de carga bajo demanda que la prueba cambia.
    stream: std::collections::HashMap<String, i32>,
}

fn load_test() -> Option<Test> {
    let secs = ini::load("game/ps2test.cfg").ok()?;
    let s = &secs[0];
    Some(Test {
        ticks: s.i32("ticks", 0),
        ai1: s.i32("ai1", 0) != 0,
        log: s.i32("log", 0),
        hold: s.i32("hold", 0) != 0,
        nosound: s.i32("nosound", 0) != 0,
        progressive: s.i32("progressive", 0) != 0,
        card: s.i32("card", 0) != 0,
        card_clean: s.i32("card.clean", 0) != 0,
        script: s.str("keys").and_then(|f| Script::load(&format!("game/{f}")).ok()),
        motif: s.str("motif").unwrap_or_default(),
        players: [s.str("p1"), s.str("p2")].map(|p| p.filter(|v| !v.is_empty())),
        stream: ["stream", "stream.cache"].iter().filter(|k| s.get(k).is_some()).map(|k| (k.to_string(), s.i32(k, 0))).collect(),
    })
}

/// La carpeta `game/`, empaquetada por `build.rs`.
#[cfg(not(feature = "disc"))]
static GAME: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/game.pak"));

/// Monta los datos del juego: de dentro del ELF o, en la versión para
/// disco, del archivo `GAME.PAK` que está junto a él.
fn mount_game() -> Result<String, String> {
    #[cfg(not(feature = "disc"))]
    {
        let n = vfs::mount(GAME)?;
        Ok(format!("{n} archivos, {} KB dentro del ELF", GAME.len() / 1024))
    }
    #[cfg(feature = "disc")]
    {
        let dir = std::env::current_dir().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
        // En un disco los nombres llevan versión: `cdrom0:\GAME.PAK;1`.
        let path = if dir.starts_with("cdrom") { "cdrom0:\\GAME.PAK;1".to_string() } else { format!("{dir}GAME.PAK") };
        let n = vfs::mount_file(&path)?;
        Ok(format!("{n} archivos, leídos de {path}"))
    }
}

fn main() {
    println!("M.U.G.E.N PS2: cargando...");
    // Quien nos lanzó (el emulador, uLaunchELF, la BIOS desde un disco) pudo
    // dejar el IOP con sus propios controladores: se reinicia para partir
    // siempre de los módulos de la BIOS.
    std::os::ps2::reset_iop();
    let start = Instant::now();
    match mount_game() {
        Ok(what) => println!("paquete de datos: {what}"),
        Err(e) => {
            println!("error: {e}");
            return;
        }
    }
    let cfg: Section = ini::load("game/ps2.cfg").ok().map(|mut s| s.swap_remove(0)).unwrap_or_default();
    let name = |key: &str, default: &str| cfg.str(key).filter(|v| !v.is_empty()).unwrap_or_else(|| default.to_string());
    let testing = load_test();
    let test = testing.as_ref();
    // Carga de sprites bajo demanda: umbral y caché, en KB.
    let kb = |key: &str, default: i32| {
        let from_test = test.and_then(|t| t.stream.get(key).copied());
        from_test.unwrap_or_else(|| cfg.i32(key, default)).max(0) as usize * 1024
    };
    mugen::sff::set_streaming(kb("stream", 4096), kb("stream.cache", 1024));
    let ticks = test.map_or(0, |t| t.ticks);
    // En una prueba manda su configuración; si no, la de ps2.cfg.
    let motif = match test {
        Some(t) => t.motif.clone(),
        None => cfg.str("motif").unwrap_or_default(),
    };
    let loaded = if motif.is_empty() {
        let player = |i: usize, key: &str| test.and_then(|t| t.players[i].clone()).unwrap_or_else(|| name(key, "kfm"));
        App::direct("game", &player(0, "p1"), &player(1, "p2"), &name("stage", "kfm"))
    } else {
        App::with_motif("game", &motif)
    };
    let mut app = match loaded {
        Ok(a) => a,
        Err(e) => {
            println!("error al cargar: {e}");
            return;
        }
    };
    println!("datos cargados en {} ms", start.elapsed().as_millis());
    if let Some(g) = &app.game {
        for w in &g.warnings {
            println!("aviso: {w}");
        }
    }

    let progressive = name("video", "448i").eq_ignore_ascii_case("240p") || test.is_some_and(|t| t.progressive);
    let mut gs = gs::Gs::init(if progressive { gs::VideoMode::Progressive } else { gs::VideoMode::Interlaced });
    // La tarjeta de memoria primero: sus módulos traen el SIO2MAN que
    // también necesitan los mandos.
    card::init();
    let pad = pad::Pad::init();

    // Opciones: las de fábrica del mando, encima las de ps2.cfg y encima las
    // guardadas en la tarjeta de memoria.
    let layout = PadLayout::PS2;
    app.set_layout(layout);
    let use_card = test.map_or(true, |t| t.card);
    let mut options = Options::new(&layout);
    if let Ok(text) = ini::read_text("game/ps2.cfg") {
        options.apply_text(&text);
    }
    if use_card {
        match card::load() {
            Some(text) => {
                options.apply_text(&text);
                println!("opciones: leídas de la tarjeta de memoria");
            }
            None => println!("opciones: no hay nada guardado en la tarjeta de memoria"),
        }
    }
    println!(
        "opciones: dificultad {}, vida {}, tiempo {}, velocidad {}, volumen {}",
        options.difficulty, options.life, options.time, options.speed, options.volume
    );
    app.set_options(options);
    let mut iop = iop::Iop::init();
    let mut audio = audio::Audio::init(iop.as_mut());
    let nosound = test.is_some_and(|t| t.nosound);
    let mut epoch = u32::MAX;
    println!("memoria libre: {} KB", std::os::ps2::heap_free_bytes() / 1024);

    let mut frame = Frame::default();
    let mut tick = 0i32;
    let mut held = false;
    let mut screen = "";
    // Tiempos por frame, sin contar los frames en que se carga algo.
    let (mut worst, mut total, mut frames, mut slow, mut load_ms) = (0u32, 0u64, 0u64, 0u32, 0u64);
    loop {
        // Botones de cada mando. Un guion de prueba habla de funciones (a,
        // b, arriba…): se pasan a los botones que las hacen de fábrica.
        let keys = match test.and_then(|t| t.script.as_ref()) {
            Some(s) => s.keys(tick).map(|k| layout.raw_from_keys(k)),
            None => [pad.buttons(0), pad.buttons(1)],
        };
        // Sin segundo mando, al jugador 2 lo lleva la máquina.
        app.pad2 = pad.connected(1) && test.is_none();
        let (has_motif, pad2) = (app.motif.is_some(), app.pad2);
        if let Some(g) = app.game.as_mut() {
            if !has_motif {
                g.chars[0].ai = test.is_some_and(|t| t.ai1);
                g.chars[1].ai = !pad2;
            } else if test.is_some_and(|t| t.ai1) {
                g.chars[0].ai = true;
            }
        }
        let t0 = Instant::now();
        if !held {
            app.tick(keys);
            app.draw(&mut frame);
        }
        // Al salir del menú de opciones con cambios, se guardan.
        if let Some(text) = app.take_options_to_save() {
            if use_card {
                match card::save(&text) {
                    Ok(()) => println!("opciones: guardadas en la tarjeta de memoria"),
                    Err(e) => println!("opciones: no se pudieron guardar ({e})"),
                }
            }
        }
        // Al cambiar de pantalla cambian los sprites y los sonidos.
        let loading = app.epoch() != epoch;
        if loading {
            epoch = app.epoch();
            gs.reset_cache();
            if !nosound {
                let t = Instant::now();
                audio.load(app.snds());
                println!("audio: {} sonidos, {} bytes en el SPU2, {} ms", audio.loaded, audio.spu_used, t.elapsed().as_millis());
            }
        }
        gs.render(&frame, app.sffs());
        if !held {
            audio.play(&frame.sounds);
        }
        let us = t0.elapsed().as_micros() as u32;
        if loading {
            load_ms += us as u64 / 1000;
        } else {
            worst = worst.max(us);
            total += us as u64;
            frames += 1;
            // Un frame que no llega a tiempo (16,7 ms) se nota como un tirón.
            slow += (us > 16_667) as u32;
        }
        gs.present();

        if let (Some(t), false) = (test, held) {
            if t.log > 0 && tick % t.log == 0 {
                println!("{}", app.summary());
            }
            if has_motif && app.screen_name() != screen {
                screen = app.screen_name();
                println!("t{tick}: {screen}");
            }
            if !frame.sounds.is_empty() && tick > 60 && tick < ticks - 1 {
                // ¿Suena de verdad la voz que se acaba de encender?
                let v = audio.last_voice;
                let (nax, envx) = audio.probe(v);
                println!("audio: t{tick} voz {v} en {nax:#x}, envolvente {envx:#x}");
            }
            if tick == ticks / 2 {
                println!("pad: estado {} / {}, botones {:#06x}", pad.state(0), pad.state(1), pad.buttons(0));
            }
            if tick + 1 == ticks {
                println!("final: {}", app.summary());
                println!("frame: media {} us, peor {} us; {} sprites, {} subidas", total / frames.max(1), worst, gs.sprites, gs.uploads);
                println!("frames fuera de tiempo: {slow} de {frames}; cargas: {load_ms} ms");
                println!("memoria libre: {} KB", std::os::ps2::heap_free_bytes() / 1024);
                let streamed: Vec<_> = app.sffs().iter().filter(|s| s.is_streamed()).collect();
                let (decodes, cached) = streamed.iter().map(|s| s.cache_stats()).fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
                println!("sprites bajo demanda: {} de {} SFF; {decodes} descompresiones, {} KB en caché", streamed.len(), app.sffs().len(), cached / 1024);
                if t.card {
                    // Lo que quedó de verdad en la tarjeta, sin los botones
                    // (cada plataforma tiene los suyos).
                    for line in card::load().unwrap_or_default().lines().filter(|l| !l.starts_with('p')) {
                        println!("tarjeta: {line}");
                    }
                    for entry in card::list() {
                        println!("tarjeta: contiene {entry}");
                    }
                    if t.card_clean {
                        println!("tarjeta: partida borrada: {}", card::remove());
                    }
                }
                println!("prueba terminada");
                held = t.hold;
                if !held {
                    return;
                }
            }
        }
        tick += 1;
    }
}
