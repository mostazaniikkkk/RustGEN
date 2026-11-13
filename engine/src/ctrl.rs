//! Controladores de estado: lo que hace cada `[State n, x]` cuando se cumple.

use crate::air::{parse_trans, AnimState};
use crate::cns::{Ctrl, Kind};
use crate::ents::{HelperSpec, ProjSpec};
use crate::expr::{self, parse_attr, Trig, V, AT_A};
use crate::fx::{Explod, PalFx};
use crate::game::*;
use crate::gfx::{SoundCmd, Trans, SCREEN_W};
use std::rc::Rc;

fn var_key(key: &str) -> Option<(Trig, Option<i32>)> {
    let (name, idx) = match key.find('(') {
        Some(p) => (&key[..p], crate::ini::num(&key[p + 1..]).map(|v| v as i32)),
        None => (key, None),
    };
    let kind = match name.trim() {
        "var" | "v" => Trig::Var,
        "fvar" | "fv" => Trig::FVar,
        "sysvar" => Trig::SysVar,
        "sysfvar" => Trig::SysFVar,
        _ => return None,
    };
    Some((kind, idx))
}

fn assert_flag(name: &str) -> u32 {
    match name.trim().to_ascii_lowercase().as_str() {
        "intro" => AS_INTRO,
        "nowalk" => AS_NOWALK,
        "noautoturn" => AS_NOAUTOTURN,
        "invisible" => AS_INVISIBLE,
        "noshadow" => AS_NOSHADOW,
        "roundnotover" => AS_ROUNDNOTOVER,
        "nojugglecheck" => AS_NOJUGGLECHECK,
        "unguardable" => AS_UNGUARDABLE,
        "nostandguard" => AS_NOSTANDGUARD,
        "nocrouchguard" => AS_NOCROUCHGUARD,
        "noairguard" => AS_NOAIRGUARD,
        "noko" => AS_NOKO,
        "nobardisplay" => AS_NOBARDISPLAY,
        "timerfreeze" => AS_TIMERFREEZE,
        "nokoslow" => AS_NOKOSLOW,
        _ => 0,
    }
}

impl Game {
    fn pf(&self, c: usize, ctrl: &Ctrl, key: &str, i: usize, default: f32) -> f32 {
        ctrl.expr(key, i).map_or(default, |e| self.eval(c, e).f())
    }

    fn pi(&self, c: usize, ctrl: &Ctrl, key: &str, i: usize, default: i32) -> i32 {
        ctrl.expr(key, i).map_or(default, |e| self.eval(c, e).i())
    }

    fn popt(&self, c: usize, ctrl: &Ctrl, key: &str, i: usize) -> Option<V> {
        ctrl.expr(key, i).map(|e| self.eval(c, e))
    }

    /// Valor con prefijo opcional de una letra (`S100`, `F5, 0`): devuelve
    /// si lo llevaba y los números.
    fn prefixed(&self, c: usize, raw: Option<&str>, letter: char) -> Option<(bool, Vec<V>)> {
        let raw = raw?.trim();
        if raw.is_empty() {
            return None;
        }
        let b = raw.as_bytes();
        let first = b[0].to_ascii_lowercase();
        // Hay prefijo si la letra va seguida de algo que no sea otra letra.
        let pref = (first == b's' || first == b'f') && b.len() > 1 && !b[1].is_ascii_alphabetic() && b[1] != b'(';
        let (has, rest) = if pref { (first as char == letter, &raw[1..]) } else { (false, raw) };
        let vals: Vec<V> = expr::parse_list(rest).into_iter().map(|e| e.map_or(V::I(0), |e| self.eval(c, &e))).collect();
        Some((has, vals))
    }

    fn sound_spec(&self, c: usize, raw: Option<&str>) -> Option<(bool, i32, i32)> {
        let (own, v) = self.prefixed(c, raw, 's')?;
        Some((own, v.first()?.i(), v.get(1).map_or(0, |x| x.i())))
    }

    fn palfx_from(&self, c: usize, ctrl: &Ctrl, prefix: &str) -> Option<PalFx> {
        let key = |k: &str| format!("{prefix}{k}");
        let time = self.popt(c, ctrl, &key("time"), 0)?.i();
        let mut p = PalFx { time, ..Default::default() };
        for i in 0..3 {
            p.add[i] = self.pi(c, ctrl, &key("add"), i, 0);
            p.mul[i] = self.pi(c, ctrl, &key("mul"), i, 256);
            p.sinadd[i] = self.pi(c, ctrl, &key("sinadd"), i, 0);
        }
        p.period = self.pi(c, ctrl, &key("sinadd"), 3, 1);
        p.invert = self.pi(c, ctrl, &key("invertall"), 0, 0) != 0;
        p.color = self.pi(c, ctrl, &key("color"), 0, 256);
        Some(p)
    }

    fn build_hitdef(&self, c: usize, ctrl: &Ctrl) -> HitDef {
        let ch = &self.chars[c];
        let f = |k: &str, i: usize, d: f32| self.pf(c, ctrl, k, i, d);
        let n = |k: &str, i: usize, d: i32| self.pi(c, ctrl, k, i, d);
        let animtype = |k: &str, d: i32| {
            ctrl.raw(k).and_then(|s| s.trim().bytes().next()).map_or(d, |b| match b.to_ascii_lowercase() {
                b'l' => 0,
                b'm' => 1,
                b'h' => 2,
                b'b' => 3,
                b'u' => 4,
                b'd' => 5,
                _ => d,
            })
        };
        let htype = |k: &str, d: i32| {
            ctrl.raw(k).and_then(|s| s.trim().bytes().next()).map_or(d, |b| match b.to_ascii_lowercase() {
                b'h' => 1,
                b'l' => 2,
                b't' => 3,
                b'n' => 0,
                _ => d,
            })
        };
        let spark = |k: &str, d: i32| match self.prefixed(c, ctrl.raw(k), 's') {
            Some((own, v)) => (own, v.first().map_or(d, |x| x.i())),
            None => (false, d),
        };
        let attr = {
            let a = parse_attr(ctrl.raw("attr").unwrap_or("s,na"));
            if a & crate::expr::AT_STATE == 0 {
                a | state_attr(ch.stype)
            } else {
                a
            }
        };
        let anim = animtype("animtype", 0);
        let air_anim = animtype("air.animtype", anim);
        let damage = (n("damage", 0, 0), n("damage", 1, 0));
        let pausetime = (n("pausetime", 0, 0), n("pausetime", 1, 0));
        let ground_type = htype("ground.type", 1);
        let ground_slidetime = n("ground.slidetime", 0, 0);
        let ground_hittime = n("ground.hittime", 0, 0);
        let guard_slidetime = n("guard.slidetime", 0, ground_slidetime);
        let guard_ctrltime = n("guard.ctrltime", 0, guard_slidetime);
        let ground_vel = (f("ground.velocity", 0, 0.0), f("ground.velocity", 1, 0.0));
        let guard_vel = f("guard.velocity", 0, ground_vel.0);
        let air_vel = (f("air.velocity", 0, 0.0), f("air.velocity", 1, 0.0));
        let ground_corner = f("ground.cornerpush.veloff", 0, if attr & AT_A != 0 { 0.0 } else { guard_vel * 1.3 });
        let guard_corner = f("guard.cornerpush.veloff", 0, ground_corner);
        let fall = n("fall", 0, 0) != 0;
        let shake = |p: &str| {
            let freq = f(&format!("{p}envshake.freq"), 0, 60.0);
            (
                n(&format!("{p}envshake.time"), 0, 0),
                freq,
                n(&format!("{p}envshake.ampl"), 0, -4),
                f(&format!("{p}envshake.phase"), 0, if freq >= 90.0 { 90.0 } else { 0.0 }),
            )
        };
        HitDef {
            attr,
            hitflag: parse_hitflag(ctrl.raw("hitflag").unwrap_or("MAF")),
            guardflag: parse_hitflag(ctrl.raw("guardflag").unwrap_or("")),
            animtype: anim,
            air_animtype: air_anim,
            fall_animtype: animtype("fall.animtype", if air_anim == 4 { 4 } else { 3 }),
            damage,
            pausetime,
            guard_pausetime: (n("guard.pausetime", 0, pausetime.0), n("guard.pausetime", 1, pausetime.1)),
            sparkno: spark("sparkno", ch.def.sparkno),
            guard_sparkno: spark("guard.sparkno", ch.def.guard_sparkno),
            sparkxy: (f("sparkxy", 0, 0.0), f("sparkxy", 1, 0.0)),
            hitsound: self.sound_spec(c, ctrl.raw("hitsound")),
            guardsound: self.sound_spec(c, ctrl.raw("guardsound")),
            ground_type,
            air_type: htype("air.type", ground_type),
            ground_slidetime,
            guard_slidetime,
            ground_hittime,
            guard_hittime: n("guard.hittime", 0, ground_hittime),
            air_hittime: n("air.hittime", 0, 20),
            guard_ctrltime,
            airguard_ctrltime: n("airguard.ctrltime", 0, guard_ctrltime),
            guard_dist: self.popt(c, ctrl, "guard.dist", 0).map(V::f),
            yaccel: f("yaccel", 0, 0.35),
            ground_vel,
            guard_vel,
            air_vel,
            airguard_vel: (f("airguard.velocity", 0, air_vel.0 * 1.5), f("airguard.velocity", 1, air_vel.1 * 0.5)),
            ground_cornerpush: ground_corner,
            air_cornerpush: f("air.cornerpush.veloff", 0, ground_corner),
            down_cornerpush: f("down.cornerpush.veloff", 0, ground_corner),
            guard_cornerpush: guard_corner,
            airguard_cornerpush: f("airguard.cornerpush.veloff", 0, guard_corner),
            p1sprpriority: n("p1sprpriority", 0, 1),
            p2sprpriority: n("p2sprpriority", 0, 0),
            p1facing: n("p1facing", 0, 0),
            p1getp2facing: n("p1getp2facing", 0, 0),
            p2facing: n("p2facing", 0, 0),
            p1stateno: self.popt(c, ctrl, "p1stateno", 0).map(V::i),
            p2stateno: self.popt(c, ctrl, "p2stateno", 0).map(V::i),
            p2getp1state: n("p2getp1state", 0, 1) != 0,
            forcestand: n("forcestand", 0, (ground_vel.1 != 0.0) as i32) != 0,
            fall,
            air_fall: n("air.fall", 0, fall as i32) != 0,
            fall_xvel: self.popt(c, ctrl, "fall.xvelocity", 0).map(V::f),
            fall_yvel: f("fall.yvelocity", 0, -4.5),
            fall_recover: n("fall.recover", 0, 1) != 0,
            fall_recovertime: n("fall.recovertime", 0, 4),
            fall_damage: n("fall.damage", 0, 0),
            down_vel: (f("down.velocity", 0, air_vel.0), f("down.velocity", 1, air_vel.1)),
            down_hittime: n("down.hittime", 0, 20),
            down_bounce: n("down.bounce", 0, 0) != 0,
            chainid: n("chainid", 0, -1),
            kill: n("kill", 0, 1) != 0,
            guard_kill: n("guard.kill", 0, 1) != 0,
            fall_kill: n("fall.kill", 0, 1) != 0,
            numhits: n("numhits", 0, 1),
            // En enteros: la FPU de la PS2 trunca y 30 * 0.7 daría 20.
            getpower: (n("getpower", 0, damage.0 * 7 / 10), n("getpower", 1, damage.1 * 7 / 10)),
            givepower: (n("givepower", 0, damage.0 * 6 / 10), n("givepower", 1, damage.1 * 6 / 10)),
            palfx: self.palfx_from(c, ctrl, "palfx."),
            envshake: shake(""),
            fall_envshake: shake("fall."),
        }
    }

    /// Dónde aparece y hacia dónde mira algo que crea `c` (un helper o un
    /// proyectil), según su `postype` y el desplazamiento del parámetro `key`.
    fn spawn_point(&self, c: usize, ctrl: &Ctrl, key: &str, proj: bool) -> ([f32; 2], i32) {
        let (x, y) = (self.pf(c, ctrl, key, 0, 0.0), self.pf(c, ctrl, key, 1, 0.0));
        let ch = &self.chars[c];
        let o = &self.chars[self.enemy(c)];
        let (left, right) = (self.screen_edge(false), self.screen_edge(true));
        let turn = if self.pi(c, ctrl, "facing", 0, 1) < 0 { -1 } else { 1 };
        let postype = ctrl.raw("postype").unwrap_or("p1").trim().to_ascii_lowercase();
        // Un proyectil mira siempre como quien lo lanza.
        let (pos, facing) = match postype.as_str() {
            "p2" => ([o.pos[0] + x * o.facing as f32, o.pos[1] + y], o.facing * turn),
            "front" => ([if ch.facing > 0 { right + x } else { left - x }, y], turn),
            "back" => ([if ch.facing > 0 { left + x } else { right - x }, y], turn),
            "left" => ([left + x, y], turn),
            "right" => ([right + x, y], turn),
            _ => ([ch.pos[0] + x * ch.facing as f32, ch.pos[1] + y], ch.facing * turn),
        };
        (pos, if proj { ch.facing } else { facing })
    }

    fn make_projectile(&mut self, c: usize, ctrl: &Ctrl) {
        let (pos, facing) = self.spawn_point(c, ctrl, "offset", true);
        let n = |k: &str, i: usize, d: i32| self.pi(c, ctrl, k, i, d);
        let f = |k: &str, i: usize, d: f32| self.pf(c, ctrl, k, i, d);
        let hit_anim = n("projhitanim", 0, -1);
        let rem_anim = n("projremanim", 0, hit_anim);
        let spec = ProjSpec {
            id: n("projid", 0, 0),
            anim: n("projanim", 0, 0),
            hit_anim,
            rem_anim,
            cancel_anim: n("projcancelanim", 0, rem_anim),
            pos,
            facing,
            vel: [f("velocity", 0, 0.0), f("velocity", 1, 0.0)],
            rem_vel: [f("remvelocity", 0, 0.0), f("remvelocity", 1, 0.0)],
            accel: [f("accel", 0, 0.0), f("accel", 1, 0.0)],
            velmul: [f("velmul", 0, 1.0), f("velmul", 1, 1.0)],
            scale: (f("projscale", 0, 1.0), f("projscale", 1, 1.0)),
            remove: n("projremove", 0, 1) != 0,
            remove_time: n("projremovetime", 0, -1),
            hits: n("projhits", 0, 1),
            miss_time: n("projmisstime", 0, 0),
            priority: n("projpriority", 0, 1),
            sprpriority: n("projsprpriority", 0, 3),
            edge_bound: f("projedgebound", 0, 40.0),
            stage_bound: f("projstagebound", 0, 40.0),
            height_bound: (f("projheightbound", 0, -240.0), f("projheightbound", 1, 1.0)),
            pause_move: (n("pausemovetime", 0, 0), n("supermovetime", 0, 0)),
            hitdef: self.build_hitdef(c, ctrl),
        };
        self.spawn_projectile(c, spec);
    }

    fn make_helper(&mut self, c: usize, ctrl: &Ctrl) {
        let (pos, facing) = self.spawn_point(c, ctrl, "pos", false);
        let own = self.chars[c].scale;
        let scale = match (self.popt(c, ctrl, "size.xscale", 0), self.popt(c, ctrl, "size.yscale", 0)) {
            (None, None) => None,
            (x, y) => Some((x.map_or(own.0, V::f), y.map_or(own.1, V::f))),
        };
        let spec = HelperSpec {
            id: self.pi(c, ctrl, "id", 0, 0),
            pos,
            facing,
            stateno: self.pi(c, ctrl, "stateno", 0, 0),
            keyctrl: self.pi(c, ctrl, "keyctrl", 0, 0) != 0,
            own_pal: self.pi(c, ctrl, "ownpal", 0, 0) != 0,
            scale,
            pause_move: (self.pi(c, ctrl, "pausemovetime", 0, 0), self.pi(c, ctrl, "supermovetime", 0, 0)),
        };
        if self.spawn_helper(c, spec).is_none() {
            // Con `format!` y no con un texto fijo: copiar un literal de este
            // tamaño genera cargas no alineadas que el legalizador de PS2Rust
            // no sabe emparejar.
            self.warn(format!("demasiados helpers a la vez (más de {}): no se crean más", crate::ents::MAX_HELPERS));
        }
    }

    /// Suma (o resta) vida respetando el K.O.
    pub fn life_add(&mut self, c: usize, amount: i32, kill: bool) {
        let training = self.training;
        let ch = &mut self.chars[c];
        let max = ch.life_max;
        ch.life = (ch.life + amount).min(max);
        if ch.life <= 0 {
            ch.life = if kill && ch.assert & AS_NOKO == 0 && !training { 0 } else { 1 };
        }
    }

    fn make_explod(&mut self, c: usize, ctrl: &Ctrl) {
        let Some((_, vals)) = self.prefixed(c, ctrl.raw("anim"), 'f') else { return };
        let fight = ctrl.raw("anim").is_some_and(|r| r.trim().to_ascii_lowercase().starts_with('f'));
        let no = vals.first().map_or(0, |v| v.i());
        let (anim, sff) = if fight {
            (self.fight.fx_anims.get(&no).cloned(), self.fight.fx_sff)
        } else {
            (self.chars[c].def.anims.get(&no).cloned(), self.chars[c].def.sff)
        };
        if anim.is_none() {
            return;
        }
        let (x, y) = (self.pf(c, ctrl, "pos", 0, 0.0), self.pf(c, ctrl, "pos", 1, 0.0));
        let postype = ctrl.raw("postype").unwrap_or("p1").trim().to_ascii_lowercase();
        let ch = &self.chars[c];
        let o = &self.chars[self.enemy(c)];
        let face = ch.facing as f32;
        let w = SCREEN_W as f32;
        let (pos, screen, base_facing, bind_to) = match postype.as_str() {
            "p2" => ([o.pos[0] + x * o.facing as f32, o.pos[1] + y], false, o.facing, Some(self.enemy(c))),
            "front" => ([if ch.facing > 0 { w + x } else { -x }, y], true, ch.facing, None),
            "back" => ([if ch.facing > 0 { x } else { w - x }, y], true, ch.facing, None),
            "left" => ([x, y], true, 1, None),
            "right" => ([w + x, y], true, 1, None),
            "none" => ([x, y], false, 1, None),
            _ => ([ch.pos[0] + x * face, ch.pos[1] + y], false, ch.facing, Some(c)),
        };
        let bindtime = self.pi(c, ctrl, "bindtime", 0, 0);
        let vel_key = if ctrl.param("velocity").is_some() { "velocity" } else { "vel" };
        let trans = ctrl.raw("trans").map(|t| match t.trim().to_ascii_lowercase().as_str() {
            "add" => Trans::ADD,
            "add1" => Trans::ADD1,
            "sub" => Trans::Sub,
            "addalpha" => Trans::Add { src: self.pi(c, ctrl, "alpha", 0, 256) as u16, dst: self.pi(c, ctrl, "alpha", 1, 0) as u16 },
            other => parse_trans(other),
        });
        let mut st = AnimState::default();
        st.set(anim, no);
        let e = Explod {
            owner: c,
            id: self.pi(c, ctrl, "id", 0, -1),
            anim: st,
            sff,
            own: !fight,
            pos,
            screen,
            vel: [self.pf(c, ctrl, vel_key, 0, 0.0), self.pf(c, ctrl, vel_key, 1, 0.0)],
            accel: [self.pf(c, ctrl, "accel", 0, 0.0), self.pf(c, ctrl, "accel", 1, 0.0)],
            facing: base_facing * if self.pi(c, ctrl, "facing", 0, 1) < 0 { -1 } else { 1 },
            vfacing: if self.pi(c, ctrl, "vfacing", 0, 1) < 0 { -1 } else { 1 },
            bind: match bind_to {
                Some(t) if bindtime != 0 => Some((t, [x, y], bindtime)),
                _ => None,
            },
            removetime: self.pi(c, ctrl, "removetime", 0, -2),
            scale: (self.pf(c, ctrl, "scale", 0, 1.0), self.pf(c, ctrl, "scale", 1, 1.0)),
            sprpriority: self.pi(c, ctrl, "sprpriority", 0, 0),
            ontop: self.pi(c, ctrl, "ontop", 0, 0) != 0,
            removeongethit: self.pi(c, ctrl, "removeongethit", 0, 0) != 0,
            trans,
        };
        self.explods.push(e);
    }

    /// Ejecuta un controlador. Devuelve `true` si cambió el estado.
    pub fn exec(&mut self, c: usize, ctrl: &Ctrl) -> bool {
        let x = self.popt(c, ctrl, "x", 0);
        let y = self.popt(c, ctrl, "y", 0);
        let value = self.popt(c, ctrl, "value", 0);
        match ctrl.kind {
            Kind::Null | Kind::Ignored | Kind::AllPalFx | Kind::BgPalFx | Kind::EnvColor | Kind::Offset | Kind::Trans => {}
            Kind::ChangeState | Kind::SelfState => {
                let Some(v) = value else { return false };
                let owner = if ctrl.kind == Kind::SelfState { Some(c) } else { None };
                let before = (self.chars[c].stateno, self.chars[c].time);
                self.change_state(c, v.i(), owner);
                if let Some(k) = self.popt(c, ctrl, "ctrl", 0) {
                    self.chars[c].ctrl = k.b();
                }
                if let Some(a) = self.popt(c, ctrl, "anim", 0) {
                    let from = self.chars[c].state_owner;
                    self.set_anim(c, a.i(), from);
                }
                // Si el estado no existía no hubo cambio.
                return (self.chars[c].stateno, self.chars[c].time) != before || self.chars[c].stateno == v.i();
            }
            Kind::ChangeAnim | Kind::ChangeAnim2 => {
                if let Some(v) = value {
                    let from = if ctrl.kind == Kind::ChangeAnim2 { self.chars[c].state_owner } else { c };
                    self.set_anim(c, v.i(), from);
                    if let Some(e) = self.popt(c, ctrl, "elem", 0) {
                        self.chars[c].anim.set_elem(e.i());
                    }
                }
            }
            Kind::VelSet => {
                let ch = &mut self.chars[c];
                if let Some(v) = x {
                    ch.vel[0] = v.f();
                }
                if let Some(v) = y {
                    ch.vel[1] = v.f();
                }
            }
            Kind::VelAdd => {
                let ch = &mut self.chars[c];
                ch.vel[0] += x.map_or(0.0, V::f);
                ch.vel[1] += y.map_or(0.0, V::f);
            }
            Kind::VelMul => {
                let ch = &mut self.chars[c];
                ch.vel[0] *= x.map_or(1.0, V::f);
                ch.vel[1] *= y.map_or(1.0, V::f);
            }
            Kind::PosSet => {
                let cam = self.cam[0];
                let ch = &mut self.chars[c];
                if let Some(v) = x {
                    ch.pos[0] = cam + v.f();
                }
                if let Some(v) = y {
                    ch.pos[1] = v.f();
                }
            }
            Kind::PosAdd => {
                let ch = &mut self.chars[c];
                ch.pos[0] += x.map_or(0.0, V::f) * ch.facing as f32;
                ch.pos[1] += y.map_or(0.0, V::f);
            }
            Kind::CtrlSet => {
                if let Some(v) = value {
                    self.chars[c].ctrl = v.b();
                }
            }
            Kind::StateTypeSet => {
                let letter = |k: &str| ctrl.raw(k).and_then(|s| s.trim().bytes().next()).map(|b| b.to_ascii_lowercase());
                let ch = &mut self.chars[c];
                if let Some(l) = letter("statetype").filter(|&l| l != b'u') {
                    ch.stype = l;
                }
                if let Some(l) = letter("movetype").filter(|&l| l != b'u') {
                    ch.movetype = l;
                }
                if let Some(l) = letter("physics").filter(|&l| l != b'u') {
                    ch.physics = l;
                }
            }
            Kind::VarSet | Kind::VarAdd | Kind::ParentVarSet | Kind::ParentVarAdd => {
                let add = matches!(ctrl.kind, Kind::VarAdd | Kind::ParentVarAdd);
                // Los valores se calculan aquí; se guardan en mí o en mi creador.
                let dst = if matches!(ctrl.kind, Kind::ParentVarSet | Kind::ParentVarAdd) {
                    let parent = self.chars[c].parent;
                    if !self.chars[c].helper || parent == NOBODY {
                        return false;
                    }
                    parent
                } else {
                    c
                };
                let mut target: Option<(Trig, i32)> = None;
                for (key, p) in &ctrl.params {
                    let Some((kind, idx)) = var_key(key) else { continue };
                    let Some(e) = p.exprs.first().and_then(|e| e.as_ref()) else { continue };
                    let v = self.eval(c, e);
                    match idx {
                        // `var(3) = expr`
                        Some(i) => self.assign(dst, kind, i, v, add),
                        // `v = 3` junto con `value = expr`
                        None => target = Some((kind, v.i())),
                    }
                }
                if let (Some((kind, i)), Some(v)) = (target, value) {
                    self.assign(dst, kind, i, v, add);
                }
            }
            Kind::VarRangeSet => {
                let first = self.pi(c, ctrl, "first", 0, 0);
                if let Some(v) = value {
                    for i in first..=self.pi(c, ctrl, "last", 0, 59) {
                        self.chars[c].set_var(Trig::Var, i, v);
                    }
                }
                if let Some(v) = self.popt(c, ctrl, "fvalue", 0) {
                    for i in first..=self.pi(c, ctrl, "last", 0, 39) {
                        self.chars[c].set_var(Trig::FVar, i, v);
                    }
                }
            }
            Kind::PlaySnd => {
                if let Some((fight, v)) = self.prefixed(c, ctrl.raw("value"), 'f') {
                    let bank = if fight { self.fight.common_snd } else { self.chars[c].def.snd };
                    let (g, n) = (v.first().map_or(0, |x| x.i()), v.get(1).map_or(0, |x| x.i()));
                    let chan = self.pi(c, ctrl, "channel", 0, -1);
                    let vol = self.pf(c, ctrl, "volumescale", 0, 100.0) / 100.0;
                    if let Some(idx) = self.snds[bank as usize].find(g, n) {
                        self.sounds.push(SoundCmd {
                            bank,
                            idx,
                            chan: if chan >= 0 { c as i32 * 100 + chan } else { -1 },
                            vol,
                            pan: 0.0,
                            freq: self.pf(c, ctrl, "freqmul", 0, 1.0),
                            stop: false,
                        });
                    }
                }
            }
            Kind::StopSnd => {
                let chan = self.pi(c, ctrl, "channel", 0, 0);
                self.sounds.push(SoundCmd { bank: 0, idx: 0, chan: c as i32 * 100 + chan, vol: 0.0, pan: 0.0, freq: 1.0, stop: true });
            }
            Kind::HitDef => {
                let h = self.build_hitdef(c, ctrl);
                let ch = &mut self.chars[c];
                ch.hitdef = Some(Rc::new(h));
                ch.hitdef_active = true;
                ch.move_contact = 0;
                ch.move_hit = 0;
                ch.move_guarded = 0;
                ch.move_reversed = 0;
            }
            Kind::ReversalDef => {
                let attr = parse_attr(ctrl.raw("reversal.attr").unwrap_or(""));
                let spark = match self.prefixed(c, ctrl.raw("sparkno"), 's') {
                    Some((own, v)) => (own, v.first().map_or(-1, |x| x.i())),
                    None => (false, self.chars[c].def.sparkno),
                };
                let r = Reversal {
                    attr,
                    pausetime: (self.pi(c, ctrl, "pausetime", 0, 0), self.pi(c, ctrl, "pausetime", 1, 0)),
                    sparkno: spark,
                    sparkxy: (self.pf(c, ctrl, "sparkxy", 0, 0.0), self.pf(c, ctrl, "sparkxy", 1, 0.0)),
                    hitsound: self.sound_spec(c, ctrl.raw("hitsound")),
                    p1stateno: self.popt(c, ctrl, "p1stateno", 0).map(V::i),
                    p2stateno: self.popt(c, ctrl, "p2stateno", 0).map(V::i),
                };
                self.chars[c].reversal = if attr == 0 { None } else { Some(r) };
            }
            Kind::HitOverride => {
                let slot = self.pi(c, ctrl, "slot", 0, 0).clamp(0, 7) as usize;
                let o = HitOverride {
                    attr: parse_attr(ctrl.raw("attr").unwrap_or("")),
                    stateno: self.pi(c, ctrl, "stateno", 0, -1),
                    time: self.pi(c, ctrl, "time", 0, 1),
                };
                self.chars[c].hitoverride[slot] = o;
            }
            Kind::NotHitBy | Kind::HitBy => {
                let time = self.pi(c, ctrl, "time", 0, 1);
                for (slot, key) in ["value", "value2"].iter().enumerate() {
                    if let Some(raw) = ctrl.raw(key) {
                        self.chars[c].nothitby[slot] = (parse_attr(raw), time, ctrl.kind == Kind::HitBy);
                    }
                }
            }
            Kind::Width => {
                let pair = |k: &str| ctrl.param(k).map(|_| (self.pf(c, ctrl, k, 0, 0.0), self.pf(c, ctrl, k, 1, 0.0)));
                let (edge, player, both) = (pair("edge"), pair("player"), pair("value"));
                let ch = &mut self.chars[c];
                if let Some(v) = both.or(edge) {
                    ch.width_edge = v;
                }
                if let Some(v) = both.or(player) {
                    ch.width_player = v;
                }
            }
            Kind::Turn => self.chars[c].facing = -self.chars[c].facing,
            Kind::Explod => self.make_explod(c, ctrl),
            Kind::RemoveExplod => {
                let id = self.pi(c, ctrl, "id", 0, -1);
                self.explods.retain(|e| e.owner != c || (id >= 0 && e.id != id));
            }
            Kind::MakeDust | Kind::GameMakeAnim => {
                let dust = ctrl.kind == Kind::MakeDust;
                let no = if dust { 120 } else { value.map_or(0, V::i) };
                let ch = &self.chars[c];
                let pos = [ch.pos[0] + self.pf(c, ctrl, "pos", 0, 0.0) * ch.facing as f32, ch.pos[1] + self.pf(c, ctrl, "pos", 1, 0.0)];
                let spacing = self.pi(c, ctrl, "spacing", 0, 3).max(1);
                if !dust || ch.time % spacing == 0 {
                    let facing = ch.facing;
                    self.spawn_fx(c, false, no, pos, facing, if self.pi(c, ctrl, "under", 0, 0) != 0 { 0 } else { 3 });
                }
            }
            Kind::AfterImage => {
                let time = self.pi(c, ctrl, "time", 0, 1);
                let trans = ctrl.raw("trans").map_or(Trans::None, |t| match t.trim().to_ascii_lowercase().as_str() {
                    "add" => Trans::ADD,
                    "add1" => Trans::ADD1,
                    "sub" => Trans::Sub,
                    other => parse_trans(other),
                });
                let three = |k: &str, d: [i32; 3]| [self.pi(c, ctrl, k, 0, d[0]), self.pi(c, ctrl, k, 1, d[1]), self.pi(c, ctrl, k, 2, d[2])];
                let bright = three("palbright", [30, 30, 30]);
                let contrast = three("palcontrast", [120, 120, 220]);
                let add = three("paladd", [10, 10, 25]);
                let mul = [self.pf(c, ctrl, "palmul", 0, 0.65), self.pf(c, ctrl, "palmul", 1, 0.65), self.pf(c, ctrl, "palmul", 2, 0.75)];
                let length = self.pi(c, ctrl, "length", 0, 20);
                let timegap = self.pi(c, ctrl, "timegap", 0, 1);
                let framegap = self.pi(c, ctrl, "framegap", 0, 4);
                let a = &mut self.chars[c].afterimage;
                a.clear();
                a.time = time;
                a.length = length;
                a.timegap = timegap;
                a.framegap = framegap;
                a.trans = trans;
                a.bright = bright;
                a.contrast = contrast;
                a.add = add;
                a.mul = mul;
            }
            Kind::AfterImageTime => {
                let t = self.popt(c, ctrl, "time", 0).or(value).map_or(0, V::i);
                self.chars[c].afterimage.time = t;
            }
            Kind::PalFx => {
                if let Some(p) = self.palfx_from(c, ctrl, "") {
                    self.chars[c].palfx = p;
                }
            }
            Kind::RemapPal => {
                let dest = (self.pi(c, ctrl, "dest", 0, -1), self.pi(c, ctrl, "dest", 1, 0));
                if dest.0 >= 0 {
                    let sff = &self.sffs[self.chars[c].def.sff as usize];
                    if let Some(&p) = sff.pal_index.get(&(dest.0 as u16, dest.1 as u16)) {
                        self.chars[c].pal = p;
                    }
                }
            }
            Kind::EnvShake => {
                let freq = self.pf(c, ctrl, "freq", 0, 60.0);
                let time = self.pi(c, ctrl, "time", 0, 1);
                self.shake = Shake {
                    time,
                    total: time,
                    freq,
                    ampl: self.pf(c, ctrl, "ampl", 0, -4.0),
                    phase: self.pf(c, ctrl, "phase", 0, if freq >= 90.0 { 90.0 } else { 0.0 }),
                };
            }
            Kind::FallEnvShake => {
                let s = self.chars[c].ghv.fall_shake;
                if s.0 > 0 {
                    self.shake = Shake { time: s.0, total: s.0, freq: s.1, ampl: s.2 as f32, phase: s.3 };
                    self.chars[c].ghv.fall_shake.0 = 0;
                }
            }
            Kind::Pause | Kind::SuperPause => {
                let sup = ctrl.kind == Kind::SuperPause;
                let time = self.pi(c, ctrl, "time", 0, if sup { 30 } else { 0 });
                self.pause = PauseState {
                    time,
                    movetime: self.pi(c, ctrl, "movetime", 0, 0),
                    by: c,
                    darken: sup && self.pi(c, ctrl, "darken", 0, 1) != 0,
                    fresh: true,
                };
                if sup {
                    let ch = &self.chars[c];
                    let pos = [ch.pos[0] + self.pf(c, ctrl, "pos", 0, 0.0) * ch.facing as f32, ch.pos[1] + self.pf(c, ctrl, "pos", 1, 0.0)];
                    let facing = ch.facing;
                    match self.prefixed(c, ctrl.raw("anim"), 's') {
                        Some((own, v)) => {
                            let no = v.first().map_or(-1, |x| x.i());
                            self.spawn_fx(c, own, no, pos, facing, 5);
                        }
                        None => self.spawn_fx(c, false, 100, pos, facing, 5),
                    }
                    if let Some((own, g, n)) = self.sound_spec(c, ctrl.raw("sound")) {
                        let bank = if own { self.chars[c].def.snd } else { self.fight.common_snd };
                        self.play(bank, g, n, -1, 1.0);
                    }
                    let add = self.pi(c, ctrl, "poweradd", 0, 0);
                    self.add_power(c, add);
                }
            }
            Kind::AssertSpecial => {
                for k in ["flag", "flag2", "flag3"] {
                    if let Some(raw) = ctrl.raw(k) {
                        self.chars[c].assert |= assert_flag(raw);
                    }
                }
            }
            Kind::ScreenBound => {
                let cam = (self.pi(c, ctrl, "movecamera", 0, 0) != 0, self.pi(c, ctrl, "movecamera", 1, 0) != 0);
                let ch = &mut self.chars[c];
                ch.screenbound = value.is_some_and(V::b);
                ch.movecamera = cam;
            }
            Kind::PosFreeze => {
                let all = value.map_or(true, V::b);
                let ch = &mut self.chars[c];
                ch.posfreeze = if x.is_some() || y.is_some() { (x.is_some_and(V::b), y.is_some_and(V::b)) } else { (all, all) };
            }
            Kind::PlayerPush => self.chars[c].playerpush = value.map_or(true, V::b),
            Kind::SprPriority => self.chars[c].sprpriority = value.map_or(0, V::i),
            Kind::Gravity => {
                let ch = &mut self.chars[c];
                ch.vel[1] += ch.def.yaccel;
            }
            Kind::HitVelSet => {
                let ch = &mut self.chars[c];
                if x.is_some_and(V::b) {
                    ch.vel[0] = ch.ghv.xvel;
                }
                if y.is_some_and(V::b) {
                    ch.vel[1] = ch.ghv.yvel;
                }
            }
            Kind::HitFallDamage => {
                let (dmg, kill) = (self.chars[c].ghv.fall_damage, self.chars[c].ghv.fall_kill);
                self.life_add(c, -dmg, kill);
                self.chars[c].ghv.fall_damage = 0;
            }
            Kind::HitFallVel => {
                let ch = &mut self.chars[c];
                if ch.ghv.fall {
                    if let Some(vx) = ch.ghv.fall_xvel {
                        ch.vel[0] = vx;
                    }
                    ch.vel[1] = ch.ghv.fall_yvel;
                }
            }
            Kind::HitFallSet => {
                let (xv, yv) = (self.popt(c, ctrl, "xvel", 0), self.popt(c, ctrl, "yvel", 0));
                let g = &mut self.chars[c].ghv;
                match value.map_or(-1, V::i) {
                    0 => g.fall = false,
                    1 => g.fall = true,
                    _ => {}
                }
                if let Some(v) = xv {
                    g.fall_xvel = Some(v.f());
                }
                if let Some(v) = yv {
                    g.fall_yvel = v.f();
                }
            }
            Kind::DefenceMulSet => self.chars[c].defence_mul = value.map_or(1.0, V::f),
            Kind::AttackMulSet => self.chars[c].attack_mul = value.map_or(1.0, V::f),
            Kind::AttackDist => {
                if let (Some(v), Some(h)) = (value, self.chars[c].hitdef.as_mut()) {
                    Rc::make_mut(h).guard_dist = Some(v.f());
                }
            }
            Kind::LifeAdd => {
                let kill = self.pi(c, ctrl, "kill", 0, 1) != 0;
                self.life_add(c, value.map_or(0, V::i), kill);
            }
            Kind::LifeSet => {
                if let Some(v) = value {
                    let max = self.chars[c].life_max;
                    self.chars[c].life = v.i().clamp(0, max);
                }
            }
            Kind::PowerAdd => self.add_power(c, value.map_or(0, V::i)),
            Kind::PowerSet => {
                if let Some(v) = value {
                    let root = self.chars[c].root;
                    self.chars[root].power = v.i().clamp(0, 3000);
                }
            }
            Kind::TargetBind => {
                let b = Bind { to: c, pos: (self.pf(c, ctrl, "pos", 0, 0.0), self.pf(c, ctrl, "pos", 1, 0.0)), time: self.pi(c, ctrl, "time", 0, 1), facing: 0 };
                for t in self.chars[c].targets.clone() {
                    self.chars[t].bind = Some(b);
                }
            }
            Kind::BindToTarget => {
                if let Some(&t) = self.chars[c].targets.first() {
                    let pos = (self.pf(c, ctrl, "pos", 0, 0.0), self.pf(c, ctrl, "pos", 1, 0.0));
                    self.chars[c].bind = Some(Bind { to: t, pos, time: self.pi(c, ctrl, "time", 0, 1), facing: 0 });
                }
            }
            Kind::TargetState => {
                if let Some(v) = value {
                    for t in self.chars[c].targets.clone() {
                        self.change_state(t, v.i(), Some(c));
                    }
                }
            }
            Kind::TargetLifeAdd => {
                let kill = self.pi(c, ctrl, "kill", 0, 1) != 0;
                let absolute = self.pi(c, ctrl, "absolute", 0, 0) != 0;
                for t in self.chars[c].targets.clone() {
                    let mut v = value.map_or(0, V::i);
                    if !absolute && v < 0 {
                        let k = self.chars[c].def.attack / self.chars[t].def.defence.max(1.0) * self.chars[c].attack_mul / self.chars[t].defence_mul.max(0.01);
                        v = (v as f32 * k - 0.001) as i32;
                    }
                    self.life_add(t, v, kill);
                    if self.chars[t].life == 0 && self.round.state == 2 {
                        self.round.ko_by_hit = true;
                        self.chars[t].ghv.fall = true;
                    }
                }
            }
            Kind::TargetPowerAdd => {
                for t in self.chars[c].targets.clone() {
                    self.add_power(t, value.map_or(0, V::i));
                }
            }
            Kind::TargetFacing => {
                let f = self.chars[c].facing * if value.map_or(1, V::i) < 0 { -1 } else { 1 };
                for t in self.chars[c].targets.clone() {
                    self.chars[t].facing = f;
                }
            }
            Kind::TargetVelSet | Kind::TargetVelAdd => {
                let add = ctrl.kind == Kind::TargetVelAdd;
                for t in self.chars[c].targets.clone() {
                    let tc = &mut self.chars[t];
                    if let Some(v) = x {
                        tc.vel[0] = if add { tc.vel[0] + v.f() } else { v.f() };
                    }
                    if let Some(v) = y {
                        tc.vel[1] = if add { tc.vel[1] + v.f() } else { v.f() };
                    }
                }
            }
            Kind::TargetDrop => self.chars[c].targets.clear(),
            Kind::Helper => self.make_helper(c, ctrl),
            Kind::Projectile => self.make_projectile(c, ctrl),
            Kind::DestroySelf => {
                // Sólo un helper puede retirarse; a un jugador no le hace nada.
                if self.chars[c].helper {
                    self.destroy_helper(c);
                    return true;
                }
            }
            Kind::BindToParent | Kind::BindToRoot => {
                let ch = &self.chars[c];
                let to = if ctrl.kind == Kind::BindToParent { ch.parent } else { ch.root };
                if ch.helper && to != NOBODY && to != c {
                    let pos = (self.pf(c, ctrl, "pos", 0, 0.0), self.pf(c, ctrl, "pos", 1, 0.0));
                    let facing = self.pi(c, ctrl, "facing", 0, 0).signum();
                    self.chars[c].bind = Some(Bind { to, pos, time: self.pi(c, ctrl, "time", 0, 1), facing });
                }
            }
            Kind::MoveHitReset => {
                let ch = &mut self.chars[c];
                ch.move_contact = 0;
                ch.move_hit = 0;
                ch.move_guarded = 0;
            }
        }
        false
    }

    fn assign(&mut self, c: usize, kind: Trig, i: i32, v: V, add: bool) {
        let ch = &self.chars[c];
        let v = if add {
            match (ch.get_var(kind, i), v) {
                (V::I(a), b) => V::I(a.wrapping_add(b.i())),
                (V::F(a), b) => V::F(a + b.f()),
            }
        } else {
            v
        };
        ch.set_var(kind, i, v);
    }
}
