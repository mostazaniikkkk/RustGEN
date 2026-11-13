//! Escenario: definición (`stages/*.def`), fondo y límites de cámara.

use crate::air::{self, AnimTable};
use crate::bg::BgSet;
use crate::gfx::Frame;
use crate::ini;
use crate::sff::Sff;

pub struct Stage {
    pub name: String,
    pub sff: u16,
    bg: BgSet,
    anims: AnimTable,
    pub bound_left: f32,
    pub bound_right: f32,
    pub bound_high: f32,
    pub bound_low: f32,
    pub verticalfollow: f32,
    pub floortension: f32,
    pub tension: f32,
    pub p_start: [(f32, f32); 2],
    pub p_facing: [i32; 2],
    pub left_bound: f32,
    pub right_bound: f32,
    pub screen_left: f32,
    pub screen_right: f32,
    pub zoffset: f32,
    pub shadow_intensity: i32,
    pub shadow_color: (u8, u8, u8),
    pub shadow_yscale: f32,
    pub reflection: i32,
}

/// Nombre de un escenario sin cargarlo entero.
pub fn stage_name(path: &str) -> Option<String> {
    let secs = ini::load(path).ok()?;
    secs.iter().find(|s| s.name == "info").and_then(|s| s.str("displayname").or_else(|| s.str("name")))
}

impl Stage {
    /// Carga el escenario y añade su SFF a `sffs`.
    pub fn load(path: &str, sffs: &mut Vec<Sff>) -> Result<Stage, String> {
        let text = ini::read_text(path).map_err(|e| format!("{path}: {e}"))?;
        let secs = ini::parse(&text);
        let sec = |n: &str| secs.iter().find(|s| s.name == n);
        let f = |n: &str, k: &str, d: f32| sec(n).map_or(d, |s| s.f32(k, d));
        let before = sffs.len();
        let bg = BgSet::load(&secs, "bg", path, u16::MAX, sffs);
        if sffs.len() == before {
            return Err(format!("{path}: no se pudo cargar el SFF del fondo"));
        }
        let (cr, cg, cb) = {
            let l = sec("shadow").map(|s| s.list("color", 0.0)).unwrap_or_default();
            let at = |i: usize| l.get(i).copied().unwrap_or(0.0) as u8;
            (at(0), at(1), at(2))
        };
        Ok(Stage {
            name: sec("info").and_then(|s| s.str("name")).unwrap_or_default(),
            sff: bg.sff,
            bg,
            anims: air::parse(&text),
            bound_left: f("camera", "boundleft", -150.0),
            bound_right: f("camera", "boundright", 150.0),
            bound_high: f("camera", "boundhigh", -25.0),
            bound_low: f("camera", "boundlow", 0.0),
            verticalfollow: f("camera", "verticalfollow", 0.2),
            floortension: f("camera", "floortension", 0.0),
            tension: f("camera", "tension", 50.0),
            p_start: [
                (f("playerinfo", "p1startx", -70.0), f("playerinfo", "p1starty", 0.0)),
                (f("playerinfo", "p2startx", 70.0), f("playerinfo", "p2starty", 0.0)),
            ],
            p_facing: [f("playerinfo", "p1facing", 1.0) as i32, f("playerinfo", "p2facing", -1.0) as i32],
            left_bound: f("playerinfo", "leftbound", -1000.0),
            right_bound: f("playerinfo", "rightbound", 1000.0),
            screen_left: f("bound", "screenleft", 15.0),
            screen_right: f("bound", "screenright", 15.0),
            zoffset: f("stageinfo", "zoffset", 200.0),
            shadow_intensity: f("shadow", "intensity", 128.0) as i32,
            shadow_color: (cr, cg, cb),
            shadow_yscale: f("shadow", "yscale", 0.4),
            reflection: f("reflection", "intensity", 0.0) as i32,
        })
    }

    /// Dibuja las capas de `layer` (0 = detrás de los personajes, 1 = delante).
    pub fn draw(&self, frame: &mut Frame, sffs: &[Sff], cam: [f32; 2], shake_y: f32, tick: i32, layer: i32) {
        self.bg.draw(frame, sffs, &self.anims, cam, shake_y, tick, layer);
    }
}
