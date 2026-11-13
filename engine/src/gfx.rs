//! Lista de dibujo de un frame (independiente de la plataforma) y un
//! rasterizador por software de referencia.

use crate::sff::{Pal, Sff};

pub const SCREEN_W: i32 = 320;
pub const SCREEN_H: i32 = 240;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Trans {
    None,
    /// `origen * src / 256 + destino * dst / 256`
    Add { src: u16, dst: u16 },
    /// `destino - origen`
    Sub,
}

impl Trans {
    pub const ADD: Trans = Trans::Add { src: 256, dst: 256 };
    pub const ADD1: Trans = Trans::Add { src: 256, dst: 128 };
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PalRef {
    /// Paleta `pal` del SFF `sff`.
    Sff(u16, u16),
    /// Paleta calculada este frame (`Frame::pals`).
    Dyn(u16),
}

#[derive(Clone, Debug)]
pub struct SpriteCmd {
    pub sff: u16,
    pub spr: u32,
    /// Posición en pantalla del eje del sprite.
    pub x: f32,
    pub y: f32,
    /// Escala; negativa = espejado respecto al eje.
    pub sx: f32,
    pub sy: f32,
    pub pal: PalRef,
    pub trans: Trans,
    /// Recorte en pantalla: x0, y0, x1, y1.
    pub clip: Option<[i32; 4]>,
    /// Parte del sprite a dibujar: u0, v0, u1, v1.
    pub src: Option<[u16; 4]>,
}

#[derive(Clone, Debug)]
pub enum Cmd {
    Sprite(SpriteCmd),
    Rect { x: i32, y: i32, w: i32, h: i32, rgba: u32, trans: Trans },
}

/// Sonido a reproducir o canal a detener.
#[derive(Clone, Copy, Debug)]
pub struct SoundCmd {
    pub bank: u16,
    pub idx: u32,
    /// Canal lógico (jugador * 100 + canal) o -1 para uno libre.
    pub chan: i32,
    pub vol: f32,
    pub pan: f32,
    pub freq: f32,
    pub stop: bool,
}

#[derive(Default)]
pub struct Frame {
    pub cmds: Vec<Cmd>,
    pub pals: Vec<Pal>,
    pub sounds: Vec<SoundCmd>,
    pub clear: u32,
}

impl Frame {
    pub fn reset(&mut self) {
        self.cmds.clear();
        self.pals.clear();
        self.sounds.clear();
    }

    pub fn dyn_pal(&mut self, pal: Pal) -> PalRef {
        self.pals.push(pal);
        PalRef::Dyn(self.pals.len() as u16 - 1)
    }

    pub fn sprite(&mut self, c: SpriteCmd) {
        self.cmds.push(Cmd::Sprite(c));
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgba: u32, trans: Trans) {
        self.cmds.push(Cmd::Rect { x, y, w, h, rgba, trans });
    }
}

fn blend(dst: u32, src: u32, t: Trans) -> u32 {
    let ch = |v: u32, s: u32| (v >> s) & 0xff;
    let mix = |s: u32, d: u32| -> u32 {
        match t {
            Trans::None => s,
            Trans::Add { src: a, dst: b } => ((s * a as u32 + d * b as u32) >> 8).min(255),
            Trans::Sub => d.saturating_sub(s),
        }
    };
    mix(ch(src, 0), ch(dst, 0)) | mix(ch(src, 8), ch(dst, 8)) << 8 | mix(ch(src, 16), ch(dst, 16)) << 16 | 0xff00_0000
}

/// Framebuffer por software de 320×240.
pub struct Soft {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Default for Soft {
    fn default() -> Self {
        Soft::new()
    }
}

impl Soft {
    pub fn new() -> Soft {
        Soft { w: SCREEN_W, h: SCREEN_H, px: vec![0xff00_0000; (SCREEN_W * SCREEN_H) as usize] }
    }

    pub fn render(&mut self, frame: &Frame, sffs: &[Sff]) {
        self.px.fill(frame.clear | 0xff00_0000);
        for cmd in &frame.cmds {
            match cmd {
                Cmd::Rect { x, y, w, h, rgba, trans } => {
                    for py in (*y).max(0)..(*y + *h).min(self.h) {
                        for px in (*x).max(0)..(*x + *w).min(self.w) {
                            let i = (py * self.w + px) as usize;
                            self.px[i] = blend(self.px[i], *rgba, *trans);
                        }
                    }
                }
                Cmd::Sprite(c) => {
                    let Some(sff) = sffs.get(c.sff as usize) else { continue };
                    let Some(spr) = sff.sprites.get(c.spr as usize) else { continue };
                    let pal = match c.pal {
                        PalRef::Sff(s, p) => sffs.get(s as usize).and_then(|f| f.pals.get(p as usize)),
                        PalRef::Dyn(i) => frame.pals.get(i as usize),
                    };
                    let Some(pal) = pal else { continue };
                    sff.with_pixels(c.spr, |pix| self.sprite(c, spr.w as i32, spr.h as i32, spr.ax as f32, spr.ay as f32, pix, pal));
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sprite(&mut self, c: &SpriteCmd, w: i32, h: i32, ax: f32, ay: f32, pix: &[u8], pal: &Pal) {
        if pix.len() < (w * h) as usize || c.sx == 0.0 || c.sy == 0.0 {
            return;
        }
        let [u0, v0, u1, v1] = c.src.map(|s| s.map(|v| v as i32)).unwrap_or([0, 0, w, h]);
        let (u0, v0, u1, v1) = (u0.max(0), v0.max(0), u1.min(w), v1.min(h));
        let [cx0, cy0, cx1, cy1] = c.clip.unwrap_or([0, 0, self.w, self.h]);
        let span = |o: f32, a: f32, s: f32, lo: i32, hi: i32| -> (i32, i32) {
            let p = o + (lo as f32 - a) * s;
            let q = o + (hi as f32 - a) * s;
            let (min, max) = if p < q { (p, q) } else { (q, p) };
            ((min - 0.5).ceil() as i32, (max - 0.5).ceil() as i32)
        };
        let (x0, x1) = span(c.x, ax, c.sx, u0, u1);
        let (y0, y1) = span(c.y, ay, c.sy, v0, v1);
        for py in y0.max(cy0).max(0)..y1.min(cy1).min(self.h) {
            let v = ((py as f32 + 0.5 - c.y) / c.sy + ay).floor() as i32;
            if v < v0 || v >= v1 {
                continue;
            }
            for px in x0.max(cx0).max(0)..x1.min(cx1).min(self.w) {
                let u = ((px as f32 + 0.5 - c.x) / c.sx + ax).floor() as i32;
                if u < u0 || u >= u1 {
                    continue;
                }
                let idx = pix[(v * w + u) as usize];
                let col = pal[idx as usize];
                if col >> 24 == 0 {
                    continue;
                }
                let i = (py * self.w + px) as usize;
                self.px[i] = blend(self.px[i], col, c.trans);
            }
        }
    }
}
