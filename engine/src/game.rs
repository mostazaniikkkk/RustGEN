//! Estado de la pelea: personajes, máquina de estados, física y golpes.

use crate::air::{AnimState, AnimTable};
use crate::cmd::{CommandSet, CommandState};
use crate::cns::{self, Ctrl, StateDef, States};
use crate::ents::Projectile;
use crate::expr::{Expr, Trig, V, AT_A, AT_C, AT_CLASS, AT_S, AT_STATE};
use crate::fx::{AfterImage, Explod, PalFx};
use crate::gfx::{Frame, PalRef, SoundCmd, SpriteCmd, Trans, SCREEN_W};
use crate::hud::{Fight, Round};
use crate::ini;
use crate::sff::Sff;
use crate::snd::Snd;
use crate::stage::Stage;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

// Bits de `hitflag` / `guardflag`.
pub const HF_H: u32 = 1;
pub const HF_L: u32 = 2;
pub const HF_A: u32 = 4;
pub const HF_F: u32 = 8;
pub const HF_D: u32 = 16;
pub const HF_PLUS: u32 = 32;
pub const HF_MINUS: u32 = 64;

// Banderas de `AssertSpecial` (valen un tick).
pub const AS_INTRO: u32 = 1;
pub const AS_NOWALK: u32 = 2;
pub const AS_NOAUTOTURN: u32 = 4;
pub const AS_INVISIBLE: u32 = 8;
pub const AS_NOSHADOW: u32 = 16;
pub const AS_ROUNDNOTOVER: u32 = 32;
pub const AS_NOJUGGLECHECK: u32 = 64;
pub const AS_UNGUARDABLE: u32 = 128;
pub const AS_NOSTANDGUARD: u32 = 256;
pub const AS_NOCROUCHGUARD: u32 = 512;
pub const AS_NOAIRGUARD: u32 = 1024;
pub const AS_NOKO: u32 = 2048;
pub const AS_NOBARDISPLAY: u32 = 4096;
pub const AS_TIMERFREEZE: u32 = 8192;
pub const AS_NOKOSLOW: u32 = 16384;

/// ¿El atributo `attr` de un golpe entra en la máscara `mask`? Una máscara
/// sin tipo de estado o sin clase de ataque acepta cualquiera.
pub fn attr_match(attr: u32, mask: u32) -> bool {
    let s = if mask & AT_STATE == 0 { AT_STATE } else { mask & AT_STATE };
    let k = if mask & AT_CLASS == 0 { AT_CLASS } else { mask & AT_CLASS };
    attr & s != 0 && attr & k != 0
}

#[derive(Clone, Debug)]
pub struct HitDef {
    pub attr: u32,
    pub hitflag: u32,
    pub guardflag: u32,
    pub animtype: i32,
    pub air_animtype: i32,
    pub fall_animtype: i32,
    pub damage: (i32, i32),
    pub pausetime: (i32, i32),
    pub guard_pausetime: (i32, i32),
    /// (del propio personaje, número de animación)
    pub sparkno: (bool, i32),
    pub guard_sparkno: (bool, i32),
    pub sparkxy: (f32, f32),
    /// (del propio personaje, grupo, número)
    pub hitsound: Option<(bool, i32, i32)>,
    pub guardsound: Option<(bool, i32, i32)>,
    pub ground_type: i32,
    pub air_type: i32,
    pub ground_slidetime: i32,
    pub guard_slidetime: i32,
    pub ground_hittime: i32,
    pub guard_hittime: i32,
    pub air_hittime: i32,
    pub guard_ctrltime: i32,
    pub airguard_ctrltime: i32,
    pub guard_dist: Option<f32>,
    pub yaccel: f32,
    pub ground_vel: (f32, f32),
    pub guard_vel: f32,
    pub air_vel: (f32, f32),
    pub airguard_vel: (f32, f32),
    pub ground_cornerpush: f32,
    pub air_cornerpush: f32,
    pub down_cornerpush: f32,
    pub guard_cornerpush: f32,
    pub airguard_cornerpush: f32,
    pub p1sprpriority: i32,
    pub p2sprpriority: i32,
    pub p1facing: i32,
    pub p1getp2facing: i32,
    pub p2facing: i32,
    pub p1stateno: Option<i32>,
    pub p2stateno: Option<i32>,
    pub p2getp1state: bool,
    pub forcestand: bool,
    pub fall: bool,
    pub air_fall: bool,
    pub fall_xvel: Option<f32>,
    pub fall_yvel: f32,
    pub fall_recover: bool,
    pub fall_recovertime: i32,
    pub fall_damage: i32,
    pub down_vel: (f32, f32),
    pub down_hittime: i32,
    pub down_bounce: bool,
    pub chainid: i32,
    pub kill: bool,
    pub guard_kill: bool,
    pub fall_kill: bool,
    pub numhits: i32,
    pub getpower: (i32, i32),
    pub givepower: (i32, i32),
    pub palfx: Option<PalFx>,
    /// tiempo, frecuencia, amplitud, fase
    pub envshake: (i32, f32, i32, f32),
    pub fall_envshake: (i32, f32, i32, f32),
}

/// Datos del último golpe recibido (trigger `GetHitVar`).
#[derive(Clone, Debug, Default)]
pub struct Ghv {
    pub attr: u32,
    pub htype: i32,
    pub animtype: i32,
    pub airtype: i32,
    pub groundtype: i32,
    pub damage: i32,
    pub hitcount: i32,
    pub fallcount: i32,
    pub hitshake: i32,
    pub hittime: i32,
    pub slidetime: i32,
    pub ctrltime: i32,
    pub recovertime: i32,
    pub xvel: f32,
    pub yvel: f32,
    pub yaccel: f32,
    pub chainid: i32,
    pub guarded: bool,
    pub fall: bool,
    pub fall_damage: i32,
    pub fall_xvel: Option<f32>,
    pub fall_yvel: f32,
    pub fall_recover: bool,
    pub fall_time: i32,
    pub fall_recovertime: i32,
    pub fall_kill: bool,
    pub fall_shake: (i32, f32, i32, f32),
    pub down_bounce: bool,
}

#[derive(Clone, Debug)]
pub struct Reversal {
    pub attr: u32,
    pub pausetime: (i32, i32),
    pub sparkno: (bool, i32),
    pub sparkxy: (f32, f32),
    pub hitsound: Option<(bool, i32, i32)>,
    pub p1stateno: Option<i32>,
    pub p2stateno: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HitOverride {
    pub attr: u32,
    pub stateno: i32,
    pub time: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct Bind {
    pub to: usize,
    pub pos: (f32, f32),
    pub time: i32,
    /// 1 = mira como aquel a quien sigue, -1 = al revés, 0 = no cambia.
    pub facing: i32,
}

/// Sin dueño: el `parent` de un helper cuyo creador ya no existe.
pub const NOBODY: usize = usize::MAX;

/// Datos fijos de un personaje (lo que hay en sus archivos).
pub struct CharDef {
    pub name: String,
    pub display: String,
    pub author: String,
    pub sff: u16,
    pub snd: u16,
    pub anims: AnimTable,
    pub cmds: CommandSet,
    pub states: States,
    pub consts: HashMap<String, f32>,
    pub life: i32,
    pub attack: f32,
    pub defence: f32,
    pub fall_defence_mul: f32,
    pub liedown_time: i32,
    pub airjuggle: i32,
    pub sparkno: i32,
    pub guard_sparkno: i32,
    pub xscale: f32,
    pub yscale: f32,
    pub ground_back: f32,
    pub ground_front: f32,
    pub air_back: f32,
    pub air_front: f32,
    pub height: f32,
    pub attack_dist: f32,
    pub proj_attack_dist: f32,
    pub shadowoffset: f32,
    pub draw_offset: (f32, f32),
    pub airjump_num: i32,
    pub airjump_height: f32,
    pub yaccel: f32,
    pub stand_friction: f32,
    pub crouch_friction: f32,
    /// Paletas `.act` cargadas (índice dentro del SFF), por número de paleta.
    pub act_pals: Vec<u16>,
    /// Frases de victoria (`[Quotes]`).
    pub quotes: Vec<String>,
    /// Storyboards del modo arcade: intro y final.
    pub intro: Option<String>,
    pub ending: Option<String>,
    pub warnings: Vec<String>,
}

const DEFAULT_CONSTS: [(&str, f32); 26] = [
    ("movement.stand.friction.threshold", 2.0),
    ("movement.crouch.friction.threshold", 0.05),
    ("movement.air.gethit.groundlevel", 25.0),
    ("movement.air.gethit.groundrecover.ground.threshold", -20.0),
    ("movement.air.gethit.groundrecover.groundlevel", 10.0),
    ("movement.air.gethit.airrecover.threshold", -1.0),
    ("movement.air.gethit.airrecover.yaccel", 0.35),
    ("movement.air.gethit.trip.groundlevel", 15.0),
    ("movement.down.bounce.offset.x", 0.0),
    ("movement.down.bounce.offset.y", 20.0),
    ("movement.down.bounce.yaccel", 0.4),
    ("movement.down.bounce.groundlevel", 12.0),
    ("movement.down.friction.threshold", 0.05),
    ("velocity.air.gethit.groundrecover.x", -0.15),
    ("velocity.air.gethit.groundrecover.y", -3.5),
    ("velocity.air.gethit.airrecover.mul.x", 0.5),
    ("velocity.air.gethit.airrecover.mul.y", 0.2),
    ("velocity.air.gethit.airrecover.add.x", 0.0),
    ("velocity.air.gethit.airrecover.add.y", -4.5),
    ("velocity.air.gethit.airrecover.back", -1.0),
    ("velocity.air.gethit.airrecover.fwd", 0.0),
    ("velocity.air.gethit.airrecover.up", -2.0),
    ("velocity.air.gethit.airrecover.down", 1.5),
    ("size.xscale", 1.0),
    ("size.yscale", 1.0),
    ("movement.airjump.num", 0.0),
];

impl CharDef {
    /// Carga un personaje desde su `.def`. `data_dir` es donde está
    /// `common1.cns`.
    pub fn load(def_path: &str, data_dir: &str, sffs: &mut Vec<Sff>, snds: &mut Vec<Snd>) -> Result<CharDef, String> {
        let secs = ini::load(def_path).map_err(|e| format!("{def_path}: {e}"))?;
        let info = secs.iter().find(|s| s.name == "info").ok_or("falta [Info]")?;
        let files = secs.iter().find(|s| s.name == "files").ok_or("falta [Files]")?;
        let file = |k: &str| files.str(k).filter(|s| !s.is_empty()).map(|f| ini::sibling(def_path, &f));
        let mut warnings = Vec::new();

        let mut sff = Sff::load(&file("sprite").ok_or("falta sprite")?)?;
        let mut act_pals = Vec::new();
        for n in 1..=12 {
            if let Some(p) = file(&format!("pal{n}")).and_then(|f| crate::sff::load_act(&f)) {
                act_pals.push(sff.pals.len() as u16);
                sff.pals.push(p);
            }
        }
        let sff_id = sffs.len() as u16;
        sffs.push(sff);
        let snd = match file("sound") {
            Some(f) => Snd::load(&f).unwrap_or_else(|e| {
                warnings.push(e);
                Snd::default()
            }),
            None => Snd::default(),
        };
        let snd_id = snds.len() as u16;
        snds.push(snd);

        let anims = crate::air::load(&file("anim").ok_or("falta anim")?).map_err(|e| format!("anim: {e}"))?;
        let cmd_path = file("cmd").ok_or("falta cmd")?;
        let (cmds, cmd_secs) = CommandSet::load(&cmd_path).map_err(|e| format!("{cmd_path}: {e}"))?;

        // Estados: los archivos del personaje, el CMD y al final los comunes.
        let mut states = States::new();
        let mut seen: Vec<String> = Vec::new();
        let mut state_files: Vec<String> = Vec::new();
        for key in ["st", "st0", "st1", "st2", "st3", "st4", "st5", "st6", "st7", "st8", "st9", "cns"] {
            if let Some(f) = file(key) {
                state_files.push(f);
            }
        }
        let cns_path = file("cns").ok_or("falta cns")?;
        for f in state_files {
            if seen.contains(&f.to_ascii_lowercase()) {
                continue;
            }
            seen.push(f.to_ascii_lowercase());
            match ini::load(&f) {
                Ok(s) => cns::parse_states(&s, &mut states, &mut warnings),
                Err(e) => warnings.push(format!("{f}: {e}")),
            }
        }
        cns::parse_states(&cmd_secs, &mut states, &mut warnings);
        if let Some(common) = files.str("stcommon") {
            let local = ini::sibling(def_path, &common);
            let shared = format!("{data_dir}/{common}");
            match ini::load(&local).or_else(|_| ini::load(&shared)) {
                Ok(s) => cns::parse_states(&s, &mut states, &mut warnings),
                Err(e) => warnings.push(format!("{common}: {e}")),
            }
        }

        // Constantes: [Data], [Size], [Velocity], [Movement].
        let mut consts: HashMap<String, f32> = DEFAULT_CONSTS.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let cns_secs = ini::load(&cns_path).map_err(|e| format!("{cns_path}: {e}"))?;
        for s in &cns_secs {
            if !matches!(s.name.as_str(), "data" | "size" | "velocity" | "movement") {
                continue;
            }
            for (k, v) in &s.items {
                let vals = ini::nums(v, 0.0);
                let key = format!("{}.{}", s.name, k);
                if vals.len() >= 2 {
                    consts.insert(format!("{key}.x"), vals[0]);
                    consts.insert(format!("{key}.y"), vals[1]);
                } else if let Some(&x) = vals.first() {
                    consts.insert(format!("{key}.x"), x);
                    consts.insert(key, x);
                }
            }
        }
        for (alias, src) in [
            ("velocity.jump.y", "velocity.jump.neu.y"),
            ("velocity.airjump.y", "velocity.airjump.neu.y"),
            ("velocity.runjump.y", "velocity.runjump.fwd.y"),
        ] {
            if let Some(&v) = consts.get(src) {
                consts.insert(alias.to_string(), v);
            }
        }
        if !consts.contains_key("velocity.airjump.y") {
            let v = consts.get("velocity.jump.y").copied().unwrap_or(-8.0);
            consts.insert("velocity.airjump.y".into(), v);
        }
        let k = |name: &str, d: f32| consts.get(name).copied().unwrap_or(d);
        let quotes = cns_secs
            .iter()
            .filter(|s| s.name == "quotes")
            .flat_map(|s| s.items.iter().filter(|(k, _)| k.starts_with("victory")).map(|(_, v)| ini::unquote(v)))
            .collect();
        let story = |key: &str| {
            let arcade = secs.iter().find(|s| s.name == "arcade")?;
            arcade.str(key).filter(|f| !f.is_empty()).map(|f| ini::sibling(def_path, &f))
        };
        Ok(CharDef {
            quotes,
            intro: story("intro.storyboard"),
            ending: story("ending.storyboard"),
            name: info.str("name").unwrap_or_default(),
            display: info.str("displayname").or_else(|| info.str("name")).unwrap_or_default(),
            author: info.str("author").unwrap_or_default(),
            sff: sff_id,
            snd: snd_id,
            anims,
            cmds,
            life: k("data.life", 1000.0) as i32,
            attack: k("data.attack", 100.0),
            defence: k("data.defence", 100.0),
            fall_defence_mul: 100.0 / (100.0 + k("data.fall.defence_up", 50.0)),
            liedown_time: k("data.liedown.time", 60.0) as i32,
            airjuggle: k("data.airjuggle", 15.0) as i32,
            sparkno: k("data.sparkno", 2.0) as i32,
            guard_sparkno: k("data.guard.sparkno", 40.0) as i32,
            xscale: k("size.xscale", 1.0),
            yscale: k("size.yscale", 1.0),
            ground_back: k("size.ground.back", 15.0),
            ground_front: k("size.ground.front", 16.0),
            air_back: k("size.air.back", 12.0),
            air_front: k("size.air.front", 12.0),
            height: k("size.height", 60.0),
            attack_dist: k("size.attack.dist", 160.0),
            proj_attack_dist: k("size.proj.attack.dist", 90.0),
            shadowoffset: k("size.shadowoffset", 0.0),
            draw_offset: (k("size.draw.offset.x", 0.0), k("size.draw.offset.y", 0.0)),
            airjump_num: k("movement.airjump.num", 0.0) as i32,
            airjump_height: k("movement.airjump.height", 35.0),
            yaccel: k("movement.yaccel", 0.44),
            stand_friction: k("movement.stand.friction", 0.85),
            crouch_friction: k("movement.crouch.friction", 0.82),
            states,
            consts,
            act_pals,
            warnings,
        })
    }
}

/// Estado en vivo de un personaje.
pub struct Char {
    pub def: Rc<CharDef>,
    pub idx: usize,
    pub pos: [f32; 2],
    pub vel: [f32; 2],
    pub facing: i32,
    pub stateno: i32,
    pub prevstateno: i32,
    pub time: i32,
    pub stype: u8,
    pub movetype: u8,
    pub physics: u8,
    pub ctrl: bool,
    pub life: i32,
    /// Vida máxima: la del personaje por el porcentaje de las opciones.
    pub life_max: i32,
    pub power: i32,
    pub anim: AnimState,
    pub ivar: Vec<Cell<i32>>,
    pub fvar: Vec<Cell<f32>>,
    pub sysvar: Vec<Cell<i32>>,
    pub sysfvar: Vec<Cell<f32>>,
    pub cmd: CommandState,
    /// De quién es la tabla de estados en uso (otro jugador = estado ajeno).
    pub state_owner: usize,
    pub persist: Vec<i32>,
    pub persist_neg: [Vec<i32>; 3],
    // Ataque
    pub hitdef: Option<Rc<HitDef>>,
    pub hitdef_active: bool,
    pub move_contact: i32,
    pub move_hit: i32,
    pub move_guarded: i32,
    pub move_reversed: i32,
    pub hitcount: i32,
    pub hitpause: i32,
    pub state_juggle: i32,
    pub juggle_paid: bool,
    pub targets: Vec<usize>,
    pub reversal: Option<Reversal>,
    // Defensa
    pub ghv: Ghv,
    pub juggle_left: i32,
    pub nothitby: [(u32, i32, bool); 2],
    pub hitoverride: [HitOverride; 8],
    pub bind: Option<Bind>,
    pub defence_mul: f32,
    pub attack_mul: f32,
    // Efímeros (valen un tick)
    pub assert: u32,
    pub width_player: (f32, f32),
    pub width_edge: (f32, f32),
    pub posfreeze: (bool, bool),
    pub screenbound: bool,
    pub movecamera: (bool, bool),
    pub playerpush: bool,
    // Presentación
    pub sprpriority: i32,
    pub pal: u16,
    pub palno: i32,
    pub palfx: PalFx,
    pub afterimage: AfterImage,
    pub airjumps: i32,
    pub ai: bool,
    pub ai_mem: crate::ai::AiMem,
    pub keys: u16,
    pub combo: i32,
    /// Vida "retrasada" de la barra (la parte roja).
    pub life_lag: f32,
    // Helpers. Los jugadores son los índices 0 y 1; el resto de la lista
    // son helpers, y un hueco con `active = false` está libre.
    pub active: bool,
    pub helper: bool,
    /// Quién lo creó (`NOBODY` si ya no existe) y de qué jugador desciende.
    pub parent: usize,
    pub root: usize,
    /// Bando: 0 o 1.
    pub team: usize,
    pub helper_id: i32,
    /// Identificador único (trigger `ID`).
    pub uid: i32,
    /// Lee los comandos del jugador.
    pub keyctrl: bool,
    /// Tiene sus propios efectos de paleta en vez de los de su jugador.
    pub own_pal: bool,
    pub scale: (f32, f32),
    /// Ticks que puede moverse durante una pausa y una superpausa.
    pub pause_move: (i32, i32),
    /// Tick en que se creó: empieza a actuar en el siguiente.
    pub born: i32,
    /// Parado por una pausa en este tick.
    pub frozen: bool,
}

/// Lo que un golpe cambia en quien lo da. Un proyectil golpea "a través"
/// de su dueño y después se lo devuelve todo.
pub(crate) struct AttackSnap {
    pos: [f32; 2],
    vel: [f32; 2],
    facing: i32,
    stype: u8,
    juggle_paid: bool,
    hitdef_active: bool,
    moves: [i32; 3],
    hitpause: i32,
    hitcount: i32,
    sprpriority: i32,
    targets: usize,
}

impl Char {
    pub fn new(def: Rc<CharDef>, idx: usize, palno: i32, sffs: &[Sff]) -> Char {
        let sff = &sffs[def.sff as usize];
        // SFF v2: paleta (1, palno). SFF v1: el archivo .act número palno.
        let pal = sff
            .pal_index
            .get(&(1, palno as u16))
            .copied()
            .or_else(|| def.act_pals.get((palno - 1).max(0) as usize).copied())
            .unwrap_or(0);
        let cmd = CommandState::new(&def.cmds);
        let scale = (def.xscale, def.yscale);
        Char {
            life: def.life,
            life_max: def.life,
            juggle_left: def.airjuggle,
            def,
            idx,
            pos: [0.0; 2],
            vel: [0.0; 2],
            facing: 1,
            stateno: 0,
            prevstateno: 0,
            time: 0,
            stype: b's',
            movetype: b'i',
            physics: b's',
            ctrl: false,
            power: 0,
            anim: AnimState::default(),
            ivar: (0..60).map(|_| Cell::new(0)).collect(),
            fvar: (0..40).map(|_| Cell::new(0.0)).collect(),
            sysvar: (0..5).map(|_| Cell::new(0)).collect(),
            sysfvar: (0..5).map(|_| Cell::new(0.0)).collect(),
            cmd,
            state_owner: idx,
            persist: Vec::new(),
            persist_neg: [Vec::new(), Vec::new(), Vec::new()],
            hitdef: None,
            hitdef_active: false,
            move_contact: 0,
            move_hit: 0,
            move_guarded: 0,
            move_reversed: 0,
            hitcount: 0,
            hitpause: 0,
            state_juggle: 0,
            juggle_paid: false,
            targets: Vec::new(),
            reversal: None,
            ghv: Ghv::default(),
            nothitby: [(0, 0, false); 2],
            hitoverride: [HitOverride::default(); 8],
            bind: None,
            defence_mul: 1.0,
            attack_mul: 1.0,
            assert: 0,
            width_player: (0.0, 0.0),
            width_edge: (0.0, 0.0),
            posfreeze: (false, false),
            screenbound: true,
            movecamera: (true, true),
            playerpush: true,
            sprpriority: 0,
            pal,
            palno,
            palfx: PalFx::default(),
            afterimage: AfterImage::default(),
            airjumps: 0,
            ai: false,
            ai_mem: Default::default(),
            keys: 0,
            combo: 0,
            life_lag: 0.0,
            active: true,
            helper: false,
            parent: idx,
            root: idx,
            team: idx & 1,
            helper_id: 0,
            uid: idx as i32 + 56,
            keyctrl: true,
            own_pal: false,
            scale,
            pause_move: (0, 0),
            born: -1,
            frozen: false,
        }
    }

    pub(crate) fn attack_snapshot(&self) -> AttackSnap {
        AttackSnap {
            pos: self.pos,
            vel: self.vel,
            facing: self.facing,
            stype: self.stype,
            juggle_paid: self.juggle_paid,
            hitdef_active: self.hitdef_active,
            moves: [self.move_contact, self.move_hit, self.move_guarded],
            hitpause: self.hitpause,
            hitcount: self.hitcount,
            sprpriority: self.sprpriority,
            targets: self.targets.len(),
        }
    }

    pub(crate) fn restore_attack(&mut self, s: AttackSnap) {
        self.pos = s.pos;
        self.vel = s.vel;
        self.facing = s.facing;
        self.stype = s.stype;
        self.juggle_paid = s.juggle_paid;
        self.hitdef_active = s.hitdef_active;
        [self.move_contact, self.move_hit, self.move_guarded] = s.moves;
        self.hitpause = s.hitpause;
        self.hitcount = s.hitcount;
        self.sprpriority = s.sprpriority;
        self.targets.truncate(s.targets);
    }

    /// Ancho del cuerpo por delante o por detrás, según esté en el aire.
    pub fn width(&self, front: bool) -> f32 {
        let d = &self.def;
        let base = match (self.stype == b'a', front) {
            (false, true) => d.ground_front,
            (false, false) => d.ground_back,
            (true, true) => d.air_front,
            (true, false) => d.air_back,
        };
        base + if front { self.width_player.0 } else { self.width_player.1 }
    }

    pub fn get_var(&self, kind: Trig, i: i32) -> V {
        let i = i.max(0) as usize;
        match kind {
            Trig::Var => V::I(self.ivar.get(i).map_or(0, Cell::get)),
            Trig::FVar => V::F(self.fvar.get(i).map_or(0.0, Cell::get)),
            Trig::SysVar => V::I(self.sysvar.get(i).map_or(0, Cell::get)),
            _ => V::F(self.sysfvar.get(i).map_or(0.0, Cell::get)),
        }
    }

    pub fn set_var(&self, kind: Trig, i: i32, v: V) {
        let i = i.max(0) as usize;
        match kind {
            Trig::Var => {
                if let Some(c) = self.ivar.get(i) {
                    c.set(v.i())
                }
            }
            Trig::FVar => {
                if let Some(c) = self.fvar.get(i) {
                    c.set(v.f())
                }
            }
            Trig::SysVar => {
                if let Some(c) = self.sysvar.get(i) {
                    c.set(v.i())
                }
            }
            _ => {
                if let Some(c) = self.sysfvar.get(i) {
                    c.set(v.f())
                }
            }
        }
    }

    pub fn has(&self, flag: u32) -> bool {
        self.assert & flag != 0
    }

    /// Cajas de colisión del cuadro actual, en coordenadas del escenario.
    pub(crate) fn boxes(&self, attack: bool) -> Vec<[f32; 4]> {
        let Some(f) = self.anim.frame() else { return Vec::new() };
        let list = if attack { &f.clsn1 } else { &f.clsn2 };
        let (sx, sy) = (self.facing as f32 * self.scale.0, self.scale.1);
        list.iter()
            .map(|b| {
                let (x1, x2) = (self.pos[0] + b[0] as f32 * sx, self.pos[0] + b[2] as f32 * sx);
                [x1.min(x2), self.pos[1] + b[1] as f32 * sy, x1.max(x2), self.pos[1] + b[3] as f32 * sy]
            })
            .collect()
    }
}

pub(crate) fn overlap(a: &[[f32; 4]], b: &[[f32; 4]]) -> bool {
    a.iter().any(|p| b.iter().any(|q| p[0] <= q[2] && q[0] <= p[2] && p[1] <= q[3] && q[1] <= p[3]))
}

#[derive(Default, Clone, Copy)]
pub struct PauseState {
    pub time: i32,
    pub movetime: i32,
    pub by: usize,
    /// SuperPause: oscurece el fondo.
    pub darken: bool,
    /// Recién creada: empieza a contar en el tick siguiente, para que el
    /// tick que la lanzó termine normalmente.
    pub fresh: bool,
}

impl PauseState {
    pub fn on(&self) -> bool {
        self.time > 0 && !self.fresh
    }
}

#[derive(Default, Clone, Copy)]
pub struct Shake {
    pub time: i32,
    pub total: i32,
    pub freq: f32,
    pub ampl: f32,
    pub phase: f32,
}

pub struct Game {
    pub sffs: Vec<Sff>,
    pub snds: Vec<Snd>,
    pub chars: Vec<Char>,
    pub stage: Stage,
    pub fight: Rc<Fight>,
    pub round: Round,
    pub explods: Vec<Explod>,
    pub projs: Vec<Projectile>,
    /// Último tick en que un proyectil golpeó, fue bloqueado o anulado:
    /// (jugador, identificador, [golpe, bloqueo, anulación]).
    pub proj_log: Vec<(usize, i32, [i32; 3])>,
    pub next_uid: i32,
    pub cam: [f32; 2],
    pub tick: i32,
    pub pause: PauseState,
    pub shake: Shake,
    pub rng: Cell<u32>,
    pub sounds: Vec<SoundCmd>,
    pub warnings: Vec<String>,
    /// Dibuja las cajas de colisión (depuración).
    pub show_clsn: bool,
    /// Al terminar una pelea empieza otra igual. Si no, se marca `finished`.
    pub auto_rematch: bool,
    pub finished: bool,
    /// Modo entrenamiento: sin reloj ni K.O.
    pub training: bool,
    pub show_bars: bool,
    /// Segundos del reloj de cada ronda; 0 = sin límite.
    pub round_secs: i32,
    /// Dificultad de la máquina: 1 a 8.
    pub ai_level: i32,
}

impl Game {
    /// Carga una pelea entre dos personajes por nombre (o por la ruta de su
    /// `.def`). `root` es la carpeta con `chars/`, `data/`, `stages/` y `font/`.
    pub fn new(root: &str, p1: &str, p2: &str, stage: &str) -> Result<Game, String> {
        let def = |name: &str| {
            if name.to_ascii_lowercase().ends_with(".def") {
                name.to_string()
            } else {
                format!("{root}/chars/{name}/{name}.def")
            }
        };
        Game::load(root, &format!("{root}/data/fight.def"), &def(p1), &def(p2), &format!("{root}/stages/{stage}.def"), [1, 1])
    }

    /// Carga una pelea a partir de las rutas de sus archivos. `pals` es el
    /// número de paleta que pidió cada jugador.
    pub fn load(root: &str, fight: &str, p1: &str, p2: &str, stage: &str, pals: [i32; 2]) -> Result<Game, String> {
        let mut sffs = Vec::new();
        let mut snds = Vec::new();
        let data = format!("{root}/data");
        let stage = Stage::load(stage, &mut sffs)?;
        let fight = Rc::new(Fight::load(root, fight, &mut sffs, &mut snds)?);
        let load = |path: &str, sffs: &mut Vec<Sff>, snds: &mut Vec<Snd>| -> Result<Rc<CharDef>, String> {
            CharDef::load(path, &data, sffs, snds).map(Rc::new)
        };
        let d1 = load(p1, &mut sffs, &mut snds)?;
        let d2 = if p1 == p2 { d1.clone() } else { load(p2, &mut sffs, &mut snds)? };
        let mut warnings = d1.warnings.clone();
        if !Rc::ptr_eq(&d1, &d2) {
            warnings.extend(d2.warnings.iter().cloned());
        }
        let same = Rc::ptr_eq(&d1, &d2);
        // Mismo personaje con la misma paleta: el segundo usa la siguiente.
        let pal2 = if same && pals[0] == pals[1] { pals[1] % 6 + 1 } else { pals[1] };
        let chars = vec![Char::new(d1, 0, pals[0], &sffs), Char::new(d2, 1, pal2, &sffs)];
        let mut g = Game {
            sffs,
            snds,
            chars,
            stage,
            fight,
            round: Round::default(),
            explods: Vec::new(),
            projs: Vec::new(),
            proj_log: Vec::new(),
            next_uid: 58,
            cam: [0.0; 2],
            tick: 0,
            pause: PauseState::default(),
            shake: Shake::default(),
            rng: Cell::new(0x2545_f491),
            sounds: Vec::new(),
            warnings,
            show_clsn: false,
            auto_rematch: true,
            finished: false,
            training: false,
            show_bars: true,
            round_secs: 99,
            ai_level: 4,
        };
        g.start_match();
        Ok(g)
    }

    /// Reglas de la pelea que salen de las opciones: vida inicial (en tanto
    /// por ciento de la del personaje), segundos de reloj (0 = sin límite) y
    /// dificultad de la máquina. Vuelve a empezar la pelea con ellas.
    pub fn set_rules(&mut self, life_pct: i32, round_secs: i32, ai_level: i32) {
        for ch in self.chars.iter_mut().take(2) {
            ch.life_max = (ch.def.life * life_pct / 100).max(1);
        }
        self.round_secs = round_secs;
        self.ai_level = ai_level.clamp(1, 8);
        self.start_match();
    }

    pub fn play(&mut self, bank: u16, group: i32, number: i32, chan: i32, vol: f32) {
        if let Some(idx) = self.snds[bank as usize].find(group, number) {
            self.sounds.push(SoundCmd { bank, idx, chan, vol, pan: 0.0, freq: 1.0, stop: false });
        }
    }

    // ------------------------------------------------------------ estados

    fn statedef(&self, c: usize, no: i32) -> Option<Rc<StateDef>> {
        let owner = if no < 0 { c } else { self.chars[c].state_owner };
        self.chars[owner].def.states.get(&no).cloned()
    }

    pub fn set_anim(&mut self, c: usize, no: i32, from: usize) {
        let anim = self.chars[from].def.anims.get(&no).cloned();
        if anim.is_none() {
            self.warn(format!("animación {no} inexistente (estado {})", self.chars[c].stateno));
        }
        self.chars[c].anim.set(anim, no);
    }

    pub fn warn(&mut self, msg: String) {
        if self.warnings.len() < 200 && !self.warnings.contains(&msg) {
            self.warnings.push(msg);
        }
    }

    /// Cambia de estado y aplica los parámetros del `[Statedef]`.
    pub fn change_state(&mut self, c: usize, no: i32, owner: Option<usize>) {
        if let Some(o) = owner {
            self.chars[c].state_owner = o;
        }
        let Some(sd) = self.statedef(c, no) else {
            self.warn(format!("estado {no} inexistente (desde {})", self.chars[c].stateno));
            return;
        };
        {
            let ch = &mut self.chars[c];
            ch.prevstateno = ch.stateno;
            ch.stateno = no;
            ch.time = 0;
            ch.persist = vec![0; sd.ctrls.len()];
            if sd.stype != b'u' {
                ch.stype = sd.stype;
            }
            if sd.movetype != b'u' {
                ch.movetype = sd.movetype;
            }
            if sd.physics != b'u' {
                ch.physics = sd.physics;
            }
            if !sd.hitdefpersist {
                ch.hitdef_active = false;
                ch.hitdef = None;
            }
            if !sd.movehitpersist {
                ch.move_contact = 0;
                ch.move_hit = 0;
                ch.move_guarded = 0;
                ch.move_reversed = 0;
            }
            if !sd.hitcountpersist {
                ch.hitcount = 0;
            }
            ch.reversal = None;
            ch.juggle_paid = false;
            ch.state_juggle = 0;
        }
        let owner = self.chars[c].state_owner;
        if let Some(e) = &sd.anim {
            let n = self.eval(c, e).i();
            self.set_anim(c, n, owner);
        }
        if let Some(e) = &sd.ctrl {
            self.chars[c].ctrl = self.eval(c, e).b();
        }
        for i in 0..2 {
            if let Some(e) = &sd.velset[i] {
                self.chars[c].vel[i] = self.eval(c, e).f();
            }
        }
        if let Some(e) = &sd.poweradd {
            let p = self.eval(c, e).i();
            self.add_power(c, p);
        }
        if let Some(e) = &sd.juggle {
            self.chars[c].state_juggle = self.eval(c, e).i();
        }
        if let Some(e) = &sd.sprpriority {
            self.chars[c].sprpriority = self.eval(c, e).i();
        }
        if sd.facep2 {
            let o = self.enemy(c);
            let dx = self.chars[o].pos[0] - self.chars[c].pos[0];
            if dx * (self.chars[c].facing as f32) < 0.0 {
                self.chars[c].facing = -self.chars[c].facing;
            }
        }
    }

    pub fn add_power(&mut self, c: usize, amount: i32) {
        let root = self.chars[c].root;
        let ch = &mut self.chars[root];
        ch.power = (ch.power + amount).clamp(0, 3000);
    }

    fn triggers_ok(&self, c: usize, ctrl: &Ctrl) -> bool {
        ctrl.triggerall.iter().all(|e| self.eval_bool(c, e)) && ctrl.triggers.iter().any(|g| g.iter().all(|e| self.eval_bool(c, e)))
    }

    /// Ejecuta los controladores de un estado. Devuelve `true` si alguno
    /// cambió de estado.
    fn run_state(&mut self, c: usize, sd: &StateDef, paused: bool) -> bool {
        let neg = if sd.no < 0 { Some((-sd.no - 1).clamp(0, 2) as usize) } else { None };
        if let Some(n) = neg {
            if self.chars[c].persist_neg[n].len() != sd.ctrls.len() {
                self.chars[c].persist_neg[n] = vec![0; sd.ctrls.len()];
            }
        }
        for (i, ctrl) in sd.ctrls.iter().enumerate() {
            if paused && !ctrl.ignorehitpause {
                continue;
            }
            if !self.triggers_ok(c, ctrl) {
                continue;
            }
            if ctrl.persistent != 1 {
                let ch = &mut self.chars[c];
                let slot = match neg {
                    Some(n) => ch.persist_neg[n].get_mut(i),
                    None => ch.persist.get_mut(i),
                };
                if let Some(count) = slot {
                    let fire = if ctrl.persistent <= 0 { *count == 0 } else { *count % ctrl.persistent == 0 };
                    *count += 1;
                    if !fire {
                        continue;
                    }
                }
            }
            if self.exec(c, ctrl) {
                return true;
            }
        }
        false
    }

    /// Transiciones que MUGEN hace por su cuenta cuando el jugador tiene el
    /// control: caminar, agacharse, saltar, guardia y girarse.
    fn auto_moves(&mut self, c: usize) {
        let o = self.enemy(c);
        let (hold_up, hold_down, hold_fwd, hold_back, press_up) = {
            let ch = &self.chars[c];
            let h = |n: &str| ch.cmd.is_active(&ch.def.cmds, n);
            let k = ch.cmd.current();
            let prev_up = ch.keys & 1 != 0;
            (h("holdup"), h("holddown"), h("holdfwd"), h("holdback"), k & 1 != 0 && !prev_up)
        };
        let ch = &self.chars[c];
        if !ch.ctrl || ch.state_owner != c {
            return;
        }
        let (stype, no) = (ch.stype, ch.stateno);
        // Girarse hacia el rival.
        if stype != b'a' && !ch.has(AS_NOAUTOTURN) && matches!(no, 0 | 10 | 11 | 12 | 20) {
            let dx = (self.chars[o].pos[0] - ch.pos[0]) * ch.facing as f32;
            if dx < 0.0 {
                self.chars[c].facing = -self.chars[c].facing;
                let turn = if stype == b'c' { 6 } else { 5 };
                if self.chars[c].def.anims.contains_key(&turn) && no != 20 {
                    self.set_anim(c, turn, c);
                }
                return;
            }
        }
        if hold_back && self.in_guard_dist(c) && !(120..=155).contains(&no) {
            let allowed = match stype {
                b'a' => !ch.has(AS_NOAIRGUARD),
                b'c' => !ch.has(AS_NOCROUCHGUARD),
                _ => !ch.has(AS_NOSTANDGUARD),
            };
            if allowed {
                self.change_state(c, 120, None);
                return;
            }
        }
        if stype == b'a' {
            let ch = &self.chars[c];
            if press_up && ch.airjumps < ch.def.airjump_num && ch.pos[1] <= -ch.def.airjump_height && no != 45 {
                self.chars[c].airjumps += 1;
                self.change_state(c, 45, None);
            }
            return;
        }
        if hold_up && !matches!(no, 40) {
            self.change_state(c, 40, None);
        } else if hold_down && stype == b's' && no != 10 {
            self.change_state(c, 10, None);
        } else if !hold_down && stype == b'c' && no == 11 {
            self.change_state(c, 12, None);
        } else if stype == b's' && (hold_fwd != hold_back) && !self.chars[c].has(AS_NOWALK) && matches!(no, 0 | 20) {
            if no != 20 {
                self.change_state(c, 20, None);
            }
        } else if no == 20 {
            self.change_state(c, 0, None);
        }
    }

    /// Un tick de lógica de un personaje: comandos y controladores.
    fn action(&mut self, c: usize, keys: u16) {
        if !self.chars[c].active || self.chars[c].born == self.tick {
            return;
        }
        // Los objetivos se sueltan cuando dejan de estar golpeados.
        let targets: Vec<usize> = self.chars[c].targets.clone();
        self.chars[c].targets =
            targets.into_iter().filter(|&t| self.chars[t].active && (self.chars[t].movetype == b'h' || self.chars[t].state_owner != t)).collect();

        let mut frozen = self.pause.on() && (self.pause.by != c || self.pause.movetime <= 0);
        if frozen {
            // Un helper puede traer permiso para moverse parte de la pausa.
            let darken = self.pause.darken;
            let ch = &mut self.chars[c];
            let allowed = if darken { &mut ch.pause_move.1 } else { &mut ch.pause_move.0 };
            if *allowed > 0 {
                *allowed -= 1;
                frozen = false;
            }
        }
        self.chars[c].frozen = frozen;
        {
            let ch = &mut self.chars[c];
            let rel = CommandState::relative(keys, ch.facing);
            let def = ch.def.clone();
            ch.cmd.update(&def.cmds, rel);
        }
        if self.chars[c].ai && !self.chars[c].helper {
            crate::ai::think(self, c);
        }
        if frozen {
            self.chars[c].keys = keys;
            return;
        }
        let paused = self.chars[c].hitpause > 0;
        if !paused {
            let ch = &mut self.chars[c];
            // Un helper no empuja la cámara ni choca con el borde si no lo pide.
            let player = !ch.helper;
            ch.assert = 0;
            ch.width_player = (0.0, 0.0);
            ch.width_edge = (0.0, 0.0);
            ch.posfreeze = (false, false);
            ch.screenbound = player;
            ch.movecamera = (player, player);
            ch.playerpush = true;
            for n in ch.nothitby.iter_mut() {
                if n.1 > 0 {
                    n.1 -= 1;
                }
            }
            for h in ch.hitoverride.iter_mut() {
                if h.time > 0 {
                    h.time -= 1;
                }
            }
            // Aterrizaje.
            if ch.physics == b'a' && ch.pos[1] >= 0.0 && ch.vel[1] > 0.0 && ch.state_owner == c {
                ch.airjumps = 0;
                self.change_state(c, 52, None);
            }
            if self.chars[c].stype != b'a' {
                self.chars[c].airjumps = 0;
            }
            // Tirado en el suelo: se levanta pasado `liedown.time`.
            let ch = &self.chars[c];
            if ch.stateno == 5110 && ch.time >= ch.def.liedown_time && ch.life > 0 && ch.state_owner == c {
                self.change_state(c, 5120, None);
            }
            if self.chars[c].life > 0 && self.round.state == 2 && self.chars[c].keyctrl {
                self.auto_moves(c);
            }
        }
        self.chars[c].keys = keys;

        // Un helper no ejecuta el estado -3, y el -1 sólo si lee comandos.
        let (own, helper, keyctrl) = (self.chars[c].state_owner == c, self.chars[c].helper, self.chars[c].keyctrl);
        for no in [-3, -2, -1] {
            if (no == -3 || no == -1) && !own {
                continue;
            }
            if (no == -3 && helper) || (no == -1 && !keyctrl) {
                continue;
            }
            if let Some(sd) = self.chars[c].def.states.get(&no).cloned() {
                if self.run_state(c, &sd, paused) {
                    break;
                }
            }
        }
        // El estado actual; si cambia, el nuevo se ejecuta en el mismo tick.
        for _ in 0..12 {
            // Un helper puede haberse retirado (`DestroySelf`).
            if !self.chars[c].active {
                return;
            }
            let no = self.chars[c].stateno;
            let Some(sd) = self.statedef(c, no) else { break };
            if !self.run_state(c, &sd, paused) {
                break;
            }
        }
        if !self.chars[c].active {
            return;
        }
        if paused {
            self.chars[c].hitpause -= 1;
        } else {
            self.chars[c].time += 1;
        }
    }

    /// Movimiento, animación y contadores de un personaje.
    fn update(&mut self, c: usize) {
        let tick = self.tick;
        let ch = &mut self.chars[c];
        // `action` ya descontó la pausa de golpe de este tick.
        if !ch.active || ch.born == tick || ch.frozen {
            return;
        }
        ch.palfx.step();
        if ch.hitpause > 0 {
            return;
        }
        if ch.bind.is_none() {
            if !ch.posfreeze.0 {
                ch.pos[0] += ch.vel[0] * ch.facing as f32;
            }
            if !ch.posfreeze.1 {
                ch.pos[1] += ch.vel[1];
            }
            match ch.physics {
                b's' => ch.vel[0] *= ch.def.stand_friction,
                b'c' => ch.vel[0] *= ch.def.crouch_friction,
                b'a' => ch.vel[1] += ch.def.yaccel,
                _ => {}
            }
        }
        ch.anim.update();
        if ch.ghv.hitshake > 0 {
            ch.ghv.hitshake -= 1;
        } else {
            if ch.ghv.hittime >= 0 {
                ch.ghv.hittime -= 1;
            }
            if ch.ghv.fall_recovertime > 0 {
                ch.ghv.fall_recovertime -= 1;
            }
        }
        for v in [&mut ch.move_contact, &mut ch.move_hit, &mut ch.move_guarded, &mut ch.move_reversed] {
            if *v > 0 {
                *v += 1;
            }
        }
        if ch.movetype != b'h' && ch.state_owner == c {
            ch.juggle_left = ch.def.airjuggle;
            ch.ghv.hitcount = 0;
            ch.ghv.fallcount = 0;
            ch.ghv.fall = false;
        }
        ch.afterimage.record(ch.anim.frame().map(|f| (f.group, f.image, f.hflip, f.vflip)), ch.pos, ch.facing);
    }

    fn apply_binds(&mut self) {
        for c in 0..self.chars.len() {
            let Some(b) = self.chars[c].bind else { continue };
            if !self.chars[c].active {
                continue;
            }
            let (tp, tf) = (self.chars[b.to].pos, self.chars[b.to].facing);
            let ch = &mut self.chars[c];
            ch.pos = [tp[0] + b.pos.0 * tf as f32, tp[1] + b.pos.1];
            if b.facing != 0 {
                ch.facing = tf * b.facing;
            }
            ch.bind = if b.time > 1 || b.time < 0 { Some(Bind { time: if b.time > 0 { b.time - 1 } else { b.time }, ..b }) } else { None };
        }
    }

    // ------------------------------------------------------------- golpes

    /// Crea un efecto de golpe (chispa) en el punto de contacto.
    fn spark(&mut self, a: usize, d: usize, no: (bool, i32), xy: (f32, f32)) {
        if no.1 < 0 {
            return;
        }
        let (ap, af) = (self.chars[a].pos, self.chars[a].facing as f32);
        let dch = &self.chars[d];
        let toward = dch.facing as f32 != af;
        let edge = dch.pos[0] - af * dch.width(toward);
        let pos = [edge + af * xy.0, ap[1] + xy.1];
        self.spawn_fx(a, no.0, no.1, pos, self.chars[a].facing, 5);
    }

    fn hit_sound(&mut self, a: usize, s: Option<(bool, i32, i32)>) {
        if let Some((own, g, n)) = s {
            let bank = if own { self.chars[a].def.snd } else { self.fight.common_snd };
            self.play(bank, g, n, -1, 1.0);
        }
    }

    fn can_hit(&self, a: usize, d: usize, h: &HitDef) -> bool {
        let (ach, dch) = (&self.chars[a], &self.chars[d]);
        let juggle = if ach.juggle_paid || ach.has(AS_NOJUGGLECHECK) { None } else { Some(ach.state_juggle) };
        self.hittable(d, h, juggle) && overlap(&ach.boxes(true), &dch.boxes(false))
    }

    /// Golpes entre personajes: cada ataque activo busca a quién alcanza
    /// del otro bando, primero los jugadores y después sus helpers.
    fn resolve_hits(&mut self) {
        for a in 0..self.chars.len() {
            for d in 0..self.chars.len() {
                let (ach, dch) = (&self.chars[a], &self.chars[d]);
                if !ach.active || !ach.hitdef_active || ach.hitpause > 0 {
                    break;
                }
                if !dch.active || dch.team == ach.team {
                    continue;
                }
                let Some(h) = ach.hitdef.clone() else { break };

                // Contraataque: el rival tiene un ReversalDef que toca nuestro golpe.
                if let Some(rev) = self.chars[d].reversal.clone() {
                    if rev.attr != 0 && attr_match(h.attr, rev.attr) && overlap(&self.chars[d].boxes(true), &self.chars[a].boxes(true)) {
                        self.chars[a].hitdef_active = false;
                        self.chars[a].move_reversed = 1;
                        self.chars[a].hitpause = rev.pausetime.1;
                        self.chars[d].hitpause = rev.pausetime.0;
                        self.chars[d].reversal = None;
                        self.spark(d, a, rev.sparkno, rev.sparkxy);
                        self.hit_sound(d, rev.hitsound);
                        if let Some(s) = rev.p1stateno {
                            self.change_state(d, s, Some(d));
                        }
                        if let Some(s) = rev.p2stateno {
                            self.change_state(a, s, Some(d));
                        }
                        break;
                    }
                }
                if !self.can_hit(a, d, &h) {
                    continue;
                }
                self.apply_hit(a, d, &h, false);
                break;
            }
        }
    }

    /// Aplica un golpe de `a` a `d`. `proj`: lo da un proyectil de `a`, así
    /// que no cambia el estado ni la orientación de `a`.
    pub(crate) fn apply_hit(&mut self, a: usize, d: usize, h: &HitDef, proj: bool) {
        let (dstype, dno) = (self.chars[d].stype, self.chars[d].stateno);
        let guard_ok = match dstype {
            b's' => h.guardflag & HF_H != 0,
            b'c' => h.guardflag & HF_L != 0,
            b'a' => h.guardflag & HF_A != 0,
            _ => false,
        };
        let guarded = guard_ok && (120..=155).contains(&dno) && !self.chars[a].has(AS_UNGUARDABLE);
        let same_dir = self.chars[a].facing == self.chars[d].facing;
        let flip = if same_dir { -1.0 } else { 1.0 };
        let was_hit = self.chars[d].movetype == b'h';
        let prev = self.chars[d].ghv.clone();
        let air = dstype == b'a';
        let atk = self.chars[a].def.attack / 100.0 * self.chars[a].attack_mul;
        let mut dfn = self.chars[d].def.defence / 100.0 / self.chars[d].defence_mul.max(0.01);
        if was_hit && prev.fall {
            dfn /= self.chars[d].def.fall_defence_mul.max(0.01);
        }
        // El margen evita que un redondeo de la FPU cambie el resultado.
        let scale = |dmg: i32| (dmg as f32 * atk / dfn.max(0.01) + 0.001) as i32;

        {
            let ach = &mut self.chars[a];
            ach.hitdef_active = false;
            ach.move_contact = 1;
            ach.sprpriority = h.p1sprpriority;
            if !ach.juggle_paid && was_hit && prev.fall {
                ach.juggle_paid = true;
            }
            if h.p1facing < 0 && !proj {
                ach.facing = -ach.facing;
            }
        }
        if was_hit && prev.fall {
            let cost = self.chars[a].state_juggle;
            self.chars[d].juggle_left -= cost;
        }

        let mut g = Ghv { attr: h.attr, chainid: h.chainid, yaccel: h.yaccel, guarded, ..Default::default() };
        g.groundtype = h.ground_type;
        g.airtype = h.air_type;
        g.htype = if air { h.air_type } else { h.ground_type };
        g.hitcount = if was_hit { prev.hitcount } else { 0 };
        g.fallcount = prev.fallcount;
        let next_state;
        let damage;
        if guarded {
            damage = scale(h.damage.1);
            g.hitshake = h.guard_pausetime.1;
            g.hittime = h.guard_hittime;
            g.slidetime = h.guard_slidetime;
            if air {
                g.ctrltime = h.airguard_ctrltime;
                g.xvel = h.airguard_vel.0 * flip;
                g.yvel = h.airguard_vel.1;
            } else {
                g.ctrltime = h.guard_ctrltime;
                g.xvel = h.guard_vel * flip;
            }
            next_state = match dstype {
                b'a' => 154,
                b'c' => 152,
                _ => 150,
            };
            let ach = &mut self.chars[a];
            ach.move_guarded = 1;
            ach.hitpause = h.guard_pausetime.0;
            self.add_power(a, h.getpower.1);
            self.add_power(d, h.givepower.1);
            self.spark(a, d, h.guard_sparkno, h.sparkxy);
            self.hit_sound(a, h.guardsound);
        } else {
            damage = scale(h.damage.0);
            g.hitshake = h.pausetime.1;
            g.hitcount += h.numhits.max(0);
            g.fall_damage = h.fall_damage;
            g.fall_xvel = h.fall_xvel;
            g.fall_yvel = h.fall_yvel;
            g.fall_recover = h.fall_recover;
            g.fall_recovertime = h.fall_recovertime;
            g.fall_kill = h.fall_kill;
            g.fall_shake = h.fall_envshake;
            g.down_bounce = h.down_bounce;
            if air {
                g.hittime = h.air_hittime;
                g.ctrltime = h.air_hittime;
                g.xvel = h.air_vel.0 * flip;
                g.yvel = h.air_vel.1;
                g.fall = h.air_fall || (was_hit && prev.fall);
                g.animtype = if g.fall { h.fall_animtype } else { h.air_animtype };
            } else if dstype == b'l' {
                g.hittime = h.down_hittime;
                g.ctrltime = h.down_hittime;
                g.xvel = h.down_vel.0 * flip;
                g.yvel = h.down_vel.1;
                g.fall = g.yvel != 0.0 || h.fall;
                g.animtype = h.animtype;
            } else {
                g.hittime = h.ground_hittime;
                g.slidetime = h.ground_slidetime;
                g.ctrltime = h.ground_hittime;
                g.xvel = h.ground_vel.0 * flip;
                g.yvel = h.ground_vel.1;
                g.fall = h.fall;
                g.animtype = h.animtype;
            }
            if g.fall {
                g.fallcount += 1;
            }
            next_state = if dstype == b'l' {
                5080
            } else if h.ground_type == 3 && !air {
                5070
            } else if air {
                5020
            } else if dstype == b'c' && !h.forcestand {
                5010
            } else {
                5000
            };
            let ach = &mut self.chars[a];
            ach.move_hit = 1;
            ach.hitpause = h.pausetime.0;
            ach.hitcount += h.numhits.max(0);
            if !ach.targets.contains(&d) {
                ach.targets.push(d);
            }
            // El contador de golpes seguidos es del jugador.
            let root = self.chars[a].root;
            let rch = &mut self.chars[root];
            rch.combo = if was_hit { rch.combo + h.numhits.max(0) } else { h.numhits.max(0) };
            self.add_power(a, h.getpower.0);
            self.add_power(d, h.givepower.0);
            self.spark(a, d, h.sparkno, h.sparkxy);
            self.hit_sound(a, h.hitsound);
            if h.envshake.0 > 0 {
                self.shake = Shake { time: h.envshake.0, total: h.envshake.0, freq: h.envshake.1, ampl: h.envshake.2 as f32, phase: h.envshake.3 };
            }
            if let Some(p) = &h.palfx {
                self.chars[d].palfx = p.clone();
            }
        }
        g.damage = damage;

        // Vida y K.O.
        let kill = if guarded { h.guard_kill } else { h.kill };
        {
            let dch = &mut self.chars[d];
            let noko = dch.assert & AS_NOKO != 0 || self.training;
            dch.life -= damage;
            if dch.life <= 0 {
                if kill && !noko {
                    dch.life = 0;
                    if !guarded {
                        g.fall = true;
                        if g.yvel == 0.0 {
                            g.yvel = -6.0;
                            g.xvel = if g.xvel == 0.0 { -2.0 * flip } else { g.xvel };
                        }
                    }
                } else {
                    dch.life = 1;
                }
            }
            dch.ghv = g;
            dch.ctrl = false;
            dch.sprpriority = h.p2sprpriority;
            dch.bind = None;
            if h.p2facing != 0 {
                let af = self.chars[a].facing;
                self.chars[d].facing = if h.p2facing > 0 { af } else { -af };
            }
        }
        if h.p1getp2facing != 0 && !proj {
            let df = self.chars[d].facing;
            self.chars[a].facing = if h.p1getp2facing > 0 { df } else { -df };
        }

        // Esquina: si el rival no puede retroceder, retrocede el atacante.
        let cornered = {
            let dch = &self.chars[d];
            let half = SCREEN_W as f32 / 2.0;
            let back_right = (dch.pos[0] - self.chars[a].pos[0]) > 0.0;
            let edge = if back_right { self.cam[0] + half - self.stage.screen_right } else { self.cam[0] - half + self.stage.screen_left };
            (dch.pos[0] - edge).abs() < 1.0
        };
        if cornered && self.chars[a].stype != b'a' {
            let push = if guarded {
                if air {
                    h.airguard_cornerpush
                } else {
                    h.guard_cornerpush
                }
            } else if air {
                h.air_cornerpush
            } else if dstype == b'l' {
                h.down_cornerpush
            } else {
                h.ground_cornerpush
            };
            self.chars[a].vel[0] += push;
        }

        // Estado del golpeado: HitOverride, estado propio del golpe o los comunes.
        let over = self.chars[d].hitoverride.iter().find(|o| o.time != 0 && o.attr != 0 && attr_match(h.attr, o.attr)).copied();
        if let Some(o) = over {
            self.change_state(d, o.stateno, Some(d));
        } else if let (Some(s), false) = (h.p2stateno, guarded) {
            let owner = if h.p2getp1state { a } else { d };
            self.change_state(d, s, Some(owner));
        } else {
            if h.forcestand && dstype == b'c' && !guarded {
                self.chars[d].stype = b's';
            }
            self.change_state(d, next_state, Some(d));
        }
        if let (Some(s), false, false) = (h.p1stateno, guarded, proj) {
            self.change_state(a, s, Some(a));
        }
        if self.chars[d].life == 0 && self.round.state == 2 && !self.chars[d].helper {
            self.round.ko_by_hit = true;
        }
    }

    // ---------------------------------------------------- empuje y límites

    fn push_players(&mut self) {
        let (a, b) = (0usize, 1usize);
        let (ca, cb) = (&self.chars[a], &self.chars[b]);
        if !ca.playerpush || !cb.playerpush || ca.bind.is_some() || cb.bind.is_some() {
            return;
        }
        // Sólo si se solapan en altura.
        let top = |c: &Char| c.pos[1] - c.def.height;
        if ca.pos[1] < top(cb) || cb.pos[1] < top(ca) {
            return;
        }
        let (left, right) = if ca.pos[0] <= cb.pos[0] { (a, b) } else { (b, a) };
        let (l, r) = (&self.chars[left], &self.chars[right]);
        let l_edge = l.pos[0] + l.width(l.facing > 0);
        let r_edge = r.pos[0] - r.width(r.facing < 0);
        let over = l_edge - r_edge;
        if over <= 0.0 {
            return;
        }
        let half = SCREEN_W as f32 / 2.0;
        let min_x = self.cam[0] - half + self.stage.screen_left;
        let max_x = self.cam[0] + half - self.stage.screen_right;
        // Se reparten el solape; el que está contra el borde no cede.
        let mut lx = l.pos[0] - over / 2.0;
        let mut rx = r.pos[0] + over / 2.0;
        if lx < min_x {
            rx += min_x - lx;
            lx = min_x;
        }
        if rx > max_x {
            lx -= rx - max_x;
            rx = max_x;
        }
        self.chars[left].pos[0] = lx;
        self.chars[right].pos[0] = rx;
    }

    fn clamp_players(&mut self) {
        let half = SCREEN_W as f32 / 2.0;
        for ch in self.chars.iter_mut().filter(|c| c.active) {
            // Un helper puede salirse del escenario.
            if !ch.helper {
                ch.pos[0] = ch.pos[0].clamp(self.stage.left_bound, self.stage.right_bound);
            }
            if ch.screenbound {
                let (back, front) = (ch.width_edge.1, ch.width_edge.0);
                let (wl, wr) = if ch.facing > 0 { (back, front) } else { (front, back) };
                let lo = self.cam[0] - half + self.stage.screen_left + wl;
                let hi = self.cam[0] + half - self.stage.screen_right - wr;
                if lo <= hi {
                    ch.pos[0] = ch.pos[0].clamp(lo, hi);
                }
            }
        }
    }

    fn update_camera(&mut self) {
        let half = SCREEN_W as f32 / 2.0;
        let st = &self.stage;
        let followers: Vec<&Char> = self.chars.iter().filter(|c| c.active && c.movecamera.0).collect();
        if !followers.is_empty() && !self.pause.on() {
            let min = followers.iter().map(|c| c.pos[0]).fold(f32::MAX, f32::min);
            let max = followers.iter().map(|c| c.pos[0]).fold(f32::MIN, f32::max);
            let mut x = self.cam[0];
            let (lo, hi) = (x - half + st.tension, x + half - st.tension);
            // La cámara cede lo justo para que nadie pase de la zona de tensión.
            if max - min > hi - lo {
                x = (min + max) / 2.0;
            } else if min < lo {
                x -= lo - min;
            } else if max > hi {
                x += max - hi;
            }
            self.cam[0] = x.clamp(st.bound_left, st.bound_right);
        }
        let high = self.chars.iter().filter(|c| c.active && c.movecamera.1).map(|c| c.pos[1]).fold(0.0f32, f32::min);
        let target = ((high + st.floortension).min(0.0) * st.verticalfollow).clamp(st.bound_high, st.bound_low);
        self.cam[1] += (target - self.cam[1]) * 0.5;
    }

    // --------------------------------------------------------------- tick

    /// Avanza un tick (1/60 s). `input[i]` son las teclas del jugador `i`
    /// (constantes `cmd::IN_*`).
    pub fn tick(&mut self, input: [u16; 2]) {
        self.sounds.clear();
        if self.finished {
            return;
        }
        self.round_update();
        if self.finished {
            return;
        }
        // Cámara lenta tras el K.O.: sólo avanza uno de cada dos ticks.
        if self.round.slow > 0 {
            self.round.slow -= 1;
            if self.round.slow % 2 == 1 {
                self.tick += 1;
                return;
            }
        }
        // La lista puede crecer por el camino: los helpers recién creados
        // empiezan a actuar en el tick siguiente.
        let mut c = 0;
        while c < self.chars.len() {
            let ch = &self.chars[c];
            let keys = if c < 2 {
                input[c]
            } else if ch.keyctrl {
                input[ch.root]
            } else {
                0
            };
            self.action(c, keys);
            c += 1;
        }
        for c in 0..self.chars.len() {
            self.update(c);
        }
        self.apply_binds();
        self.update_explods();
        self.update_projs();
        if !self.pause.on() {
            self.resolve_hits();
            self.resolve_proj_hits();
            self.push_players();
        }
        self.apply_binds();
        self.clamp_players();
        self.update_camera();
        self.clamp_players();
        if self.pause.fresh {
            self.pause.fresh = false;
        } else if self.pause.time > 0 {
            self.pause.time -= 1;
            if self.pause.movetime > 0 {
                self.pause.movetime -= 1;
            }
        }
        if self.shake.time > 0 {
            self.shake.time -= 1;
        }
        for ch in self.chars.iter_mut() {
            let target = ch.life as f32;
            if ch.life_lag < target || ch.movetype != b'h' {
                ch.life_lag += (target - ch.life_lag) * 0.15;
                if (ch.life_lag - target).abs() < 0.5 {
                    ch.life_lag = target;
                }
            }
        }
        self.tick += 1;
    }

    // ------------------------------------------------------------- dibujo

    fn shake_offset(&self) -> f32 {
        if self.shake.time <= 0 {
            return 0.0;
        }
        let t = (self.shake.total - self.shake.time) as f32;
        crate::eval::sinf((t * self.shake.freq + self.shake.phase) * 3.141_592_7 / 180.0) * self.shake.ampl
    }

    /// Posición en pantalla de un punto del escenario.
    pub fn to_screen(&self, pos: [f32; 2]) -> (f32, f32) {
        (pos[0] - self.cam[0] + SCREEN_W as f32 / 2.0, pos[1] - self.cam[1] + self.stage.zoffset + self.shake_offset())
    }

    fn char_pal(&self, c: usize, spr_pal: u16, frame: &mut Frame) -> PalRef {
        let ch = &self.chars[c];
        let sff = ch.def.sff;
        // Sólo la paleta principal del personaje cambia con `palno`.
        let idx = if spr_pal == 0 { ch.pal } else { spr_pal };
        // Un helper sin paleta propia lleva los efectos de su jugador.
        let fx = if ch.helper && !ch.own_pal { &self.chars[ch.root].palfx } else { &ch.palfx };
        if fx.active() {
            frame.dyn_pal(fx.apply(&self.sffs[sff as usize].pals[idx as usize]))
        } else {
            PalRef::Sff(sff, idx)
        }
    }

    fn draw_char(&self, c: usize, frame: &mut Frame) {
        let ch = &self.chars[c];
        if ch.has(AS_INVISIBLE) {
            return;
        }
        let Some(f) = ch.anim.frame() else { return };
        let sff = &self.sffs[ch.def.sff as usize];
        let Some(si) = sff.find(f.group, f.image) else { return };
        let spr = &sff.sprites[si as usize];
        let (mut sx, mut sy) = self.to_screen(ch.pos);
        // Temblor del golpeado.
        if ch.movetype == b'h' && ch.ghv.hitshake > 0 && self.tick % 2 == 0 {
            sx += if (self.tick / 2) % 2 == 0 { 2.0 } else { -2.0 };
        }
        let face = ch.facing as f32;
        sx += face * (f.x as f32 + ch.def.draw_offset.0) * ch.scale.0;
        sy += (f.y as f32 + ch.def.draw_offset.1) * ch.scale.1;
        let scale_x = face * ch.scale.0 * if f.hflip { -1.0 } else { 1.0 };
        let scale_y = ch.scale.1 * if f.vflip { -1.0 } else { 1.0 };
        ch.afterimage.draw(self, c, frame);
        let pal = self.char_pal(c, spr.pal, frame);
        frame.sprite(SpriteCmd { sff: ch.def.sff, spr: si, x: sx, y: sy, sx: scale_x, sy: scale_y, pal, trans: f.trans, clip: None, src: None });
    }

    fn draw_shadow(&self, c: usize, frame: &mut Frame) {
        let ch = &self.chars[c];
        if ch.has(AS_INVISIBLE | AS_NOSHADOW) {
            return;
        }
        let Some(f) = ch.anim.frame() else { return };
        let sff = &self.sffs[ch.def.sff as usize];
        let Some(si) = sff.find(f.group, f.image) else { return };
        let spr = &sff.sprites[si as usize];
        let st = &self.stage;
        let face = ch.facing as f32;
        let (sx, floor) = self.to_screen([ch.pos[0], 0.0]);
        let x = sx + face * f.x as f32 * ch.scale.0;
        let scale_x = face * ch.scale.0 * if f.hflip { -1.0 } else { 1.0 };
        let flip_y = if f.vflip { -1.0 } else { 1.0 };
        // Reflejo en el suelo: el sprite invertido, translúcido.
        if st.reflection > 0 {
            let pal = self.char_pal(c, spr.pal, frame);
            let y = floor - (ch.pos[1] + f.y as f32 * ch.scale.1);
            frame.sprite(SpriteCmd {
                sff: ch.def.sff,
                spr: si,
                x,
                y,
                sx: scale_x,
                sy: -ch.scale.1 * flip_y,
                pal,
                trans: Trans::Add { src: (st.reflection / 2).clamp(0, 256) as u16, dst: 256 },
                clip: None,
                src: None,
            });
        }
        if st.shadow_intensity > 0 && st.shadow_yscale != 0.0 {
            let i = st.shadow_intensity.clamp(0, 256) as u16;
            let (r, g, b) = st.shadow_color;
            let col = r as u32 | (g as u32) << 8 | (b as u32) << 16 | 0xff00_0000;
            let mut p = [col; 256];
            p[0] = 0;
            let pal = frame.dyn_pal(p);
            let ys = -st.shadow_yscale * ch.scale.1;
            // La sombra se aplasta contra el suelo aunque el personaje salte.
            let y = floor + ch.def.shadowoffset + (ch.pos[1] + f.y as f32 * ch.scale.1) * ys;
            frame.sprite(SpriteCmd {
                sff: ch.def.sff,
                spr: si,
                x,
                y,
                sx: scale_x,
                sy: ys * flip_y,
                pal,
                trans: Trans::Add { src: i, dst: 256 - i },
                clip: None,
                src: None,
            });
        }
    }

    fn draw_clsn(&self, frame: &mut Frame) {
        for p in &self.projs {
            for b in p.boxes() {
                let (x0, y0) = self.to_screen([b[0], b[1]]);
                let (x1, y1) = self.to_screen([b[2], b[3]]);
                frame.rect(x0 as i32, y0 as i32, (x1 - x0) as i32 + 1, (y1 - y0) as i32 + 1, 0xff40_40ff, Trans::Add { src: 96, dst: 256 });
            }
        }
        for ch in self.chars.iter().filter(|c| c.active) {
            for (attack, color) in [(false, 0xffff_4040u32), (true, 0xff40_40ff)] {
                for b in ch.boxes(attack) {
                    let (x0, y0) = self.to_screen([b[0], b[1]]);
                    let (x1, y1) = self.to_screen([b[2], b[3]]);
                    frame.rect(x0 as i32, y0 as i32, (x1 - x0) as i32 + 1, (y1 - y0) as i32 + 1, color, Trans::Add { src: 96, dst: 256 });
                }
            }
            let (x, y) = self.to_screen(ch.pos);
            frame.rect(x as i32 - 2, y as i32, 5, 1, 0xffff_ffff, Trans::None);
            frame.rect(x as i32, y as i32 - 2, 1, 5, 0xffff_ffff, Trans::None);
        }
    }

    /// Genera la lista de dibujo y de sonidos del tick actual.
    pub fn draw(&self, frame: &mut Frame) {
        frame.reset();
        frame.clear = 0;
        frame.sounds.extend(self.sounds.iter().copied());
        let shake = self.shake_offset();
        self.stage.draw(frame, &self.sffs, self.cam, shake, self.tick, 0);
        if self.pause.on() && self.pause.darken {
            frame.rect(0, 0, 320, 240, 0xff00_0000, Trans::Add { src: 128, dst: 128 });
        }
        let mut order: Vec<usize> = (0..self.chars.len()).filter(|&c| self.chars[c].active).collect();
        for &c in &order {
            self.draw_shadow(c, frame);
        }
        self.draw_explods(frame, false);
        self.draw_projs(frame, false);
        order.sort_by_key(|&c| self.chars[c].sprpriority);
        for c in order {
            self.draw_char(c, frame);
        }
        self.draw_projs(frame, true);
        self.draw_explods(frame, true);
        self.stage.draw(frame, &self.sffs, self.cam, shake, self.tick, 1);
        if self.show_clsn {
            self.draw_clsn(frame);
        }
        self.draw_hud(frame);
    }

    /// Resumen del estado, para comparar ejecuciones entre plataformas.
    pub fn summary(&self) -> String {
        let c = |i: usize| {
            let ch = &self.chars[i];
            format!("{}:{} a{} L{} P{} x{} y{}", ch.stateno, ch.time, ch.anim.no, ch.life, ch.power, ch.pos[0] as i32, ch.pos[1] as i32)
        };
        let mut s = format!("t{} r{}/{} [{}] [{}]", self.tick, self.round.no, self.round.state, c(0), c(1));
        // Helpers y proyectiles en juego, sólo si los hay.
        let helpers = self.chars.iter().filter(|c| c.active && c.helper).count();
        if helpers + self.projs.len() > 0 {
            s.push_str(&format!(" h{} p{}", helpers, self.projs.len()));
        }
        s
    }
}

/// Lee `hitflag` / `guardflag`.
pub fn parse_hitflag(s: &str) -> u32 {
    let mut f = 0;
    for ch in s.to_ascii_lowercase().chars() {
        f |= match ch {
            'h' => HF_H,
            'l' => HF_L,
            'm' => HF_H | HF_L,
            'a' => HF_A,
            'f' => HF_F,
            'd' => HF_D,
            '+' => HF_PLUS,
            '-' => HF_MINUS,
            _ => 0,
        };
    }
    f
}

pub fn state_attr(stype: u8) -> u32 {
    match stype {
        b'c' => AT_C,
        b'a' => AT_A,
        _ => AT_S,
    }
}

/// Evalúa una expresión opcional con valor por defecto.
impl Game {
    pub fn eval_or(&self, c: usize, e: Option<&Expr>, default: f32) -> f32 {
        e.map_or(default, |e| self.eval(c, e).f())
    }
}
