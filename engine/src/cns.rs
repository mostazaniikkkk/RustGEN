//! Estados y controladores (archivos CNS, y el `[Statedef -1]` del CMD).

use crate::expr::{self, Expr};
use crate::ini::Section;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Null,
    ChangeState,
    SelfState,
    ChangeAnim,
    ChangeAnim2,
    VelSet,
    VelAdd,
    VelMul,
    PosSet,
    PosAdd,
    CtrlSet,
    StateTypeSet,
    VarSet,
    VarAdd,
    VarRangeSet,
    PlaySnd,
    StopSnd,
    HitDef,
    ReversalDef,
    HitOverride,
    NotHitBy,
    HitBy,
    Width,
    Turn,
    Explod,
    RemoveExplod,
    MakeDust,
    GameMakeAnim,
    AfterImage,
    AfterImageTime,
    PalFx,
    AllPalFx,
    BgPalFx,
    RemapPal,
    EnvShake,
    FallEnvShake,
    EnvColor,
    Pause,
    SuperPause,
    AssertSpecial,
    ScreenBound,
    PosFreeze,
    PlayerPush,
    SprPriority,
    Gravity,
    HitVelSet,
    HitFallDamage,
    HitFallVel,
    HitFallSet,
    DefenceMulSet,
    AttackMulSet,
    AttackDist,
    LifeAdd,
    LifeSet,
    PowerAdd,
    PowerSet,
    TargetBind,
    TargetState,
    TargetLifeAdd,
    TargetPowerAdd,
    TargetFacing,
    TargetVelSet,
    TargetVelAdd,
    TargetDrop,
    BindToTarget,
    MoveHitReset,
    Offset,
    Trans,
    /// Controladores que este motor acepta sin hacer nada.
    Ignored,
}

fn kind(name: &str) -> Option<Kind> {
    use Kind::*;
    Some(match name {
        "null" => Null,
        "changestate" => ChangeState,
        "selfstate" => SelfState,
        "changeanim" => ChangeAnim,
        "changeanim2" => ChangeAnim2,
        "velset" => VelSet,
        "veladd" => VelAdd,
        "velmul" => VelMul,
        "posset" => PosSet,
        "posadd" => PosAdd,
        "ctrlset" => CtrlSet,
        "statetypeset" => StateTypeSet,
        "varset" => VarSet,
        "varadd" => VarAdd,
        "varrangeset" => VarRangeSet,
        "playsnd" => PlaySnd,
        "stopsnd" => StopSnd,
        "hitdef" => HitDef,
        "reversaldef" => ReversalDef,
        "hitoverride" => HitOverride,
        "nothitby" => NotHitBy,
        "hitby" => HitBy,
        "width" => Width,
        "turn" => Turn,
        "explod" => Explod,
        "removeexplod" => RemoveExplod,
        "makedust" => MakeDust,
        "gamemakeanim" => GameMakeAnim,
        "afterimage" => AfterImage,
        "afterimagetime" => AfterImageTime,
        "palfx" => PalFx,
        "allpalfx" => AllPalFx,
        "bgpalfx" => BgPalFx,
        "remappal" => RemapPal,
        "envshake" => EnvShake,
        "fallenvshake" => FallEnvShake,
        "envcolor" => EnvColor,
        "pause" => Pause,
        "superpause" => SuperPause,
        "assertspecial" => AssertSpecial,
        "screenbound" => ScreenBound,
        "posfreeze" => PosFreeze,
        "playerpush" => PlayerPush,
        "sprpriority" => SprPriority,
        "gravity" => Gravity,
        "hitvelset" => HitVelSet,
        "hitfalldamage" => HitFallDamage,
        "hitfallvel" => HitFallVel,
        "hitfallset" => HitFallSet,
        "defencemulset" => DefenceMulSet,
        "attackmulset" => AttackMulSet,
        "attackdist" => AttackDist,
        "lifeadd" => LifeAdd,
        "lifeset" => LifeSet,
        "poweradd" => PowerAdd,
        "powerset" => PowerSet,
        "targetbind" => TargetBind,
        "targetstate" => TargetState,
        "targetlifeadd" => TargetLifeAdd,
        "targetpoweradd" => TargetPowerAdd,
        "targetfacing" => TargetFacing,
        "targetvelset" => TargetVelSet,
        "targetveladd" => TargetVelAdd,
        "targetdrop" => TargetDrop,
        "bindtotarget" => BindToTarget,
        "movehitreset" => MoveHitReset,
        "offset" => Offset,
        "trans" => Trans,
        "forcefeedback" | "displaytoclipboard" | "appendtoclipboard" | "clearclipboard" | "angledraw" | "angleset" | "angleadd"
        | "anglemul" | "victoryquote" | "sndpan" | "projectile" | "helper" | "destroyself" | "modifyexplod" | "explodbindtime"
        | "bindtoparent" | "bindtoroot" | "parentvarset" | "parentvaradd" | "varrandom" | "zoom" | "modifybgctrl"
        | "hitadd" | "tagin" | "tagout" => Ignored,
        _ => return None,
    })
}

/// Parámetro de un controlador: el texto y, si se pudieron analizar, las
/// expresiones separadas por comas.
#[derive(Clone, Debug)]
pub struct Param {
    pub raw: String,
    pub exprs: Vec<Option<Expr>>,
}

#[derive(Clone, Debug)]
pub struct Ctrl {
    pub kind: Kind,
    pub triggerall: Vec<Expr>,
    pub triggers: Vec<Vec<Expr>>,
    pub persistent: i32,
    pub ignorehitpause: bool,
    pub params: Vec<(String, Param)>,
}

impl Ctrl {
    pub fn param(&self, key: &str) -> Option<&Param> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, p)| p)
    }

    pub fn expr(&self, key: &str, i: usize) -> Option<&Expr> {
        self.param(key).and_then(|p| p.exprs.get(i)).and_then(|e| e.as_ref())
    }

    pub fn raw(&self, key: &str) -> Option<&str> {
        self.param(key).map(|p| p.raw.as_str())
    }
}

#[derive(Debug, Default)]
pub struct StateDef {
    pub no: i32,
    /// Letras en minúscula; `u` = sin cambio.
    pub stype: u8,
    pub movetype: u8,
    pub physics: u8,
    pub anim: Option<Expr>,
    pub velset: [Option<Expr>; 2],
    pub ctrl: Option<Expr>,
    pub poweradd: Option<Expr>,
    pub juggle: Option<Expr>,
    pub sprpriority: Option<Expr>,
    pub facep2: bool,
    pub hitdefpersist: bool,
    pub movehitpersist: bool,
    pub hitcountpersist: bool,
    pub ctrls: Vec<Ctrl>,
}

pub type States = HashMap<i32, Rc<StateDef>>;

fn letter(s: Option<&str>, default: u8) -> u8 {
    s.and_then(|v| v.trim().bytes().next()).map_or(default, |b| b.to_ascii_lowercase())
}

fn opt_expr(s: Option<&str>, warn: &mut Vec<String>, ctx: &str) -> Option<Expr> {
    let s = s?.trim();
    if s.is_empty() {
        return None;
    }
    match expr::parse(s) {
        Ok(e) => Some(e),
        Err(e) => {
            warn.push(format!("{ctx}: `{s}`: {e}"));
            None
        }
    }
}

fn parse_param(value: &str, warn: &mut Vec<String>, ctx: &str) -> Param {
    let exprs = expr::split_args(value).into_iter().map(|a| if a.is_empty() { None } else { expr::parse(a).ok() }).collect();
    let _ = (warn, ctx);
    Param { raw: value.trim().to_string(), exprs }
}

fn trigger_index(key: &str) -> Option<usize> {
    key.strip_prefix("trigger").and_then(|n| n.parse::<usize>().ok())
}

/// Añade a `states` los estados del texto ya dividido en secciones. Los
/// estados que ya existen no se reemplazan (gana el primero cargado).
pub fn parse_states(secs: &[Section], states: &mut States, warn: &mut Vec<String>) {
    let mut cur: Option<StateDef> = None;
    let mut skip = false;
    macro_rules! flush {
        () => {
            if let Some(sd) = cur.take() {
                if !skip {
                    states.insert(sd.no, Rc::new(sd));
                }
            }
        };
    }
    for s in secs {
        if let Some(rest) = s.name.strip_prefix("statedef") {
            flush!();
            let Some(no) = crate::ini::num(rest) else { continue };
            let no = no as i32;
            skip = states.contains_key(&no);
            let ctx = format!("statedef {no}");
            let vel = s.get("velset").map(expr::split_args).unwrap_or_default();
            cur = Some(StateDef {
                no,
                stype: letter(s.get("type"), b's'),
                movetype: letter(s.get("movetype"), b'i'),
                physics: letter(s.get("physics"), b'n'),
                anim: opt_expr(s.get("anim"), warn, &ctx),
                velset: [opt_expr(vel.first().copied(), warn, &ctx), opt_expr(vel.get(1).copied(), warn, &ctx)],
                ctrl: opt_expr(s.get("ctrl"), warn, &ctx),
                poweradd: opt_expr(s.get("poweradd"), warn, &ctx),
                juggle: opt_expr(s.get("juggle"), warn, &ctx),
                sprpriority: opt_expr(s.get("sprpriority"), warn, &ctx),
                facep2: s.i32("facep2", 0) != 0,
                hitdefpersist: s.i32("hitdefpersist", 0) != 0,
                movehitpersist: s.i32("movehitpersist", 0) != 0,
                hitcountpersist: s.i32("hitcountpersist", 0) != 0,
                ctrls: Vec::new(),
            });
            continue;
        }
        if !s.name.starts_with("state ") && s.name != "state" {
            continue;
        }
        let Some(sd) = cur.as_mut() else { continue };
        let ctx = format!("[{}]", s.raw_name);
        let tname = s.get("type").unwrap_or("").trim().to_ascii_lowercase();
        let Some(k) = kind(&tname) else {
            warn.push(format!("{ctx}: controlador desconocido `{tname}`"));
            continue;
        };
        let mut c = Ctrl { kind: k, triggerall: Vec::new(), triggers: Vec::new(), persistent: 1, ignorehitpause: false, params: Vec::new() };
        let mut bad = false;
        for (key, value) in &s.items {
            if key == "type" {
                continue;
            }
            if key == "triggerall" {
                match expr::parse(value) {
                    Ok(e) => c.triggerall.push(e),
                    Err(e) => {
                        warn.push(format!("{ctx}: `{value}`: {e}"));
                        bad = true;
                    }
                }
            } else if let Some(n) = trigger_index(key) {
                while c.triggers.len() < n {
                    c.triggers.push(Vec::new());
                }
                match expr::parse(value) {
                    Ok(e) => c.triggers[n - 1].push(e),
                    Err(e) => {
                        warn.push(format!("{ctx}: `{value}`: {e}"));
                        // Una condición ilegible hace falso su grupo.
                        c.triggers[n - 1].push(Expr::int(0));
                    }
                }
            } else if key == "persistent" {
                c.persistent = crate::ini::num(value).unwrap_or(1.0) as i32;
            } else if key == "ignorehitpause" {
                c.ignorehitpause = crate::ini::num(value).unwrap_or(0.0) != 0.0;
            } else {
                c.params.push((key.clone(), parse_param(value, warn, &ctx)));
            }
        }
        // Los grupos triggerN deben ser consecutivos desde 1.
        if let Some(gap) = c.triggers.iter().position(Vec::is_empty) {
            c.triggers.truncate(gap);
        }
        if !bad && !c.triggers.is_empty() {
            sd.ctrls.push(c);
        }
    }
    flush!();
}
