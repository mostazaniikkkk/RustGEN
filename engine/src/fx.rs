//! Efectos visuales: explods (chispas, objetos), estelas y efectos de paleta.

use crate::air::AnimState;
use crate::eval::sinf;
use crate::game::Game;
use crate::gfx::{Frame, PalRef, SpriteCmd, Trans, SCREEN_W};
use crate::sff::Pal;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct PalFx {
    /// Ticks restantes; -1 = indefinido, 0 = apagado.
    pub time: i32,
    pub add: [i32; 3],
    pub mul: [i32; 3],
    pub sinadd: [i32; 3],
    pub period: i32,
    pub invert: bool,
    pub color: i32,
    pub t: i32,
}

impl Default for PalFx {
    fn default() -> Self {
        PalFx { time: 0, add: [0; 3], mul: [256; 3], sinadd: [0; 3], period: 1, invert: false, color: 256, t: 0 }
    }
}

impl PalFx {
    pub fn active(&self) -> bool {
        self.time != 0
    }

    pub fn step(&mut self) {
        if self.time > 0 {
            self.time -= 1;
        }
        self.t += 1;
    }

    pub fn apply(&self, pal: &Pal) -> Pal {
        let wave = if self.period != 0 { sinf(6.283_185_5 * self.t as f32 / self.period as f32) } else { 0.0 };
        let mut out = *pal;
        for c in out.iter_mut() {
            let a = *c & 0xff00_0000;
            let mut ch = [(*c & 0xff) as i32, (*c >> 8 & 0xff) as i32, (*c >> 16 & 0xff) as i32];
            if self.color < 256 {
                let gray = (ch[0] * 77 + ch[1] * 150 + ch[2] * 29) >> 8;
                for v in ch.iter_mut() {
                    *v = (*v * self.color + gray * (256 - self.color)) >> 8;
                }
            }
            for (i, v) in ch.iter_mut().enumerate() {
                if self.invert {
                    *v = 255 - *v;
                }
                let add = self.add[i] + (self.sinadd[i] as f32 * wave) as i32;
                *v = (((*v + add) * self.mul[i]) >> 8).clamp(0, 255);
            }
            *c = a | ch[0] as u32 | (ch[1] as u32) << 8 | (ch[2] as u32) << 16;
        }
        out
    }
}

#[derive(Clone, Copy)]
struct Ghost {
    group: i32,
    image: i32,
    hflip: bool,
    vflip: bool,
    pos: [f32; 2],
    facing: i32,
}

/// Estela de imágenes tras el personaje.
pub struct AfterImage {
    pub time: i32,
    pub length: i32,
    pub timegap: i32,
    pub framegap: i32,
    pub trans: Trans,
    pub bright: [i32; 3],
    pub contrast: [i32; 3],
    pub add: [i32; 3],
    pub mul: [f32; 3],
    hist: VecDeque<Option<Ghost>>,
}

impl Default for AfterImage {
    fn default() -> Self {
        AfterImage {
            time: 0,
            length: 20,
            timegap: 1,
            framegap: 4,
            trans: Trans::None,
            bright: [30; 3],
            contrast: [120, 120, 220],
            add: [10, 10, 25],
            mul: [0.65, 0.65, 0.75],
            hist: VecDeque::new(),
        }
    }
}

impl AfterImage {
    pub fn clear(&mut self) {
        self.hist.clear();
    }

    /// Guarda el cuadro de este tick; cuando la estela se apaga, se consume.
    pub fn record(&mut self, frame: Option<(i32, i32, bool, bool)>, pos: [f32; 2], facing: i32) {
        if self.time != 0 {
            let g = frame.map(|(group, image, hflip, vflip)| Ghost { group, image, hflip, vflip, pos, facing });
            self.hist.push_front(g);
            self.hist.truncate(self.length.max(1) as usize);
            if self.time > 0 {
                self.time -= 1;
            }
        } else if !self.hist.is_empty() {
            // Sigue avanzando para que las imágenes viejas desaparezcan.
            self.hist.push_front(None);
            self.hist.truncate(self.length.max(1) as usize);
            if self.hist.iter().all(Option::is_none) {
                self.hist.clear();
            }
        }
    }

    pub fn draw(&self, g: &Game, c: usize, frame: &mut Frame) {
        if self.hist.is_empty() {
            return;
        }
        let ch = &g.chars[c];
        let sff = &g.sffs[ch.def.sff as usize];
        let gap = self.framegap.max(1) as usize;
        let mut k = (self.hist.len() - 1) / gap * gap;
        while k >= gap {
            if let Some(gh) = self.hist[k] {
                if let Some(si) = sff.find(gh.group, gh.image) {
                    let spr = &sff.sprites[si as usize];
                    let base = &sff.pals[if spr.pal == 0 { ch.pal } else { spr.pal } as usize];
                    let step = (k / gap) as i32;
                    let mut pal = *base;
                    for col in pal.iter_mut().skip(1) {
                        let mut out = 0xff00_0000u32;
                        for i in 0..3 {
                            let v = (*col >> (8 * i) & 0xff) as i32;
                            let mut f = (v * self.contrast[i] / 256 + self.bright[i]) as f32;
                            for _ in 0..step {
                                f = (f + self.add[i] as f32) * self.mul[i];
                            }
                            out |= (f.clamp(0.0, 255.0) as u32) << (8 * i);
                        }
                        *col = out;
                    }
                    let pal = frame.dyn_pal(pal);
                    let (x, y) = g.to_screen(gh.pos);
                    let face = gh.facing as f32;
                    frame.sprite(SpriteCmd {
                        sff: ch.def.sff,
                        spr: si,
                        x,
                        y,
                        sx: face * ch.def.xscale * if gh.hflip { -1.0 } else { 1.0 },
                        sy: ch.def.yscale * if gh.vflip { -1.0 } else { 1.0 },
                        pal,
                        trans: self.trans,
                        clip: None,
                        src: None,
                    });
                }
            }
            k -= gap;
        }
    }
}

pub struct Explod {
    pub owner: usize,
    pub id: i32,
    pub anim: AnimState,
    pub sff: u16,
    /// Usa los sprites (y la paleta) del personaje dueño.
    pub own: bool,
    pub pos: [f32; 2],
    /// `pos` está en coordenadas de pantalla y no del escenario.
    pub screen: bool,
    pub vel: [f32; 2],
    pub accel: [f32; 2],
    pub facing: i32,
    pub vfacing: i32,
    /// Sigue a un personaje: (cuál, desplazamiento, ticks restantes o -1).
    pub bind: Option<(usize, [f32; 2], i32)>,
    pub removetime: i32,
    pub scale: (f32, f32),
    pub sprpriority: i32,
    pub ontop: bool,
    pub removeongethit: bool,
    pub trans: Option<Trans>,
}

impl Game {
    /// Crea un efecto de una sola pasada en un punto del escenario.
    pub fn spawn_fx(&mut self, owner: usize, own: bool, no: i32, pos: [f32; 2], facing: i32, prio: i32) {
        let (anim, sff) = if own {
            (self.chars[owner].def.anims.get(&no).cloned(), self.chars[owner].def.sff)
        } else {
            (self.fight.fx_anims.get(&no).cloned(), self.fight.fx_sff)
        };
        if anim.is_none() {
            return;
        }
        let mut st = AnimState::default();
        st.set(anim, no);
        self.explods.push(Explod {
            owner,
            id: -1,
            anim: st,
            sff,
            own,
            pos,
            screen: false,
            vel: [0.0; 2],
            accel: [0.0; 2],
            facing,
            vfacing: 1,
            bind: None,
            removetime: -2,
            scale: (1.0, 1.0),
            sprpriority: prio,
            ontop: false,
            removeongethit: false,
            trans: None,
        });
    }

    pub fn update_explods(&mut self) {
        let mut list = std::mem::take(&mut self.explods);
        for e in list.iter_mut() {
            if let Some((to, off, time)) = e.bind {
                let t = &self.chars[to];
                e.pos = [t.pos[0] + off[0] * t.facing as f32, t.pos[1] + off[1]];
                e.bind = match time {
                    t if t < 0 => Some((to, off, t)),
                    t if t > 1 => Some((to, off, t - 1)),
                    _ => None,
                };
            } else {
                e.pos[0] += e.vel[0] * e.facing as f32;
                e.pos[1] += e.vel[1];
                e.vel[0] += e.accel[0];
                e.vel[1] += e.accel[1];
            }
            e.anim.update();
            if e.removetime > 0 {
                e.removetime -= 1;
            }
        }
        list.retain(|e| {
            let gone = e.removetime == 0
                || (e.removetime == -2 && e.anim.finished())
                || (e.removeongethit && self.chars[e.owner].movetype == b'h')
                || e.anim.anim.is_none();
            !gone
        });
        // Los creados durante la actualización (no hay) se conservarían.
        list.append(&mut self.explods);
        self.explods = list;
    }

    pub fn draw_explods(&self, frame: &mut Frame, front: bool) {
        for e in &self.explods {
            if (e.ontop || e.sprpriority > 0) != front {
                continue;
            }
            let Some(f) = e.anim.frame() else { continue };
            let sff = &self.sffs[e.sff as usize];
            let Some(si) = sff.find(f.group, f.image) else { continue };
            let spr = &sff.sprites[si as usize];
            let (x, y) = if e.screen { (e.pos[0], e.pos[1]) } else { self.to_screen(e.pos) };
            let pal = if e.own {
                let ch = &self.chars[e.owner];
                PalRef::Sff(e.sff, if spr.pal == 0 { ch.pal } else { spr.pal })
            } else {
                PalRef::Sff(e.sff, spr.pal)
            };
            let face = e.facing as f32;
            frame.sprite(SpriteCmd {
                sff: e.sff,
                spr: si,
                x: x + face * f.x as f32 * e.scale.0,
                y: y + f.y as f32 * e.scale.1,
                sx: face * e.scale.0 * if f.hflip { -1.0 } else { 1.0 },
                sy: e.vfacing as f32 * e.scale.1 * if f.vflip { -1.0 } else { 1.0 },
                pal,
                trans: e.trans.unwrap_or(f.trans),
                clip: None,
                src: None,
            });
        }
    }

    /// Borde de la pantalla, en coordenadas del escenario.
    pub fn screen_edge(&self, right: bool) -> f32 {
        let half = SCREEN_W as f32 / 2.0;
        if right {
            self.cam[0] + half
        } else {
            self.cam[0] - half
        }
    }
}
