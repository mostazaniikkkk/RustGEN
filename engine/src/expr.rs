//! Expresiones de los archivos CNS/CMD: análisis a un árbol que después
//! evalúa `eval.rs` contra el estado de la pelea.

use crate::ini;

/// Valor de una expresión: MUGEN distingue enteros de reales.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum V {
    I(i32),
    F(f32),
}

impl V {
    pub fn f(self) -> f32 {
        match self {
            V::I(i) => i as f32,
            V::F(f) => f,
        }
    }

    pub fn i(self) -> i32 {
        match self {
            V::I(i) => i,
            V::F(f) => f as i32,
        }
    }

    pub fn b(self) -> bool {
        match self {
            V::I(i) => i != 0,
            V::F(f) => f != 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Or,
    Xor,
    And,
    BitOr,
    BitXor,
    BitAnd,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Redir {
    Enemy,
    EnemyNear,
    Root,
    Parent,
    Target,
    Partner,
    Helper,
    PlayerId,
}

/// Triggers sin sintaxis especial. Los argumentos van en `Expr::T`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trig {
    Time,
    StateNo,
    PrevStateNo,
    Anim,
    AnimTime,
    AnimElemNo,
    AnimElemTime,
    AnimExist,
    SelfAnimExist,
    Ctrl,
    Life,
    LifeMax,
    Power,
    PowerMax,
    Alive,
    Facing,
    Random,
    RoundState,
    RoundNo,
    RoundsExisted,
    MatchOver,
    MoveContact,
    MoveHit,
    MoveGuarded,
    MoveReversed,
    HitShakeOver,
    HitOver,
    HitFall,
    CanRecover,
    HitCount,
    HitPauseTime,
    InGuardDist,
    FrontEdgeDist,
    FrontEdgeBodyDist,
    BackEdgeDist,
    BackEdgeBodyDist,
    GameTime,
    PalNo,
    Win,
    Lose,
    DrawGame,
    NumEnemy,
    NumPartner,
    NumHelper,
    NumExplod,
    NumProj,
    NumTarget,
    IsHelper,
    Id,
    Pi,
    E,
    TicksPerSecond,
    AiLevel,
    TeamSide,
    VelX,
    VelY,
    PosX,
    PosY,
    P2DistX,
    P2DistY,
    P2BodyDistX,
    P2BodyDistY,
    ScreenPosX,
    ScreenPosY,
    HitVelX,
    HitVelY,
    CameraPosX,
    CameraPosY,
    P2Life,
    P2StateNo,
    Var,
    FVar,
    SysVar,
    SysFVar,
    Abs,
    Ceil,
    Floor,
    Exp,
    Ln,
    Log,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    IfElse,
    Const720p,
    Const240p,
    Const480p,
    GameWidth,
    GameHeight,
    NumProjId,
    PlayerIdExist,
    ParentDistX,
    ParentDistY,
    RootDistX,
    RootDistY,
    /// Ticks desde que un proyectil golpeó o fue bloqueado, golpeó, fue
    /// bloqueado o fue anulado; -1 si no ha pasado.
    ProjContactTime,
    ProjHitTime,
    ProjGuardedTime,
    ProjCancelTime,
    Zero,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Num(V),
    Not(Box<Expr>),
    BitNot(Box<Expr>),
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    /// `x = [a, b]`: valor, límites, inclusivos, negado.
    Range(Box<Expr>, Box<Expr>, Box<Expr>, bool, bool, bool),
    T(Trig, Vec<Expr>),
    Redir(Redir, Option<Box<Expr>>, Box<Expr>),
    /// `var(n) := x`; el `Trig` es Var, FVar, SysVar o SysFVar.
    Assign(Trig, Box<Expr>, Box<Expr>),
    Command(String),
    Const(String),
    GetHitVar(String),
    /// `StateType = S`: quién (0 = yo, 1 = P2), qué (0 = statetype,
    /// 1 = movetype) y la letra.
    StateIs(u8, u8, u8),
    /// `HitDefAttr = SC, NA`: máscara y si es `!=`.
    HitDefAttr(u32, bool),
    /// Comparación de nombre (`name`, `p2name`, `authorname`): cuál y texto.
    NameIs(u8, String),
}

impl Expr {
    pub fn int(i: i32) -> Expr {
        Expr::Num(V::I(i))
    }
}

// Bits de atributo de golpe: tipo de estado y clase de ataque.
pub const AT_S: u32 = 1;
pub const AT_C: u32 = 2;
pub const AT_A: u32 = 4;
pub const AT_STATE: u32 = 7;
pub const AT_NA: u32 = 8;
pub const AT_SA: u32 = 16;
pub const AT_HA: u32 = 32;
pub const AT_NT: u32 = 64;
pub const AT_ST: u32 = 128;
pub const AT_HT: u32 = 256;
pub const AT_NP: u32 = 512;
pub const AT_SP: u32 = 1024;
pub const AT_HP: u32 = 2048;
pub const AT_CLASS: u32 = 0xff8;

/// `S, NA, SA` → máscara de bits. Si no nombra ninguna clase, valen todas.
pub fn parse_attr(s: &str) -> u32 {
    let mut m = 0;
    for (i, part) in s.split(',').enumerate() {
        let p = part.trim().to_ascii_lowercase();
        if i == 0 && p.chars().all(|c| "sca".contains(c)) {
            for c in p.chars() {
                m |= match c {
                    's' => AT_S,
                    'c' => AT_C,
                    _ => AT_A,
                };
            }
            continue;
        }
        m |= match p.as_str() {
            "na" => AT_NA,
            "sa" => AT_SA,
            "ha" => AT_HA,
            "nt" => AT_NT,
            "st" => AT_ST,
            "ht" => AT_HT,
            "np" => AT_NP,
            "sp" => AT_SP,
            "hp" => AT_HP,
            "aa" => AT_NA | AT_SA | AT_HA,
            "at" => AT_NT | AT_ST | AT_HT,
            "ap" => AT_NP | AT_SP | AT_HP,
            "n" => AT_NA | AT_NT | AT_NP,
            "s" => AT_SA | AT_ST | AT_SP,
            "h" => AT_HA | AT_HT | AT_HP,
            "a" => AT_NA | AT_SA | AT_HA,
            "t" => AT_NT | AT_ST | AT_HT,
            "p" => AT_NP | AT_SP | AT_HP,
            _ => 0,
        };
    }
    m
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(V),
    Str(String),
    Id(String),
    Sym(&'static str),
}

const SYMS: [&str; 27] = [
    ":=", "**", "||", "^^", "&&", "!=", "<=", ">=", "=", "<", ">", "+", "-", "*", "/", "%", "!", "~", "&", "^", "|", "(", ")", "[", "]", ",", ":",
];

fn lex(s: &str) -> Vec<Tok> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'"' {
            let end = s[i + 1..].find('"').map_or(s.len(), |e| i + 1 + e);
            out.push(Tok::Str(s[i + 1..end].to_string()));
            i = end + 1;
        } else if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            let mut dot = false;
            while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !dot)) {
                dot |= b[i] == b'.';
                i += 1;
            }
            let t = &s[start..i];
            out.push(Tok::Num(if dot {
                V::F(ini::parse_f32(t).unwrap_or(0.0))
            } else {
                // Los enteros que no caben se saturan, como en MUGEN.
                V::I(t.bytes().fold(0i64, |a, d| (a * 10 + (d - b'0') as i64).min(i32::MAX as i64)) as i32)
            }));
        } else if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'.') {
                i += 1;
            }
            out.push(Tok::Id(s[start..i].to_ascii_lowercase()));
        } else if let Some(sym) = SYMS.iter().find(|sym| s[i..].starts_with(**sym)) {
            out.push(Tok::Sym(sym));
            i += sym.len();
        } else {
            // Carácter desconocido (incluye bytes no ASCII): se ignora.
            i += 1;
            while i < b.len() && !s.is_char_boundary(i) {
                i += 1;
            }
        }
    }
    out
}

struct Parser {
    t: Vec<Tok>,
    p: usize,
}

fn bx(e: Expr) -> Box<Expr> {
    Box::new(e)
}

fn simple(name: &str) -> Option<Trig> {
    use Trig::*;
    Some(match name {
        "time" | "statetime" => Time,
        "stateno" => StateNo,
        "prevstateno" => PrevStateNo,
        "anim" => Anim,
        "animtime" => AnimTime,
        "animelemno" => AnimElemNo,
        "animelemtime" => AnimElemTime,
        "animexist" => AnimExist,
        "selfanimexist" => SelfAnimExist,
        "ctrl" => Ctrl,
        "life" => Life,
        "lifemax" => LifeMax,
        "power" => Power,
        "powermax" => PowerMax,
        "alive" => Alive,
        "facing" => Facing,
        "random" => Random,
        "roundstate" => RoundState,
        "roundno" => RoundNo,
        "roundsexisted" => RoundsExisted,
        "matchover" => MatchOver,
        "movecontact" => MoveContact,
        "movehit" => MoveHit,
        "moveguarded" => MoveGuarded,
        "movereversed" => MoveReversed,
        "hitshakeover" => HitShakeOver,
        "hitover" => HitOver,
        "hitfall" => HitFall,
        "canrecover" => CanRecover,
        "hitcount" | "uniqhitcount" => HitCount,
        "hitpausetime" => HitPauseTime,
        "inguarddist" => InGuardDist,
        "frontedgedist" => FrontEdgeDist,
        "frontedgebodydist" => FrontEdgeBodyDist,
        "backedgedist" => BackEdgeDist,
        "backedgebodydist" => BackEdgeBodyDist,
        "gametime" => GameTime,
        "palno" => PalNo,
        "win" | "winko" | "wintime" | "winperfect" => Win,
        "lose" | "loseko" | "losetime" => Lose,
        "drawgame" => DrawGame,
        "numenemy" => NumEnemy,
        "numpartner" => NumPartner,
        "numhelper" => NumHelper,
        "numexplod" => NumExplod,
        "numproj" => NumProj,
        "numprojid" => NumProjId,
        "playeridexist" => PlayerIdExist,
        "projcontacttime" => ProjContactTime,
        "projhittime" => ProjHitTime,
        "projguardedtime" => ProjGuardedTime,
        "projcanceltime" => ProjCancelTime,
        "numtarget" => NumTarget,
        "ishelper" => IsHelper,
        "id" => Id,
        "pi" => Pi,
        "e" => E,
        "tickspersecond" => TicksPerSecond,
        "ailevel" => AiLevel,
        "teamside" => TeamSide,
        "p2life" => P2Life,
        "p2stateno" => P2StateNo,
        "var" => Var,
        "fvar" => FVar,
        "sysvar" => SysVar,
        "sysfvar" => SysFVar,
        "abs" => Abs,
        "ceil" => Ceil,
        "floor" => Floor,
        "exp" => Exp,
        "ln" => Ln,
        "log" => Log,
        "sin" => Sin,
        "cos" => Cos,
        "tan" => Tan,
        "asin" => Asin,
        "acos" => Acos,
        "atan" => Atan,
        "ifelse" | "cond" => IfElse,
        "const720p" => Const720p,
        "const240p" => Const240p,
        "const480p" => Const480p,
        "gamewidth" => GameWidth,
        "gameheight" => GameHeight,
        // Triggers que este motor no modela: valen 0.
        "ishometeam" | "teammode" | "isbound" | "matchno" | "timemod" | "movehitcount" => Zero,
        _ => return None,
    })
}

fn axis(name: &str, y: bool) -> Option<Trig> {
    use Trig::*;
    Some(match (name, y) {
        ("vel", false) => VelX,
        ("vel", true) => VelY,
        ("pos", false) => PosX,
        ("pos", true) => PosY,
        ("p2dist", false) => P2DistX,
        ("p2dist", true) => P2DistY,
        ("p2bodydist", false) => P2BodyDistX,
        ("p2bodydist", true) => P2BodyDistY,
        ("screenpos", false) => ScreenPosX,
        ("screenpos", true) => ScreenPosY,
        ("hitvel", false) => HitVelX,
        ("hitvel", true) => HitVelY,
        ("camerapos", false) => CameraPosX,
        ("camerapos", true) => CameraPosY,
        ("rootdist", false) => RootDistX,
        ("rootdist", true) => RootDistY,
        ("parentdist", false) => ParentDistX,
        ("parentdist", true) => ParentDistY,
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.p)
    }

    fn is(&self, s: &str) -> bool {
        matches!(self.peek(), Some(Tok::Sym(x)) if *x == s)
    }

    fn eat(&mut self, s: &str) -> bool {
        let hit = self.is(s);
        if hit {
            self.p += 1;
        }
        hit
    }

    fn ident(&mut self) -> Option<String> {
        if let Some(Tok::Id(s)) = self.peek() {
            let s = s.clone();
            self.p += 1;
            return Some(s);
        }
        None
    }

    fn binary(&mut self, level: usize) -> Result<Expr, String> {
        const LEVELS: [&[(&str, Op)]; 10] = [
            &[("||", Op::Or)],
            &[("^^", Op::Xor)],
            &[("&&", Op::And)],
            &[("|", Op::BitOr)],
            &[("^", Op::BitXor)],
            &[("&", Op::BitAnd)],
            &[("=", Op::Eq), ("!=", Op::Ne)],
            &[("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)],
            &[("+", Op::Add), ("-", Op::Sub)],
            &[("*", Op::Mul), ("/", Op::Div), ("%", Op::Mod)],
        ];
        if level == LEVELS.len() {
            return self.power();
        }
        let mut lhs = self.binary(level + 1)?;
        loop {
            let Some(&(_, op)) = LEVELS[level].iter().find(|(s, _)| self.is(s)) else { return Ok(lhs) };
            self.p += 1;
            if matches!(op, Op::Eq | Op::Ne) {
                if let Some(r) = self.range(&lhs, op == Op::Ne)? {
                    lhs = r;
                    continue;
                }
            }
            let rhs = self.binary(level + 1)?;
            lhs = Expr::Bin(op, bx(lhs), bx(rhs));
        }
    }

    /// Intenta leer `[a, b]`, `(a, b)`, `[a, b)` o `(a, b]` tras un `=`.
    fn range(&mut self, lhs: &Expr, neg: bool) -> Result<Option<Expr>, String> {
        let open = if self.is("[") {
            true
        } else if self.is("(") {
            false
        } else {
            return Ok(None);
        };
        let save = self.p;
        self.p += 1;
        let lo = self.binary(0);
        if lo.is_err() || !self.eat(",") {
            self.p = save;
            return Ok(None);
        }
        let hi = self.binary(0)?;
        let close = if self.eat("]") {
            true
        } else if self.eat(")") {
            false
        } else {
            return Err("intervalo sin cerrar".into());
        };
        Ok(Some(Expr::Range(bx(lhs.clone()), bx(lo?), bx(hi), open, close, neg)))
    }

    fn power(&mut self) -> Result<Expr, String> {
        let base = self.unary()?;
        if self.eat("**") {
            let exp = self.power()?;
            return Ok(Expr::Bin(Op::Pow, bx(base), bx(exp)));
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.eat("!") {
            return Ok(Expr::Not(bx(self.unary()?)));
        }
        if self.eat("~") {
            return Ok(Expr::BitNot(bx(self.unary()?)));
        }
        if self.eat("-") {
            return Ok(match self.unary()? {
                Expr::Num(V::I(i)) => Expr::Num(V::I(i.wrapping_neg())),
                Expr::Num(V::F(f)) => Expr::Num(V::F(-f)),
                e => Expr::Neg(bx(e)),
            });
        }
        if self.eat("+") {
            return self.unary();
        }
        self.primary()
    }

    fn args(&mut self) -> Result<Vec<Expr>, String> {
        let mut v = Vec::new();
        if !self.eat("(") {
            return Ok(v);
        }
        if self.eat(")") {
            return Ok(v);
        }
        loop {
            v.push(self.binary(0)?);
            if self.eat(")") {
                return Ok(v);
            }
            if !self.eat(",") {
                return Err("falta `)`".into());
            }
        }
    }

    /// Operador de comparación opcional tras un trigger de sintaxis especial.
    fn cmp_op(&mut self) -> Option<Op> {
        for (s, op) in [("!=", Op::Ne), ("<=", Op::Le), (">=", Op::Ge), ("=", Op::Eq), ("<", Op::Lt), (">", Op::Gt)] {
            if self.eat(s) {
                return Some(op);
            }
        }
        None
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.peek().cloned() {
            None => Err("expresión incompleta".into()),
            Some(Tok::Num(v)) => {
                self.p += 1;
                Ok(Expr::Num(v))
            }
            Some(Tok::Str(_)) => {
                self.p += 1;
                // Un texto suelto (comparado con un trigger que no se modela)
                // nunca es igual a nada.
                Ok(Expr::int(-999_999_999))
            }
            Some(Tok::Sym("(")) => {
                self.p += 1;
                let e = self.binary(0)?;
                if !self.eat(")") {
                    return Err("falta `)`".into());
                }
                Ok(e)
            }
            Some(Tok::Sym(s)) => Err(format!("símbolo inesperado `{s}`")),
            Some(Tok::Id(name)) => {
                self.p += 1;
                self.named(&name)
            }
        }
    }

    fn named(&mut self, name: &str) -> Result<Expr, String> {
        let redir = match name {
            "enemy" => Some(Redir::Enemy),
            "enemynear" => Some(Redir::EnemyNear),
            "root" => Some(Redir::Root),
            "parent" => Some(Redir::Parent),
            "target" => Some(Redir::Target),
            "partner" => Some(Redir::Partner),
            "helper" => Some(Redir::Helper),
            "playerid" => Some(Redir::PlayerId),
            _ => None,
        };
        if let Some(r) = redir {
            let arg = self.args()?.into_iter().next().map(bx);
            if !self.eat(",") {
                return Err(format!("falta `,` tras `{name}`"));
            }
            return Ok(Expr::Redir(r, arg, bx(self.unary()?)));
        }
        match name {
            "command" => {
                let neg = match self.cmp_op() {
                    Some(Op::Eq) => false,
                    Some(Op::Ne) => true,
                    _ => return Err("`command` sin `=`".into()),
                };
                let Some(Tok::Str(s)) = self.peek().cloned() else { return Err("`command` sin nombre".into()) };
                self.p += 1;
                let e = Expr::Command(s.to_ascii_lowercase());
                Ok(if neg { Expr::Not(bx(e)) } else { e })
            }
            "statetype" | "p2statetype" | "movetype" | "p2movetype" => {
                let who = name.starts_with("p2") as u8;
                let what = name.ends_with("movetype") as u8;
                let neg = match self.cmp_op() {
                    Some(Op::Eq) => false,
                    Some(Op::Ne) => true,
                    _ => return Err(format!("`{name}` sin `=`")),
                };
                let letter = self.ident().and_then(|s| s.bytes().next()).ok_or(format!("`{name}` sin valor"))?;
                let e = Expr::StateIs(who, what, letter);
                Ok(if neg { Expr::Not(bx(e)) } else { e })
            }
            "hitdefattr" => {
                let neg = match self.cmp_op() {
                    Some(Op::Eq) => false,
                    Some(Op::Ne) => true,
                    _ => return Err("`hitdefattr` sin `=`".into()),
                };
                let mut spec = String::new();
                loop {
                    match self.peek() {
                        Some(Tok::Id(s)) => {
                            spec.push_str(s);
                            self.p += 1;
                        }
                        _ => break,
                    }
                    // La lista sigue sólo si tras la coma viene otro atributo.
                    if self.is(",") && matches!(self.t.get(self.p + 1), Some(Tok::Id(s)) if s.len() <= 2) {
                        self.p += 1;
                        spec.push(',');
                    } else {
                        break;
                    }
                }
                Ok(Expr::HitDefAttr(parse_attr(&spec), neg))
            }
            "animelem" => {
                let Some(op) = self.cmp_op() else { return Ok(Expr::T(Trig::AnimElemNo, vec![Expr::int(0)])) };
                let n = self.binary(8)?;
                let time = Expr::T(Trig::AnimElemTime, vec![n.clone()]);
                if self.is(",") {
                    let save = self.p;
                    self.p += 1;
                    // `AnimElem = n, k` equivale a `AnimElem = n, = k`.
                    let op2 = self.cmp_op().unwrap_or(Op::Eq);
                    if let Ok(k) = self.binary(8) {
                        return Ok(Expr::Bin(op2, bx(time), bx(k)));
                    }
                    self.p = save;
                }
                Ok(match op {
                    Op::Eq => Expr::Bin(Op::Eq, bx(time), bx(Expr::int(0))),
                    Op::Ne => Expr::Bin(Op::Ne, bx(time), bx(Expr::int(0))),
                    _ => Expr::Bin(op, bx(Expr::T(Trig::AnimElemNo, vec![Expr::int(0)])), bx(n)),
                })
            }
            "timemod" => {
                let Some(op) = self.cmp_op() else { return Ok(Expr::int(0)) };
                let d = self.binary(8)?;
                if !self.eat(",") {
                    return Err("`timemod` sin resto".into());
                }
                let r = self.binary(8)?;
                let m = Expr::Bin(Op::Mod, bx(Expr::T(Trig::Time, vec![])), bx(d));
                Ok(Expr::Bin(op, bx(m), bx(r)))
            }
            "name" | "p1name" | "p2name" | "p3name" | "p4name" | "authorname" | "teammode" => {
                let which = match name {
                    "name" | "p1name" => 0,
                    "p2name" => 1,
                    "authorname" => 2,
                    _ => 3,
                };
                let neg = self.cmp_op() == Some(Op::Ne);
                let text = match self.peek().cloned() {
                    Some(Tok::Str(s)) | Some(Tok::Id(s)) => {
                        self.p += 1;
                        s
                    }
                    _ => String::new(),
                };
                let e = Expr::NameIs(which, text.to_ascii_lowercase());
                Ok(if neg { Expr::Not(bx(e)) } else { e })
            }
            "const" | "gethitvar" => {
                if !self.eat("(") {
                    return Err(format!("`{name}` sin argumento"));
                }
                let key = self.ident().unwrap_or_default();
                if !self.eat(")") {
                    return Err("falta `)`".into());
                }
                Ok(if name == "const" { Expr::Const(key) } else { Expr::GetHitVar(key) })
            }
            _ => {
                if let Some(Tok::Id(ax)) = self.peek() {
                    if let Some(t) = axis(name, ax == "y") {
                        if ax == "x" || ax == "y" {
                            self.p += 1;
                            return Ok(Expr::T(t, vec![]));
                        }
                    }
                }
                let trig = match simple(name) {
                    Some(t) => t,
                    None => {
                        if let Some(e) = self.proj_trigger(name)? {
                            return Ok(e);
                        }
                        if name != "stagevar" {
                            return Err(format!("trigger desconocido `{name}`"));
                        }
                        // No se modela: vale 0. Se descarta su argumento,
                        // que puede no ser una expresión.
                        if self.eat("(") {
                            let mut depth = 1;
                            while depth > 0 && self.p < self.t.len() {
                                if self.is("(") {
                                    depth += 1;
                                } else if self.is(")") {
                                    depth -= 1;
                                }
                                self.p += 1;
                            }
                        }
                        return Ok(Expr::T(Trig::Zero, vec![]));
                    }
                };
                let args = self.args()?;
                if matches!(trig, Trig::Var | Trig::FVar | Trig::SysVar | Trig::SysFVar) && self.eat(":=") {
                    let idx = args.into_iter().next().unwrap_or(Expr::int(0));
                    let val = self.binary(6)?;
                    return Ok(Expr::Assign(trig, bx(idx), bx(val)));
                }
                Ok(Expr::T(trig, args))
            }
        }
    }
}

impl Parser {
    /// `ProjHit[id] = valor [, operador tiempo]` y sus hermanos
    /// `ProjContact` y `ProjGuarded`. El identificador va pegado al nombre
    /// (sin él vale cualquier proyectil). Sin tiempo, se cumple sólo en el
    /// tick en que ocurre; con él, mientras el tiempo transcurrido cumpla la
    /// comparación.
    fn proj_trigger(&mut self, name: &str) -> Result<Option<Expr>, String> {
        let kinds = [("projcontact", Trig::ProjContactTime), ("projhit", Trig::ProjHitTime), ("projguarded", Trig::ProjGuardedTime)];
        let Some((rest, trig)) = kinds.iter().find_map(|(p, t)| name.strip_prefix(p).map(|r| (r, *t))) else { return Ok(None) };
        let id = if rest.is_empty() {
            0
        } else {
            match rest.parse::<i32>() {
                Ok(n) => n,
                Err(_) => return Ok(None),
            }
        };
        let time = Expr::T(trig, vec![Expr::int(id)]);
        let now = Expr::Bin(Op::Eq, bx(time.clone()), bx(Expr::int(0)));
        let Some(op) = self.cmp_op() else { return Ok(Some(now)) };
        let value = self.binary(8)?;
        let mut happened = now;
        if self.is(",") {
            let save = self.p;
            self.p += 1;
            match self.cmp_op().map(|op2| (op2, self.binary(8))) {
                Some((op2, Ok(limit))) => {
                    let since = Expr::Bin(Op::Ge, bx(time.clone()), bx(Expr::int(0)));
                    happened = Expr::Bin(Op::And, bx(since), bx(Expr::Bin(op2, bx(time), bx(limit))));
                }
                _ => self.p = save,
            }
        }
        Ok(Some(Expr::Bin(op, bx(happened), bx(value))))
    }
}

/// Analiza una expresión completa.
pub fn parse(s: &str) -> Result<Expr, String> {
    let mut p = Parser { t: lex(s), p: 0 };
    if p.t.is_empty() {
        return Err("expresión vacía".into());
    }
    let e = p.binary(0)?;
    if p.p < p.t.len() {
        return Err(format!("sobra texto en `{s}`"));
    }
    Ok(e)
}

/// Divide por las comas de primer nivel (fuera de paréntesis y comillas).
pub fn split_args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut quoted, mut start) = (0i32, false, 0);
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '(' | '[' if !quoted => depth += 1,
            ')' | ']' if !quoted => depth -= 1,
            ',' if !quoted && depth <= 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out
}

/// Analiza una lista de expresiones separadas por comas (el valor de un
/// parámetro). Una redirección lleva su propia coma (`parent, var(1)`), así
/// que un trozo que no se entiende solo se prueba unido a los siguientes.
pub fn parse_list(s: &str) -> Vec<Option<Expr>> {
    let parts = split_args(s);
    let mut out = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        let mut found = if parts[i].is_empty() { None } else { parse(parts[i]).ok().map(|e| (e, i)) };
        if found.is_none() && !parts[i].is_empty() {
            found = (i + 1..parts.len().min(i + 4)).find_map(|j| parse(&parts[i..=j].join(", ")).ok().map(|e| (e, j)));
        }
        match found {
            Some((e, last)) => {
                out.push(Some(e));
                i = last + 1;
            }
            None => {
                out.push(None);
                i += 1;
            }
        }
    }
    out
}
