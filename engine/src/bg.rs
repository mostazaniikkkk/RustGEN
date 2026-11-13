//! Fondos por capas: los de los escenarios (`[BG x]`) y los de las pantallas
//! del screenpack (`[TitleBG x]`, `[SelectBG x]`…), que usan el mismo formato.

use crate::air::{parse_trans, AnimTable};
use crate::gfx::{Frame, PalRef, SpriteCmd, Trans, SCREEN_H, SCREEN_W};
use crate::ini::{self, Section};
use crate::sff::Sff;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Normal,
    Parallax,
    /// Animación `[Begin Action n]` del mismo archivo.
    Anim(i32),
}

struct Bg {
    kind: Kind,
    sprite: (i32, i32),
    layer: i32,
    start: (f32, f32),
    delta: (f32, f32),
    trans: Trans,
    mask: bool,
    tile: (i32, i32),
    tilespacing: (f32, f32),
    velocity: (f32, f32),
    window: Option<[i32; 4]>,
    /// Parallax: escala horizontal arriba y abajo, o anchos en píxeles.
    xscale: (f32, f32),
    width: Option<(f32, f32)>,
    yscalestart: f32,
    yscaledelta: f32,
}

pub struct BgSet {
    pub sff: u16,
    bgs: Vec<Bg>,
    /// Color de fondo (`bgclearcolor`).
    pub clear: u32,
}

pub fn rgb(l: &[f32]) -> u32 {
    let at = |i: usize| l.get(i).copied().unwrap_or(0.0).clamp(0.0, 255.0) as u32;
    at(0) | at(1) << 8 | at(2) << 16 | 0xff00_0000
}

pub fn trans_of(s: &Section, key: &str, alpha_key: &str) -> Trans {
    let t = match s.get(key).unwrap_or("none").trim().to_ascii_lowercase().as_str() {
        "add" => Trans::ADD,
        "add1" => Trans::ADD1,
        "sub" => Trans::Sub,
        "addalpha" => {
            let (a, d) = s.pair(alpha_key, 256.0, 0.0);
            Trans::Add { src: a as u16, dst: d as u16 }
        }
        other => parse_trans(other),
    };
    if t == (Trans::Add { src: 256, dst: 0 }) {
        Trans::None
    } else {
        t
    }
}

impl BgSet {
    pub fn empty(sff: u16) -> BgSet {
        BgSet { sff, bgs: Vec::new(), clear: 0xff00_0000 }
    }

    /// Lee las secciones `[<prefix> nombre]` y `[<prefix>def]`. Si la
    /// definición trae su propio `spr`, lo carga y lo añade a `sffs`.
    pub fn load(secs: &[Section], prefix: &str, def_path: &str, default_sff: u16, sffs: &mut Vec<Sff>) -> BgSet {
        let def = secs.iter().find(|s| s.name == format!("{prefix}def"));
        let mut sff = default_sff;
        if let Some(file) = def.and_then(|d| d.str("spr")).filter(|f| !f.is_empty()) {
            if let Ok(s) = Sff::load(&ini::sibling(def_path, &file)) {
                sffs.push(s);
                sff = sffs.len() as u16 - 1;
            }
        }
        let clear = def.map(|d| d.list("bgclearcolor", 0.0)).map_or(0xff00_0000, |l| rgb(&l));
        let mut bgs = Vec::new();
        let lead = format!("{prefix} ");
        for s in secs.iter().filter(|s| s.name.starts_with(&lead) || s.name == prefix) {
            let kind = match s.get("type").unwrap_or("normal").trim().to_ascii_lowercase().as_str() {
                "normal" => Kind::Normal,
                "parallax" => Kind::Parallax,
                "anim" => Kind::Anim(s.i32("actionno", 0)),
                _ => continue,
            };
            let (g, i) = s.pair("spriteno", 0.0, 0.0);
            let (tx, ty) = s.pair("tile", 0.0, 0.0);
            let window = s.get("window").map(|v| ini::nums(v, 0.0)).filter(|w| w.len() >= 4).map(|w| [w[0] as i32, w[1] as i32, w[2] as i32 + 1, w[3] as i32 + 1]);
            bgs.push(Bg {
                kind,
                sprite: (g as i32, i as i32),
                layer: s.i32("layerno", 0),
                start: s.pair("start", 0.0, 0.0),
                delta: s.pair("delta", 1.0, 1.0),
                trans: trans_of(s, "trans", "alpha"),
                // Las animaciones siempre respetan la transparencia.
                mask: s.i32("mask", 0) != 0 || matches!(kind, Kind::Anim(_)),
                tile: (tx as i32, ty as i32),
                tilespacing: s.pair("tilespacing", 0.0, 0.0),
                velocity: s.pair("velocity", 0.0, 0.0),
                window,
                xscale: s.pair("xscale", 1.0, 1.0),
                width: s.get("width").map(|_| s.pair("width", 0.0, 0.0)),
                yscalestart: s.f32("yscalestart", 100.0),
                yscaledelta: s.f32("yscaledelta", 0.0),
            });
        }
        BgSet { sff, bgs, clear }
    }

    /// Dibuja las capas de `layer` (0 = al fondo, 1 = por delante).
    pub fn draw(&self, frame: &mut Frame, sffs: &[Sff], anims: &AnimTable, cam: [f32; 2], shake_y: f32, tick: i32, layer: i32) {
        let Some(sff) = sffs.get(self.sff as usize) else { return };
        for bg in self.bgs.iter().filter(|b| b.layer == layer) {
            // Sprite, desplazamiento y mezcla: fijos o del cuadro de animación.
            let (sprite, off, trans, flip) = match bg.kind {
                Kind::Anim(no) => {
                    let Some(f) = anims.get(&no).and_then(|a| a.frame_at(tick, true)) else { continue };
                    let t = if f.trans == Trans::None { bg.trans } else { f.trans };
                    ((f.group, f.image), (f.x as f32, f.y as f32), t, if f.hflip { -1.0 } else { 1.0 })
                }
                _ => (bg.sprite, (0.0, 0.0), bg.trans, 1.0),
            };
            let Some(si) = sff.find(sprite.0, sprite.1) else { continue };
            let spr = &sff.sprites[si as usize];
            let (w, h) = (spr.w as f32, spr.h as f32);
            if w == 0.0 || h == 0.0 {
                continue;
            }
            let pal = if bg.mask {
                PalRef::Sff(self.sff, spr.pal)
            } else {
                // Sin máscara, el color 0 también se dibuja.
                let mut p = sff.pals[spr.pal as usize];
                p[0] |= 0xff00_0000;
                frame.dyn_pal(p)
            };
            let t = tick as f32;
            let half = SCREEN_W as f32 / 2.0;
            let y = bg.start.1 + off.1 - cam[1] * bg.delta.1 + bg.velocity.1 * t + shake_y;
            let cmd = |x: f32, y: f32, sx: f32, src: Option<[u16; 4]>| SpriteCmd { sff: self.sff, spr: si, x, y, sx, sy: 1.0, pal, trans, clip: bg.window, src };
            match bg.kind {
                Kind::Normal | Kind::Anim(_) => {
                    let x = half + bg.start.0 + off.0 - cam[0] * bg.delta.0 + bg.velocity.0 * t;
                    let (step_x, step_y) = (w + bg.tilespacing.0, h + bg.tilespacing.1);
                    let (left, top) = (x - spr.ax as f32, y - spr.ay as f32);
                    // Rango de copias que tocan la pantalla.
                    let range = |tile: i32, origin: f32, step: f32, size: f32, screen: f32| -> (i32, i32) {
                        match tile {
                            0 => (0, 0),
                            1 if step > 0.0 => ((-(origin + size) / step).ceil() as i32, ((screen - origin) / step).floor() as i32),
                            1 => (0, 0),
                            n => (0, n - 1),
                        }
                    };
                    let (x0, x1) = range(bg.tile.0, left, step_x, w, SCREEN_W as f32);
                    let (y0, y1) = range(bg.tile.1, top, step_y, h, SCREEN_H as f32);
                    for j in y0..=y1 {
                        for i in x0..=x1 {
                            frame.sprite(cmd(x + i as f32 * step_x, y + j as f32 * step_y, flip, None));
                        }
                    }
                }
                Kind::Parallax => {
                    // Cada fila del sprite lleva su propia escala horizontal:
                    // el rectángulo se ve como un trapecio.
                    let (top_s, bot_s) = match bg.width {
                        Some((a, b)) => (a / w, b / w),
                        None => bg.xscale,
                    };
                    let ys = ((bg.yscalestart + bg.yscaledelta * -cam[1]) / 100.0).max(0.01);
                    let rows = spr.h as i32;
                    let top = y - spr.ay as f32 * ys;
                    let scroll = bg.start.0 - cam[0] * bg.delta.0 + bg.velocity.0 * t;
                    for oy in 0..(h * ys).ceil() as i32 {
                        let v = ((oy as f32 + 0.5) / ys) as i32;
                        if v >= rows {
                            break;
                        }
                        let row_y = top + oy as f32;
                        if row_y < -1.0 || row_y > SCREEN_H as f32 {
                            continue;
                        }
                        let s = top_s + (bot_s - top_s) * (v as f32 + 0.5) / h;
                        let x = half + scroll * s;
                        let src = Some([0, v as u16, spr.w, v as u16 + 1]);
                        let yy = row_y - (v as f32 - spr.ay as f32);
                        if bg.tile.0 == 1 {
                            let step = w * s;
                            let left = x - spr.ax as f32 * s;
                            let i0 = (-(left + step) / step).ceil() as i32;
                            let i1 = ((SCREEN_W as f32 - left) / step).floor() as i32;
                            for i in i0..=i1 {
                                frame.sprite(cmd(x + i as f32 * step, yy, s, src));
                            }
                        } else {
                            frame.sprite(cmd(x, yy, s, src));
                        }
                    }
                }
            }
        }
    }
}
