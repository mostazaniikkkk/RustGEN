//! Storyboards: secuencias de escenas con capas animadas (logo, intro,
//! finales, créditos).

use crate::air::{self, AnimTable};
use crate::bg::{rgb, BgSet};
use crate::gfx::{Frame, PalRef, SpriteCmd, Trans, SCREEN_H, SCREEN_W};
use crate::ini;
use crate::sff::Sff;

struct Layer {
    anim: i32,
    offset: (f32, f32),
    start: i32,
}

struct Scene {
    fadein: (i32, u32),
    fadeout: (i32, u32),
    clear: u32,
    pos: (f32, f32),
    layers: Vec<Layer>,
    bg: Option<BgSet>,
    end: i32,
}

pub struct Story {
    scenes: Vec<Scene>,
    anims: AnimTable,
    sff: u16,
    scene: usize,
    t: i32,
}

/// Velo de un color con opacidad `alpha` (0 a 256) sobre toda la pantalla.
pub fn fade(frame: &mut Frame, color: u32, alpha: i32) {
    let a = alpha.clamp(0, 256) as u16;
    if a > 0 {
        frame.rect(0, 0, SCREEN_W, SCREEN_H, color, Trans::Add { src: a, dst: 256 - a });
    }
}

impl Story {
    /// Carga el storyboard y añade sus sprites a `sffs`.
    pub fn load(path: &str, sffs: &mut Vec<Sff>) -> Result<Story, String> {
        let text = ini::read_text(path).map_err(|e| format!("{path}: {e}"))?;
        let secs = ini::parse(&text);
        let def = secs.iter().find(|s| s.name == "scenedef").ok_or(format!("{path}: falta [SceneDef]"))?;
        let file = def.str("spr").ok_or(format!("{path}: falta spr"))?;
        sffs.push(Sff::load(&ini::sibling(path, &file))?);
        let sff = sffs.len() as u16 - 1;

        let mut scenes = Vec::new();
        // El color de fondo y la posición de las capas se heredan de la
        // escena anterior.
        let (mut clear, mut pos) = (0xff00_0000u32, (0.0, 0.0));
        for s in secs.iter().filter(|s| s.name.starts_with("scene ")) {
            if s.get("clearcolor").is_some() {
                clear = rgb(&s.list("clearcolor", 0.0));
            }
            if s.get("layerall.pos").is_some() {
                pos = s.pair("layerall.pos", 0.0, 0.0);
            }
            let color = |k: &str| s.get(k).map_or(0xff00_0000, |_| rgb(&s.list(k, 0.0)));
            let mut layers = Vec::new();
            for n in 0..10 {
                let key = |k: &str| format!("layer{n}.{k}");
                if let Some(anim) = s.get(&key("anim")).and_then(ini::num) {
                    layers.push(Layer { anim: anim as i32, offset: s.pair(&key("offset"), 0.0, 0.0), start: s.i32(&key("starttime"), 0) });
                }
            }
            let bg = s.str("bg.name").filter(|n| !n.is_empty()).map(|n| BgSet::load(&secs, &n.to_ascii_lowercase(), path, sff, sffs));
            scenes.push(Scene {
                fadein: (s.i32("fadein.time", 0), color("fadein.col")),
                fadeout: (s.i32("fadeout.time", 0), color("fadeout.col")),
                clear,
                pos,
                layers,
                bg,
                end: s.i32("end.time", 1).max(1),
            });
        }
        if scenes.is_empty() {
            return Err(format!("{path}: sin escenas"));
        }
        let start = (def.i32("startscene", 0).max(0) as usize).min(scenes.len() - 1);
        Ok(Story { scenes, anims: air::parse(&text), sff, scene: start, t: 0 })
    }

    /// Avanza un tick. Devuelve `true` cuando terminó (o se saltó).
    pub fn tick(&mut self, skip: bool) -> bool {
        if skip {
            return true;
        }
        self.t += 1;
        if self.t >= self.scenes[self.scene].end {
            self.scene += 1;
            self.t = 0;
        }
        self.scene >= self.scenes.len()
    }

    pub fn draw(&self, frame: &mut Frame, sffs: &[Sff]) {
        let Some(sc) = self.scenes.get(self.scene) else {
            frame.clear = 0;
            return;
        };
        frame.clear = sc.clear & 0x00ff_ffff;
        if let Some(bg) = &sc.bg {
            bg.draw(frame, sffs, &self.anims, [0.0, 0.0], 0.0, self.t, 0);
        }
        let sff = &sffs[self.sff as usize];
        for l in &sc.layers {
            let Some(f) = self.anims.get(&l.anim).and_then(|a| a.frame_at(self.t - l.start, true)) else { continue };
            let Some(si) = sff.find(f.group, f.image) else { continue };
            frame.sprite(SpriteCmd {
                sff: self.sff,
                spr: si,
                x: sc.pos.0 + l.offset.0 + f.x as f32,
                y: sc.pos.1 + l.offset.1 + f.y as f32,
                sx: if f.hflip { -1.0 } else { 1.0 },
                sy: if f.vflip { -1.0 } else { 1.0 },
                pal: PalRef::Sff(self.sff, sff.sprites[si as usize].pal),
                trans: f.trans,
                clip: None,
                src: None,
            });
        }
        if let Some(bg) = &sc.bg {
            bg.draw(frame, sffs, &self.anims, [0.0, 0.0], 0.0, self.t, 1);
        }
        if sc.fadein.0 > 0 && self.t < sc.fadein.0 {
            fade(frame, sc.fadein.1, 256 - 256 * self.t / sc.fadein.0);
        }
        let left = sc.end - self.t;
        if sc.fadeout.0 > 0 && left <= sc.fadeout.0 {
            fade(frame, sc.fadeout.1, 256 - 256 * left / sc.fadeout.0);
        }
    }
}
