//! Evaluación de expresiones contra el estado de la pelea.

use crate::expr::{Expr, Op, Redir, Trig, V, AT_CLASS, AT_STATE};
use crate::ents::{PROJ_CANCEL, PROJ_GUARDED, PROJ_HIT};
use crate::game::Game;
use crate::gfx::{SCREEN_H, SCREEN_W};

fn arith(op: Op, a: V, b: V) -> V {
    let t = |x: bool| V::I(x as i32);
    if let (V::I(x), V::I(y)) = (a, b) {
        return match op {
            Op::Add => V::I(x.wrapping_add(y)),
            Op::Sub => V::I(x.wrapping_sub(y)),
            Op::Mul => V::I(x.wrapping_mul(y)),
            Op::Div => V::I(if y == 0 { 0 } else { x.wrapping_div(y) }),
            Op::Mod => V::I(if y == 0 { 0 } else { x.wrapping_rem(y) }),
            Op::Pow => {
                if y >= 0 {
                    V::I((0..y.min(64)).fold(1i32, |acc, _| acc.wrapping_mul(x)))
                } else {
                    V::F(powf(x as f32, y as f32))
                }
            }
            Op::Eq => t(x == y),
            Op::Ne => t(x != y),
            Op::Lt => t(x < y),
            Op::Le => t(x <= y),
            Op::Gt => t(x > y),
            Op::Ge => t(x >= y),
            Op::BitAnd => V::I(x & y),
            Op::BitOr => V::I(x | y),
            Op::BitXor => V::I(x ^ y),
            Op::And => t(x != 0 && y != 0),
            Op::Or => t(x != 0 || y != 0),
            Op::Xor => t((x != 0) != (y != 0)),
        };
    }
    let (x, y) = (a.f(), b.f());
    match op {
        Op::Add => V::F(x + y),
        Op::Sub => V::F(x - y),
        Op::Mul => V::F(x * y),
        Op::Div => V::F(if y == 0.0 { 0.0 } else { x / y }),
        Op::Mod => V::I(if b.i() == 0 { 0 } else { a.i().wrapping_rem(b.i()) }),
        Op::Pow => V::F(powf(x, y)),
        Op::Eq => t(x == y),
        Op::Ne => t(x != y),
        Op::Lt => t(x < y),
        Op::Le => t(x <= y),
        Op::Gt => t(x > y),
        Op::Ge => t(x >= y),
        Op::BitAnd => V::I(a.i() & b.i()),
        Op::BitOr => V::I(a.i() | b.i()),
        Op::BitXor => V::I(a.i() ^ b.i()),
        Op::And => t(a.b() && b.b()),
        Op::Or => t(a.b() || b.b()),
        Op::Xor => t(a.b() != b.b()),
    }
}

// Matemática en `f32` con series cortas: la FPU de la PS2 no tiene estas
// funciones y las de `f64` por software son lentas.

pub fn floorf(x: f32) -> f32 {
    let t = x as i32 as f32;
    if t > x {
        t - 1.0
    } else {
        t
    }
}

pub fn sinf(x: f32) -> f32 {
    const TAU: f32 = 6.283_185_5;
    const PI: f32 = 3.141_592_7;
    let mut a = x - floorf(x / TAU) * TAU;
    if a > PI {
        a -= TAU;
    }
    // Reduce a [-π/2, π/2].
    if a > PI / 2.0 {
        a = PI - a;
    } else if a < -PI / 2.0 {
        a = -PI - a;
    }
    let a2 = a * a;
    a * (1.0 - a2 / 6.0 * (1.0 - a2 / 20.0 * (1.0 - a2 / 42.0 * (1.0 - a2 / 72.0))))
}

pub fn cosf(x: f32) -> f32 {
    sinf(x + 1.570_796_4)
}

fn expf(x: f32) -> f32 {
    if x > 80.0 {
        return f32::MAX;
    }
    if x < -80.0 {
        return 0.0;
    }
    // e^x = 2^k * e^r, con r pequeño.
    let k = floorf(x / 0.693_147_2 + 0.5);
    let r = x - k * 0.693_147_2;
    let mut term = 1.0;
    let mut sum = 1.0;
    for i in 1..10 {
        term *= r / i as f32;
        sum += term;
    }
    let mut p = 1.0f32;
    for _ in 0..(k.abs() as i32) {
        p *= 2.0;
    }
    if k < 0.0 {
        sum / p
    } else {
        sum * p
    }
}

fn lnf(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut v = x;
    let mut k = 0.0f32;
    while v > 1.5 {
        v /= 2.0;
        k += 1.0;
    }
    while v < 0.75 {
        v *= 2.0;
        k -= 1.0;
    }
    let y = (v - 1.0) / (v + 1.0);
    let y2 = y * y;
    let mut term = y;
    let mut sum = 0.0;
    for i in 0..8 {
        sum += term / (2 * i + 1) as f32;
        term *= y2;
    }
    2.0 * sum + k * 0.693_147_2
}

fn powf(x: f32, y: f32) -> f32 {
    if y == 0.0 {
        return 1.0;
    }
    if x == 0.0 {
        return 0.0;
    }
    if y == floorf(y) && y.abs() < 64.0 {
        let mut p = 1.0;
        for _ in 0..(y.abs() as i32) {
            p *= x;
        }
        return if y < 0.0 { 1.0 / p } else { p };
    }
    if x < 0.0 {
        return 0.0;
    }
    expf(y * lnf(x))
}

fn atanf(x: f32) -> f32 {
    if x.abs() > 1.0 {
        let s = if x > 0.0 { 1.570_796_4 } else { -1.570_796_4 };
        return s - atanf(1.0 / x);
    }
    // Reducción para que la serie converja rápido.
    if x.abs() > 0.4 {
        let c = if x > 0.0 { 1.0 } else { -1.0 };
        return c * 0.785_398_2 + atanf((x - c) / (1.0 + c * x));
    }
    let x2 = x * x;
    x * (1.0 - x2 * (1.0 / 3.0 - x2 * (1.0 / 5.0 - x2 * (1.0 / 7.0 - x2 / 9.0))))
}

pub fn sqrtf(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    let mut g = if x > 1.0 { x / 2.0 } else { 1.0 };
    for _ in 0..12 {
        g = 0.5 * (g + x / g);
    }
    g
}

impl Game {
    pub fn rand(&self) -> i32 {
        let mut s = self.rng.get();
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        self.rng.set(s);
        ((s >> 8) % 1000) as i32
    }

    /// El jugador rival de `c` (que puede ser un jugador o un helper).
    pub fn enemy(&self, c: usize) -> usize {
        1 - self.chars[c].team
    }

    /// El personaje al que apunta una redirección (`parent, …`), si existe.
    fn redirect(&self, c: usize, r: Redir, arg: Option<i32>) -> Option<usize> {
        let ch = &self.chars[c];
        let alive = |i: usize| self.chars.get(i).is_some_and(|x| x.active);
        match r {
            Redir::Enemy | Redir::EnemyNear => Some(self.enemy(c)),
            Redir::Root => Some(ch.root),
            // En un jugador, `parent` es él mismo.
            Redir::Parent => Some(ch.parent).filter(|&p| alive(p)),
            Redir::Target => ch.targets.first().copied(),
            Redir::Helper => self.helpers_of(c, arg.unwrap_or(0)).next(),
            Redir::PlayerId => self.chars.iter().position(|x| x.active && Some(x.uid) == arg),
            Redir::Partner => None,
        }
    }

    pub fn eval_bool(&self, c: usize, e: &Expr) -> bool {
        self.eval(c, e).b()
    }

    pub fn eval(&self, c: usize, e: &Expr) -> V {
        match e {
            Expr::Num(v) => *v,
            Expr::Not(a) => V::I(!self.eval(c, a).b() as i32),
            Expr::BitNot(a) => V::I(!self.eval(c, a).i()),
            Expr::Neg(a) => match self.eval(c, a) {
                V::I(i) => V::I(i.wrapping_neg()),
                V::F(f) => V::F(-f),
            },
            Expr::Bin(op, a, b) => {
                // Cortocircuito, como en MUGEN.
                match op {
                    Op::And if !self.eval(c, a).b() => return V::I(0),
                    Op::Or if self.eval(c, a).b() => return V::I(1),
                    _ => {}
                }
                arith(*op, self.eval(c, a), self.eval(c, b))
            }
            Expr::Range(v, lo, hi, lo_inc, hi_inc, neg) => {
                let (v, lo, hi) = (self.eval(c, v).f(), self.eval(c, lo).f(), self.eval(c, hi).f());
                let inside = (if *lo_inc { v >= lo } else { v > lo }) && (if *hi_inc { v <= hi } else { v < hi });
                V::I((inside != *neg) as i32)
            }
            Expr::Redir(r, arg, inner) => {
                let arg = arg.as_ref().map(|a| self.eval(c, a).i());
                self.redirect(c, *r, arg).map_or(V::I(0), |t| self.eval(t, inner))
            }
            Expr::Assign(kind, idx, val) => {
                let (i, v) = (self.eval(c, idx).i(), self.eval(c, val));
                self.chars[c].set_var(*kind, i, v);
                self.chars[c].get_var(*kind, i)
            }
            Expr::Command(name) => {
                let ch = &self.chars[c];
                V::I(ch.cmd.is_active(&ch.def.cmds, name) as i32)
            }
            Expr::Const(key) => {
                let v = self.chars[c].def.consts.get(key.as_str()).copied().unwrap_or(0.0);
                if key.starts_with("data.") && !key.ends_with("_up") || key.ends_with(".num") || key.ends_with(".time") {
                    V::I(v as i32)
                } else {
                    V::F(v)
                }
            }
            Expr::GetHitVar(key) => self.gethitvar(c, key),
            Expr::StateIs(who, what, letter) => {
                let ch = &self.chars[if *who == 1 { self.enemy(c) } else { c }];
                V::I((if *what == 1 { ch.movetype } else { ch.stype } == *letter) as i32)
            }
            Expr::HitDefAttr(mask, neg) => {
                let ch = &self.chars[c];
                let hit = ch.movetype == b'a'
                    && ch.hitdef.as_ref().is_some_and(|h| h.attr & mask & AT_STATE != 0 && h.attr & mask & AT_CLASS != 0);
                V::I((hit != *neg) as i32)
            }
            Expr::NameIs(which, text) => {
                let ch = &self.chars[if *which == 1 { self.enemy(c) } else { c }];
                let name = if *which == 2 { &ch.def.author } else { &ch.def.name };
                V::I((*which != 3 && name.to_ascii_lowercase() == *text) as i32)
            }
            Expr::T(t, args) => self.trig(c, *t, args),
        }
    }

    fn gethitvar(&self, c: usize, key: &str) -> V {
        let g = &self.chars[c].ghv;
        match key {
            "xveladd" | "yveladd" => V::F(0.0),
            "type" => V::I(g.htype),
            "animtype" => V::I(g.animtype),
            "airtype" => V::I(g.airtype),
            "groundtype" => V::I(g.groundtype),
            "damage" => V::I(g.damage),
            "hitcount" => V::I(g.hitcount),
            "fallcount" => V::I(g.fallcount),
            "hitshaketime" => V::I(g.hitshake.max(0)),
            "hittime" => V::I(g.hittime),
            "slidetime" => V::I(g.slidetime),
            "ctrltime" => V::I(g.ctrltime),
            "recovertime" => V::I(g.recovertime.max(0)),
            "xoff" | "yoff" => V::F(0.0),
            "xvel" => V::F(g.xvel),
            "yvel" => V::F(g.yvel),
            "yaccel" => V::F(g.yaccel),
            "hitid" | "chainid" => V::I(g.chainid),
            "guarded" => V::I(g.guarded as i32),
            "isbound" => V::I(self.chars[c].bind.is_some() as i32),
            "fall" => V::I(g.fall as i32),
            "fall.damage" => V::I(g.fall_damage),
            "fall.xvel" => V::F(g.fall_xvel.unwrap_or(0.0)),
            "fall.yvel" => V::F(g.fall_yvel),
            "fall.recover" => V::I(g.fall_recover as i32),
            "fall.time" => V::I(g.fall_time),
            "fall.recovertime" => V::I(g.fall_recovertime),
            "fall.kill" => V::I(g.fall_kill as i32),
            "fall.envshake.time" => V::I(g.fall_shake.0),
            "fall.envshake.freq" => V::F(g.fall_shake.1),
            "fall.envshake.ampl" => V::I(g.fall_shake.2),
            "fall.envshake.phase" => V::F(g.fall_shake.3),
            _ => V::I(0),
        }
    }

    fn trig(&self, c: usize, t: Trig, args: &[Expr]) -> V {
        let ch = &self.chars[c];
        let b = |x: bool| V::I(x as i32);
        let arg = |i: usize| args.get(i).map_or(V::I(0), |e| self.eval(c, e));
        let other = &self.chars[self.enemy(c)];
        let half = SCREEN_W as f32 / 2.0;
        match t {
            Trig::Time => V::I(ch.time),
            Trig::StateNo => V::I(ch.stateno),
            Trig::PrevStateNo => V::I(ch.prevstateno),
            Trig::Anim => V::I(ch.anim.no),
            Trig::AnimTime => V::I(ch.anim.anim_time()),
            Trig::AnimElemNo => V::I(ch.anim.elem_no()),
            Trig::AnimElemTime => V::I(ch.anim.elem_time_of(arg(0).i())),
            Trig::AnimExist | Trig::SelfAnimExist => b(ch.def.anims.contains_key(&arg(0).i())),
            Trig::Ctrl => b(ch.ctrl),
            Trig::Life => V::I(ch.life),
            Trig::LifeMax => V::I(ch.life_max),
            Trig::Power => V::I(self.chars[ch.root].power),
            Trig::PowerMax => V::I(3000),
            Trig::Alive => b(ch.life > 0),
            Trig::Facing => V::I(ch.facing),
            Trig::Random => V::I(self.rand()),
            Trig::RoundState => V::I(self.round.state),
            Trig::RoundNo => V::I(self.round.no),
            Trig::RoundsExisted => V::I(self.round.no - 1 + self.round.matches * 9),
            Trig::MatchOver => b(self.round.match_over),
            Trig::MoveContact => V::I(ch.move_contact),
            Trig::MoveHit => V::I(ch.move_hit),
            Trig::MoveGuarded => V::I(ch.move_guarded),
            Trig::MoveReversed => V::I(ch.move_reversed),
            Trig::HitShakeOver => b(ch.ghv.hitshake <= 0),
            Trig::HitOver => b(ch.ghv.hittime < 0),
            Trig::HitFall => b(ch.ghv.fall),
            Trig::CanRecover => b(ch.ghv.fall && ch.ghv.fall_recover && ch.ghv.fall_recovertime <= 0),
            Trig::HitCount => V::I(ch.hitcount),
            Trig::HitPauseTime => V::I(ch.hitpause),
            Trig::InGuardDist => b(self.in_guard_dist(c)),
            Trig::FrontEdgeDist | Trig::FrontEdgeBodyDist | Trig::BackEdgeDist | Trig::BackEdgeBodyDist => {
                let front = matches!(t, Trig::FrontEdgeDist | Trig::FrontEdgeBodyDist);
                let right = (ch.facing > 0) == front;
                let d = if right { self.cam[0] + half - ch.pos[0] } else { ch.pos[0] - (self.cam[0] - half) };
                let body = match t {
                    Trig::FrontEdgeBodyDist => ch.width(true),
                    Trig::BackEdgeBodyDist => ch.width(false),
                    _ => 0.0,
                };
                V::F(d - body)
            }
            Trig::GameTime => V::I(self.tick),
            Trig::PalNo => V::I(ch.palno),
            Trig::Win => b(self.round.winner == Some(ch.team)),
            Trig::Lose => b(self.round.winner == Some(1 - ch.team)),
            Trig::DrawGame => b(self.round.state >= 3 && self.round.decided && self.round.winner.is_none()),
            Trig::NumEnemy => V::I(1),
            Trig::NumPartner | Trig::Zero => V::I(0),
            // 0 si lo lleva una persona; si no, la dificultad elegida.
            Trig::AiLevel => V::I(if self.chars[ch.root].ai { self.ai_level } else { 0 }),
            // Sin identificador (o con 0 o menos) cuentan todos.
            Trig::NumHelper => V::I(self.helpers_of(c, arg(0).i()).count() as i32),
            Trig::IsHelper => b(ch.helper && (arg(0).i() <= 0 || ch.helper_id == arg(0).i())),
            Trig::NumProj => V::I(self.count_projs(c, -1)),
            Trig::NumProjId => V::I(if arg(0).i() < 0 { 0 } else { self.count_projs(c, arg(0).i()) }),
            Trig::PlayerIdExist => b(self.chars.iter().any(|x| x.active && x.uid == arg(0).i())),
            Trig::ProjContactTime | Trig::ProjHitTime | Trig::ProjGuardedTime | Trig::ProjCancelTime => {
                let what = match t {
                    Trig::ProjHitTime => PROJ_HIT,
                    Trig::ProjGuardedTime => PROJ_GUARDED,
                    Trig::ProjCancelTime => PROJ_CANCEL,
                    _ => 3,
                };
                V::I(self.proj_time(c, arg(0).i(), what))
            }
            Trig::ParentDistX | Trig::ParentDistY | Trig::RootDistX | Trig::RootDistY => {
                let r = if matches!(t, Trig::ParentDistX | Trig::ParentDistY) { Redir::Parent } else { Redir::Root };
                match self.redirect(c, r, None) {
                    Some(to) if matches!(t, Trig::ParentDistX | Trig::RootDistX) => V::F((self.chars[to].pos[0] - ch.pos[0]) * ch.facing as f32),
                    Some(to) => V::F(self.chars[to].pos[1] - ch.pos[1]),
                    None => V::F(0.0),
                }
            }
            Trig::NumExplod => {
                let id = if args.is_empty() { -1 } else { arg(0).i() };
                V::I(self.explods.iter().filter(|e| e.owner == c && (id < 0 || e.id == id)).count() as i32)
            }
            Trig::NumTarget => V::I(ch.targets.len() as i32),
            Trig::Id => V::I(ch.uid),
            Trig::Pi => V::F(3.141_592_7),
            Trig::E => V::F(2.718_281_7),
            Trig::TicksPerSecond => V::I(60),
            Trig::TeamSide => V::I(ch.team as i32 + 1),
            Trig::VelX => V::F(ch.vel[0]),
            Trig::VelY => V::F(ch.vel[1]),
            Trig::PosX => V::F(ch.pos[0] - self.cam[0]),
            Trig::PosY => V::F(ch.pos[1]),
            Trig::P2DistX => V::F((other.pos[0] - ch.pos[0]) * ch.facing as f32),
            Trig::P2DistY => V::F(other.pos[1] - ch.pos[1]),
            Trig::P2BodyDistX => V::F(self.body_dist(c)),
            Trig::P2BodyDistY => V::F(other.pos[1] - ch.pos[1]),
            Trig::ScreenPosX => V::F(ch.pos[0] - self.cam[0] + half),
            Trig::ScreenPosY => V::F(ch.pos[1] - self.cam[1] + self.stage.zoffset),
            Trig::HitVelX => V::F(ch.ghv.xvel),
            Trig::HitVelY => V::F(ch.ghv.yvel),
            Trig::CameraPosX => V::F(self.cam[0]),
            Trig::CameraPosY => V::F(self.cam[1]),
            Trig::P2Life => V::I(other.life),
            Trig::P2StateNo => V::I(other.stateno),
            Trig::Var | Trig::FVar | Trig::SysVar | Trig::SysFVar => ch.get_var(t, arg(0).i()),
            Trig::Abs => match arg(0) {
                V::I(i) => V::I(i.wrapping_abs()),
                V::F(f) => V::F(f.abs()),
            },
            Trig::Ceil => match arg(0) {
                V::I(i) => V::I(i),
                V::F(f) => V::I(-floorf(-f) as i32),
            },
            Trig::Floor => match arg(0) {
                V::I(i) => V::I(i),
                V::F(f) => V::I(floorf(f) as i32),
            },
            Trig::Exp => V::F(expf(arg(0).f())),
            Trig::Ln => V::F(lnf(arg(0).f())),
            Trig::Log => {
                let d = lnf(arg(0).f());
                V::F(if d == 0.0 { 0.0 } else { lnf(arg(1).f()) / d })
            }
            Trig::Sin => V::F(sinf(arg(0).f())),
            Trig::Cos => V::F(cosf(arg(0).f())),
            Trig::Tan => {
                let (s, co) = (sinf(arg(0).f()), cosf(arg(0).f()));
                V::F(if co == 0.0 { 0.0 } else { s / co })
            }
            Trig::Atan => V::F(atanf(arg(0).f())),
            Trig::Asin => {
                let x = arg(0).f().clamp(-1.0, 1.0);
                let d = sqrtf(1.0 - x * x);
                V::F(if d == 0.0 { 1.570_796_4 * x } else { atanf(x / d) })
            }
            Trig::Acos => {
                let x = arg(0).f().clamp(-1.0, 1.0);
                let d = sqrtf(1.0 - x * x);
                V::F(1.570_796_4 - if d == 0.0 { 1.570_796_4 * x } else { atanf(x / d) })
            }
            Trig::IfElse => {
                if arg(0).b() {
                    arg(1)
                } else {
                    arg(2)
                }
            }
            Trig::Const720p => V::F(arg(0).f() * 0.25),
            Trig::Const240p => arg(0),
            Trig::Const480p => V::F(arg(0).f() * 0.5),
            Trig::GameWidth => V::I(SCREEN_W),
            Trig::GameHeight => V::I(SCREEN_H),
        }
    }

    /// Distancia en X entre los cuerpos de `c` y su rival, vista desde `c`.
    pub fn body_dist(&self, c: usize) -> f32 {
        let ch = &self.chars[c];
        let other = &self.chars[self.enemy(c)];
        let d = (other.pos[0] - ch.pos[0]) * ch.facing as f32;
        // Ancho del rival por el lado que nos queda cerca.
        let toward = (other.facing != ch.facing) == (d >= 0.0);
        d - ch.width(true) - other.width(toward)
    }

    /// ¿Hay un ataque del otro bando (de un personaje o de un proyectil) lo
    /// bastante cerca como para ponerse en guardia?
    pub fn in_guard_dist(&self, c: usize) -> bool {
        let ch = &self.chars[c];
        let near = self.chars.iter().any(|other| {
            if !other.active || other.team == ch.team || other.movetype != b'a' {
                return false;
            }
            let dist = other.hitdef.as_ref().and_then(|h| h.guard_dist).unwrap_or(other.def.attack_dist);
            let d = (ch.pos[0] - other.pos[0]) * other.facing as f32;
            d > -ch.width(true) && d - other.width(true) - ch.width(true) < dist
        });
        near || self.proj_in_guard_dist(c)
    }
}
