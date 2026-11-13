//! Helpers (personajes auxiliares creados por un jugador) y proyectiles.
//!
//! Un helper es un personaje más de la lista (`Game::chars`, a partir del
//! índice 2): tiene su propio estado, animación y variables, y usa los
//! archivos de quien lo creó. Un proyectil es más simple: una animación que
//! avanza sola y lleva un `HitDef`.

use crate::air::AnimState;
use crate::game::{attr_match, overlap, Char, Game, HitDef, HF_A, HF_D, HF_F, HF_H, HF_L, HF_MINUS, HF_PLUS, NOBODY};
use crate::gfx::{Frame, PalRef, SpriteCmd, SCREEN_W};
use std::rc::Rc;

/// Qué le pasó a un proyectil: índice de `Game::proj_log`.
pub const PROJ_HIT: usize = 0;
pub const PROJ_GUARDED: usize = 1;
pub const PROJ_CANCEL: usize = 2;

/// Topes, para que un personaje mal hecho no agote la memoria.
pub const MAX_HELPERS: usize = 56;
pub const MAX_PROJS: usize = 64;

pub struct Projectile {
    /// Personaje que lo lanzó y jugador al que pertenece.
    pub owner: usize,
    pub root: usize,
    pub team: usize,
    pub id: i32,
    pub anim: AnimState,
    hit_anim: i32,
    rem_anim: i32,
    cancel_anim: i32,
    pub pos: [f32; 2],
    pub facing: i32,
    vel: [f32; 2],
    rem_vel: [f32; 2],
    accel: [f32; 2],
    velmul: [f32; 2],
    scale: (f32, f32),
    /// Se retira al golpear.
    remove: bool,
    remove_time: i32,
    hits: i32,
    miss_time: i32,
    miss_left: i32,
    priority: i32,
    pub sprpriority: i32,
    edge_bound: f32,
    stage_bound: f32,
    height_bound: (f32, f32),
    hitdef: Rc<HitDef>,
    hitpause: i32,
    /// Ticks que puede moverse durante una pausa y una superpausa.
    pause_move: (i32, i32),
    /// Terminando: sólo reproduce su animación final.
    ending: bool,
}

/// Parámetros de un proyectil, ya evaluados por el controlador.
pub struct ProjSpec {
    pub id: i32,
    pub anim: i32,
    pub hit_anim: i32,
    pub rem_anim: i32,
    pub cancel_anim: i32,
    pub pos: [f32; 2],
    pub facing: i32,
    pub vel: [f32; 2],
    pub rem_vel: [f32; 2],
    pub accel: [f32; 2],
    pub velmul: [f32; 2],
    pub scale: (f32, f32),
    pub remove: bool,
    pub remove_time: i32,
    pub hits: i32,
    pub miss_time: i32,
    pub priority: i32,
    pub sprpriority: i32,
    pub edge_bound: f32,
    pub stage_bound: f32,
    pub height_bound: (f32, f32),
    pub pause_move: (i32, i32),
    pub hitdef: HitDef,
}

/// Parámetros de un helper, ya evaluados por el controlador.
pub struct HelperSpec {
    pub id: i32,
    pub pos: [f32; 2],
    pub facing: i32,
    pub stateno: i32,
    pub keyctrl: bool,
    pub own_pal: bool,
    pub scale: Option<(f32, f32)>,
    pub pause_move: (i32, i32),
}

impl Projectile {
    /// Ya terminó su animación final. Si esa animación no acaba nunca, se
    /// da por terminada: no deja el proyectil colgado.
    fn gone(&self) -> bool {
        self.ending && (self.anim.finished() || self.anim.anim.as_ref().is_some_and(|a| a.infinite))
    }

    pub(crate) fn boxes(&self) -> Vec<[f32; 4]> {
        let Some(f) = self.anim.frame() else { return Vec::new() };
        let (sx, sy) = (self.facing as f32 * self.scale.0, self.scale.1);
        f.clsn1
            .iter()
            .map(|b| {
                let (x1, x2) = (self.pos[0] + b[0] as f32 * sx, self.pos[0] + b[2] as f32 * sx);
                [x1.min(x2), self.pos[1] + b[1] as f32 * sy, x1.max(x2), self.pos[1] + b[3] as f32 * sy]
            })
            .collect()
    }
}

impl Game {
    // ------------------------------------------------------------ helpers

    /// Crea un helper de `parent`. Devuelve su índice.
    pub fn spawn_helper(&mut self, parent: usize, spec: HelperSpec) -> Option<usize> {
        // Tope de seguridad: un personaje mal hecho no agota la memoria.
        if self.chars.iter().filter(|c| c.active && c.helper).count() >= MAX_HELPERS {
            return None;
        }
        let (def, root, team, palno, pal, scale) = {
            let p = &self.chars[parent];
            (p.def.clone(), p.root, p.team, p.palno, p.pal, p.scale)
        };
        let slot = self.chars.iter().position(|c| c.helper && !c.active).unwrap_or(self.chars.len());
        let mut h = Char::new(def, slot, palno, &self.sffs);
        h.helper = true;
        h.parent = parent;
        h.root = root;
        h.team = team;
        h.helper_id = spec.id;
        h.uid = self.next_uid;
        self.next_uid += 1;
        h.pal = pal;
        h.pos = spec.pos;
        h.facing = spec.facing;
        h.keyctrl = spec.keyctrl;
        h.own_pal = spec.own_pal;
        h.scale = spec.scale.unwrap_or(scale);
        h.pause_move = spec.pause_move;
        h.born = self.tick;
        h.screenbound = false;
        h.movecamera = (false, false);
        h.ai = false;
        if slot == self.chars.len() {
            self.chars.push(h);
        } else {
            self.chars[slot] = h;
        }
        self.change_state(slot, spec.stateno, Some(slot));
        Some(slot)
    }

    /// Retira un helper y borra las referencias que otros le tuvieran. Su
    /// hueco en la lista queda libre para el siguiente.
    pub fn destroy_helper(&mut self, c: usize) {
        if !self.chars[c].helper || !self.chars[c].active {
            return;
        }
        let root = self.chars[c].root;
        {
            let ch = &mut self.chars[c];
            ch.active = false;
            ch.assert = 0;
            ch.movecamera = (false, false);
            ch.hitdef_active = false;
            ch.reversal = None;
            ch.bind = None;
            ch.targets.clear();
        }
        for ch in self.chars.iter_mut() {
            ch.targets.retain(|&t| t != c);
            if ch.bind.is_some_and(|b| b.to == c) {
                ch.bind = None;
            }
            // Quien estuviera en un estado suyo sigue en él: la tabla de
            // estados es la misma que la de su jugador.
            if ch.state_owner == c {
                ch.state_owner = root;
            }
            if ch.helper && ch.parent == c {
                ch.parent = NOBODY;
            }
        }
        for e in self.explods.iter_mut() {
            if e.bind.is_some_and(|b| b.0 == c) {
                e.bind = None;
            }
            if e.owner == c {
                e.owner = root;
            }
        }
        // Sus proyectiles siguen volando a nombre del jugador.
        for p in self.projs.iter_mut() {
            if p.owner == c {
                p.owner = root;
            }
        }
        if self.pause.by == c {
            self.pause.by = root;
        }
    }

    /// Helpers vivos del mismo jugador que `c` con ese identificador
    /// (cualquiera si es negativo).
    pub fn helpers_of(&self, c: usize, id: i32) -> impl Iterator<Item = usize> + '_ {
        let root = self.chars[c].root;
        self.chars.iter().enumerate().filter(move |(_, h)| h.active && h.helper && h.root == root && (id <= 0 || h.helper_id == id)).map(|(i, _)| i)
    }

    // -------------------------------------------------------- proyectiles

    pub fn spawn_projectile(&mut self, owner: usize, spec: ProjSpec) {
        if self.projs.len() >= MAX_PROJS {
            return;
        }
        let ch = &self.chars[owner];
        let anim = ch.def.anims.get(&spec.anim).cloned();
        if anim.is_none() {
            self.warn(format!("proyectil con animación {} inexistente", spec.anim));
            return;
        }
        let mut st = AnimState::default();
        st.set(anim, spec.anim);
        let (root, team) = (ch.root, ch.team);
        self.projs.push(Projectile {
            owner,
            root,
            team,
            id: spec.id,
            anim: st,
            hit_anim: spec.hit_anim,
            rem_anim: spec.rem_anim,
            cancel_anim: spec.cancel_anim,
            pos: spec.pos,
            facing: spec.facing,
            vel: spec.vel,
            rem_vel: spec.rem_vel,
            accel: spec.accel,
            velmul: spec.velmul,
            scale: spec.scale,
            remove: spec.remove,
            remove_time: spec.remove_time,
            hits: spec.hits.max(1),
            miss_time: spec.miss_time,
            miss_left: 0,
            priority: spec.priority,
            sprpriority: spec.sprpriority,
            edge_bound: spec.edge_bound,
            stage_bound: spec.stage_bound,
            height_bound: spec.height_bound,
            hitdef: Rc::new(spec.hitdef),
            hitpause: 0,
            pause_move: spec.pause_move,
            ending: false,
        });
    }

    /// Proyectiles en vuelo del mismo jugador que `c` con ese identificador
    /// (cualquiera si es negativo).
    pub fn count_projs(&self, c: usize, id: i32) -> i32 {
        let root = self.chars[c].root;
        self.projs.iter().filter(|p| p.root == root && !p.ending && (id < 0 || p.id == id)).count() as i32
    }

    /// Apunta que un proyectil de `root` golpeó, fue bloqueado o anulado.
    fn log_proj(&mut self, root: usize, id: i32, what: usize) {
        let tick = self.tick;
        match self.proj_log.iter_mut().find(|e| e.0 == root && e.1 == id) {
            Some(e) => e.2[what] = tick,
            None => {
                let mut times = [-1; 3];
                times[what] = tick;
                self.proj_log.push((root, id, times));
            }
        }
    }

    /// Ticks desde que un proyectil del jugador de `c` golpeó (`what`), o -1
    /// si no ha pasado. `id` negativo = cualquier proyectil; `what` = 3
    /// significa "golpeó o fue bloqueado".
    pub fn proj_time(&self, c: usize, id: i32, what: usize) -> i32 {
        let root = self.chars[c].root;
        let last = self
            .proj_log
            .iter()
            .filter(|e| e.0 == root && (id <= 0 || e.1 == id))
            .map(|e| if what == 3 { e.2[PROJ_HIT].max(e.2[PROJ_GUARDED]) } else { e.2[what] })
            .max()
            .unwrap_or(-1);
        // Ocurrió al final del tick `last`: en el siguiente han pasado 0.
        if last < 0 {
            -1
        } else {
            (self.tick - last - 1).max(0)
        }
    }

    /// Pasa a la animación final del proyectil (o lo retira si no la tiene).
    fn end_projectile(&mut self, i: usize, anim: i32) {
        let owner = self.projs[i].owner;
        let a = if anim >= 0 { self.chars[owner].def.anims.get(&anim).cloned() } else { None };
        let p = &mut self.projs[i];
        p.ending = true;
        match a {
            Some(a) => p.anim.set(Some(a), anim),
            None => p.anim.set(None, -1),
        }
    }

    pub fn update_projs(&mut self) {
        let half = SCREEN_W as f32 / 2.0;
        let (cam, left, right) = (self.cam[0], self.stage.left_bound, self.stage.right_bound);
        let (paused, darken) = (self.pause.on(), self.pause.darken);
        let mut expired = Vec::new();
        for (i, p) in self.projs.iter_mut().enumerate() {
            if paused {
                // Durante una pausa sólo se mueve el tiempo que tenga concedido.
                let allowed = if darken { &mut p.pause_move.1 } else { &mut p.pause_move.0 };
                if *allowed <= 0 {
                    continue;
                }
                *allowed -= 1;
            }
            if p.hitpause > 0 {
                p.hitpause -= 1;
                continue;
            }
            if p.miss_left > 0 {
                p.miss_left -= 1;
            }
            let v = if p.ending { p.rem_vel } else { p.vel };
            p.pos[0] += v[0] * p.facing as f32;
            p.pos[1] += v[1];
            if !p.ending {
                p.vel[0] = (p.vel[0] + p.accel[0]) * p.velmul[0];
                p.vel[1] = (p.vel[1] + p.accel[1]) * p.velmul[1];
            }
            p.anim.update();
            if !p.ending && p.remove_time > 0 {
                p.remove_time -= 1;
                if p.remove_time == 0 {
                    expired.push(i);
                }
            }
            // Fuera de la pantalla, del escenario o de la altura permitida.
            let off_screen = p.pos[0] < cam - half - p.edge_bound || p.pos[0] > cam + half + p.edge_bound;
            let off_stage = p.pos[0] < left - p.stage_bound || p.pos[0] > right + p.stage_bound;
            if off_screen || off_stage || p.pos[1] < p.height_bound.0 || p.pos[1] > p.height_bound.1 {
                p.ending = true;
                p.anim.set(None, -1);
            }
        }
        for i in expired {
            let anim = self.projs[i].rem_anim;
            self.end_projectile(i, anim);
        }
        self.projs.retain(|p| !p.gone());
    }

    /// ¿Puede este golpe alcanzar a `d` tal como está ahora? `juggle` es lo
    /// que cuesta golpearlo si está cayendo (`None` = ya está pagado).
    pub(crate) fn hittable(&self, d: usize, h: &HitDef, juggle: Option<i32>) -> bool {
        let dch = &self.chars[d];
        if self.round.state > 3 || (self.round.state == 3 && self.round.over_time > self.fight.over_hittime) {
            return false;
        }
        let hit_state = dch.movetype == b'h';
        let falling = hit_state && dch.ghv.fall;
        let ok = match dch.stype {
            b's' => h.hitflag & HF_H != 0,
            b'c' => h.hitflag & HF_L != 0,
            b'a' => h.hitflag & HF_A != 0 && (!falling || h.hitflag & HF_F != 0),
            _ => h.hitflag & HF_D != 0,
        };
        if !ok || (h.hitflag & HF_PLUS != 0 && !hit_state) || (h.hitflag & HF_MINUS != 0 && hit_state) {
            return false;
        }
        for &(mask, time, hitby) in &dch.nothitby {
            if time > 0 && attr_match(h.attr, mask) != hitby {
                return false;
            }
        }
        // Malabares: un rival que cae sólo se puede golpear si quedan puntos.
        !(falling && juggle.is_some_and(|cost| dch.juggle_left < cost))
    }

    /// Golpes de los proyectiles contra personajes, y choques entre
    /// proyectiles de bandos contrarios.
    pub fn resolve_proj_hits(&mut self) {
        // Proyectil contra proyectil: gana el de más prioridad.
        for i in 0..self.projs.len() {
            for j in i + 1..self.projs.len() {
                let (a, b) = (&self.projs[i], &self.projs[j]);
                if a.ending || b.ending || a.team == b.team || !overlap(&a.boxes(), &b.boxes()) {
                    continue;
                }
                let (pa, pb) = (a.priority, b.priority);
                let (ra, rb, ia, ib, ca, cb) = (a.root, b.root, a.id, b.id, a.cancel_anim, b.cancel_anim);
                if pa <= pb {
                    self.end_projectile(i, ca);
                    self.log_proj(ra, ia, PROJ_CANCEL);
                } else {
                    self.projs[i].priority -= 1;
                }
                if pb <= pa {
                    self.end_projectile(j, cb);
                    self.log_proj(rb, ib, PROJ_CANCEL);
                } else {
                    self.projs[j].priority -= 1;
                }
            }
        }
        for i in 0..self.projs.len() {
            let p = &self.projs[i];
            if p.ending || p.hitpause > 0 || p.miss_left > 0 {
                continue;
            }
            let (team, owner, boxes, h) = (p.team, p.owner, p.boxes(), p.hitdef.clone());
            for d in 0..self.chars.len() {
                let dch = &self.chars[d];
                if !dch.active || dch.team == team || !self.hittable(d, &h, Some(0)) || !overlap(&boxes, &dch.boxes(false)) {
                    continue;
                }
                // El golpe se aplica como si lo diera el dueño desde donde
                // está el proyectil; después se le devuelve su estado.
                let (pos, facing) = (self.projs[i].pos, self.projs[i].facing);
                let saved = self.chars[owner].attack_snapshot();
                {
                    let o = &mut self.chars[owner];
                    o.pos = pos;
                    o.facing = facing;
                    // En el aire no hay empuje de esquina para el atacante.
                    o.stype = b'a';
                    o.juggle_paid = true;
                }
                self.apply_hit(owner, d, &h, true);
                let pause = self.chars[owner].hitpause;
                self.chars[owner].restore_attack(saved);
                let guarded = self.chars[d].ghv.guarded;
                let (root, id) = (self.projs[i].root, self.projs[i].id);
                self.log_proj(root, id, if guarded { PROJ_GUARDED } else { PROJ_HIT });
                let p = &mut self.projs[i];
                p.hitpause = pause;
                p.hits -= 1;
                if p.hits <= 0 {
                    if p.remove {
                        let anim = p.hit_anim;
                        self.end_projectile(i, anim);
                    }
                } else {
                    p.miss_left = p.miss_time;
                }
                break;
            }
        }
        self.projs.retain(|p| !p.gone());
    }

    /// ¿Hay un proyectil enemigo lo bastante cerca de `c` como para cubrirse?
    pub fn proj_in_guard_dist(&self, c: usize) -> bool {
        let ch = &self.chars[c];
        self.projs.iter().any(|p| {
            let dist = p.hitdef.guard_dist.unwrap_or(self.chars[p.owner].def.proj_attack_dist);
            !p.ending && p.team != ch.team && (ch.pos[0] - p.pos[0]) * p.facing as f32 > -ch.width(true) && (ch.pos[0] - p.pos[0]).abs() < dist
        })
    }

    pub fn draw_projs(&self, frame: &mut Frame, front: bool) {
        for p in &self.projs {
            if (p.sprpriority > 0) != front {
                continue;
            }
            let Some(f) = p.anim.frame() else { continue };
            let ch = &self.chars[p.owner];
            let sff = &self.sffs[ch.def.sff as usize];
            let Some(si) = sff.find(f.group, f.image) else { continue };
            let spr = &sff.sprites[si as usize];
            let (x, y) = self.to_screen(p.pos);
            let face = p.facing as f32;
            frame.sprite(SpriteCmd {
                sff: ch.def.sff,
                spr: si,
                x: x + face * f.x as f32 * p.scale.0,
                y: y + f.y as f32 * p.scale.1,
                sx: face * p.scale.0 * if f.hflip { -1.0 } else { 1.0 },
                sy: p.scale.1 * if f.vflip { -1.0 } else { 1.0 },
                pal: PalRef::Sff(ch.def.sff, if spr.pal == 0 { ch.pal } else { spr.pal }),
                trans: f.trans,
                clip: None,
                src: None,
            });
        }
    }
}
