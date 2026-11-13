//! Herramienta de PC: inspecciona archivos de MUGEN y ejecuta el motor sin
//! consola, guardando capturas PNG.

mod iso;
mod png;

use mugen::air;
use mugen::fnt::Font;
use mugen::app::App;
use mugen::gfx::{Frame, PalRef, Soft, SpriteCmd, Trans};
use mugen::sff::Sff;
use mugen::snd::Snd;
use std::env;

fn sheet(path: &str, out: &str, pal: Option<u16>) {
    let sff = Sff::load(path).expect("sff");
    println!("{path}: v{}, {} sprites, {} paletas", sff.version, sff.sprites.len(), sff.pals.len());
    let bad = (0..sff.sprites.len() as u32).filter(|&i| sff.with_pixels(i, |_| ()).is_none()).count();
    println!("  descomprimido: {} KB{}", sff.decoded_size() / 1024, if sff.is_streamed() { " (bajo demanda)" } else { "" });
    println!("  sin decodificar: {bad}");
    let (cw, ch) = (1280usize, 960usize);
    let mut px = vec![0x40_20_20u32; cw * ch];
    let (mut x, mut y, mut row) = (0usize, 0usize, 0usize);
    for (si, s) in sff.sprites.iter().enumerate() {
        let (w, h) = (s.w as usize, s.h as usize);
        let Some(pix) = sff.with_pixels(si as u32, |p| p.to_vec()) else { continue };
        if w > cw {
            continue;
        }
        if x + w > cw {
            x = 0;
            y += row + 1;
            row = 0;
        }
        if y + h > ch {
            break;
        }
        let p = &sff.pals[pal.unwrap_or(s.pal).min(sff.pals.len() as u16 - 1) as usize];
        for j in 0..h {
            for i in 0..w {
                let c = p[pix[j * w + i] as usize];
                if c >> 24 != 0 {
                    px[(y + j) * cw + x + i] = c & 0xff_ffff;
                }
            }
        }
        x += w + 1;
        row = row.max(h);
    }
    png::save(out, cw, ch, &px, 1).expect("png");
    println!("  -> {out}");
}

/// Ejecuta una pelea sin ventana: `run <ticks>` y, opcionales:
/// `--p1 nombre --p2 nombre --stage nombre --root carpeta` (qué cargar),
/// `--motif data/system.def --pad2 --screens` (juego completo con screenpack),
/// `--ai1 --noai2 --keys archivo --power n` (quién juega),
/// `--stream KB` (umbral de carga de sprites bajo demanda),
/// `--options archivo` (opciones: se leen de ahí y se guardan al cambiarlas),
/// `--log n` (estado cada n ticks), `--shots a,b,c --every n --strip a:b:paso
/// --out carpeta --clsn` (capturas PNG) y `--drawall` (dibuja cada tick).
fn run(args: &[String]) {
    let ticks: i32 = args.first().and_then(|s| s.parse().ok()).unwrap_or(600);
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let flag = |name: &str| args.iter().any(|a| a == name);
    let shots: Vec<i32> = opt("--shots").map(|s| s.split(',').filter_map(|v| v.parse().ok()).collect()).unwrap_or_default();
    let every: i32 = opt("--every").and_then(|s| s.parse().ok()).unwrap_or(0);
    let log: i32 = opt("--log").and_then(|s| s.parse().ok()).unwrap_or(0);
    let out = opt("--out").unwrap_or_else(|| "out".into());
    let root = opt("--root").unwrap_or_else(|| "game".into());
    // --stream KB: umbral a partir del cual un SFF se carga bajo demanda.
    if let Some(kb) = opt("--stream").and_then(|s| s.parse::<usize>().ok()) {
        mugen::sff::set_streaming(kb * 1024, 1 << 20);
    }
    let stage = opt("--stage").unwrap_or_else(|| "kfm".into());
    let script = opt("--keys").map(|f| mugen::script::Script::load(&f).expect("guion"));
    let (p1, p2) = (opt("--p1").unwrap_or_else(|| "kfm".into()), opt("--p2").unwrap_or_else(|| "kfm".into()));
    // Con --motif se ejecuta el juego completo, con sus pantallas.
    let mut app = match opt("--motif") {
        Some(m) => App::with_motif(&root, &m),
        None => App::direct(&root, &p1, &p2, &stage),
    }
    .expect("no pude cargar el juego");
    app.pad2 = flag("--pad2");
    // --options archivo: las opciones se leen de ahí al empezar y se guardan
    // ahí cuando cambian en el menú.
    let options_file = opt("--options");
    if let Some(text) = options_file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()) {
        let mut o = app.options().clone();
        o.apply_text(&text);
        app.set_options(o);
    }
    let mut before = 0;
    if let Some(g) = app.game.as_mut() {
        g.chars[0].ai = flag("--ai1");
        g.chars[1].ai = !flag("--noai2");
        g.show_clsn = flag("--clsn");
        for w in &g.warnings {
            println!("aviso: {w}");
        }
        before = g.warnings.len();
    }
    let mut frame = Frame::default();
    let mut soft = Soft::new();
    // --strip a:b:paso -> una sola imagen con los frames en cuadrícula.
    let strip: Vec<i32> = opt("--strip").map(|s| s.split(':').filter_map(|v| v.parse().ok()).collect()).unwrap_or_default();
    let mut tiles: Vec<Vec<u32>> = Vec::new();
    let start = std::time::Instant::now();
    let mut last_screen = "";
    for t in 0..ticks {
        let keys = script.as_ref().map_or([0, 0], |s| s.keys(t));
        if t == 200 {
            if let (Some(p), Some(g)) = (opt("--power").and_then(|s| s.parse().ok()), app.game.as_mut()) {
                g.chars[0].power = p;
            }
        }
        // Con screenpack, --ai1 deja al jugador 1 en manos de la máquina.
        if let (true, true, Some(g)) = (flag("--ai1"), app.motif.is_some(), app.game.as_mut()) {
            g.chars[0].ai = true;
        }
        app.tick(keys);
        if flag("--drawall") {
            app.draw(&mut frame);
            soft.render(&frame, app.sffs());
        }
        if let Some(text) = app.take_options_to_save() {
            if let Some(f) = &options_file {
                std::fs::write(f, &text).expect("no pude guardar las opciones");
            }
        }
        if log > 0 && t % log == 0 {
            println!("{}", app.summary());
        }
        if flag("--screens") && app.screen_name() != last_screen {
            last_screen = app.screen_name();
            println!("t{t}: {last_screen}");
        }
        if shots.contains(&t) || (every > 0 && t % every == 0) {
            app.draw(&mut frame);
            soft.render(&frame, app.sffs());
            png::save(&format!("{out}/f{t:05}.png"), 320, 240, &soft.px, 2).expect("png");
        }
        if strip.len() == 3 && t >= strip[0] && t < strip[1] && (t - strip[0]) % strip[2].max(1) == 0 {
            app.draw(&mut frame);
            soft.render(&frame, app.sffs());
            tiles.push(soft.px.clone());
        }
    }
    if !tiles.is_empty() {
        let cols = 4.min(tiles.len());
        let rows = tiles.len().div_ceil(cols);
        let mut px = vec![0u32; cols * 320 * rows * 240];
        for (i, t) in tiles.iter().enumerate() {
            let (cx, cy) = (i % cols * 320, i / cols * 240);
            for y in 0..240 {
                px[(cy + y) * cols * 320 + cx..][..320].copy_from_slice(&t[y * 320..][..320]);
            }
        }
        png::save(&format!("{out}/strip.png"), cols * 320, rows * 240, &px, 1).expect("png");
    }
    println!("{} ticks en {:?}; final: {}", ticks, start.elapsed(), app.summary());
    if let Some(e) = &app.error {
        println!("error: {e}");
    }
    if let Some(g) = &app.game {
        for w in g.warnings.iter().skip(before) {
            println!("aviso: {w}");
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let a = |i: usize| args.get(i).map(String::as_str).unwrap_or("");
    match a(0) {
        "sheet" => sheet(a(1), a(2), args.get(3).and_then(|s| s.parse().ok())),
        "info" => {
            let snd = Snd::load(a(1)).expect("snd");
            println!("{} sonidos", snd.samples.len());
            let mut keys: Vec<_> = snd.index.iter().collect();
            keys.sort();
            for (k, &i) in keys {
                let s = &snd.samples[i as usize];
                println!("  {:?}: {} Hz, {} muestras", k, s.rate, s.frames);
            }
        }
        "air" => {
            let t = air::load(a(1)).expect("air");
            let mut nos: Vec<_> = t.keys().copied().collect();
            nos.sort();
            println!("{} animaciones", nos.len());
            for no in nos {
                let an = &t[&no];
                println!("  {no}: {} elem, total {}, loop {}, inf {}, clsn1 {} clsn2 {}", an.frames.len(), an.total, an.loopstart, an.infinite,
                    an.frames.iter().map(|f| f.clsn1.len()).sum::<usize>(), an.frames.iter().map(|f| f.clsn2.len()).sum::<usize>());
            }
        }
        "font" => {
            let (font, sheet) = Font::load(a(1)).expect("fnt");
            println!("glifos {}, size {:?}, spacing {:?}, offset {:?}, bancos {}", font.glyphs.len(), font.size, font.spacing, font.offset, font.banks);
            let sffs = vec![sheet];
            let mut frame = Frame::default();
            frame.clear = 0x40_20_20;
            for b in 0..sffs[0].pals.len().min(8) {
                font.draw(&mut frame, 0, b as i32, 1, 4.0, 20.0 + b as f32 * 24.0, "Kung Fu Man 0123456789 Round 1");
            }
            font.draw(&mut frame, 0, 0, 0, 160.0, 230.0, "CENTRO");
            frame.sprite(SpriteCmd { sff: 0, spr: 0, x: 0.0, y: 200.0, sx: 1.0, sy: 1.0, pal: PalRef::Sff(0, 0), trans: Trans::None, clip: None, src: None });
            let mut soft = Soft::new();
            soft.render(&frame, &sffs);
            png::save(a(2), 320, 240, &soft.px, 3).unwrap();
        }
        "run" => run(&args[1..]),
        "iso" => {
            // iso <elf> <salida.iso> [game.pak]: imagen de disco con el
            // ejecutable y, si se da, el paquete de datos como GAME.PAK.
            let elf = std::fs::read(a(1)).expect("no pude leer el ELF");
            let pak = args.get(3).map(|p| std::fs::read(p).expect("no pude leer el paquete"));
            let img = iso::build(&elf, "MUGN_000.01", "MUGEN", pak.as_deref());
            std::fs::write(a(2), &img).expect("no pude escribir la imagen");
            println!("{}: {} KB", a(2), img.len() / 1024);
        }
        _ => eprintln!("uso: mugen-host run <ticks> [...] | iso <elf> <iso> [game.pak] | sheet <sff> <png> [paleta] | info <snd> | air <air> | font <fnt> <png>"),
    }
}
