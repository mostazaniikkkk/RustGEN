//! Comandos (archivo CMD): secuencias de direcciones y botones, y su
//! reconocimiento sobre el historial de entradas del jugador.

use crate::ini::{self, Section};
use std::collections::HashMap;

// Teclas tal como las entrega la plataforma (izquierda/derecha absolutas).
pub const IN_UP: u16 = 1;
pub const IN_DOWN: u16 = 2;
pub const IN_LEFT: u16 = 4;
pub const IN_RIGHT: u16 = 8;
pub const IN_A: u16 = 16;
pub const IN_B: u16 = 32;
pub const IN_C: u16 = 64;
pub const IN_X: u16 = 128;
pub const IN_Y: u16 = 256;
pub const IN_Z: u16 = 512;
pub const IN_S: u16 = 1024;
/// Botón de sistema (SELECT): no forma parte de ningún comando.
pub const IN_SELECT: u16 = 2048;
/// Cualquier botón de ataque o START.
pub const IN_ANY: u16 = IN_A | IN_B | IN_C | IN_X | IN_Y | IN_Z | IN_S;

// Dentro del motor, las direcciones son relativas: atrás / adelante.
const U: u16 = 1;
const D: u16 = 2;
const B: u16 = 4;
const F: u16 = 8;
const DIRS: u16 = 15;

#[derive(Clone, Debug)]
struct Key {
    /// Bits que deben estar pulsados.
    bits: u16,
    /// Dirección: debe coincidir exacta, salvo con `$`.
    dir: bool,
    four_way: bool,
    hold: bool,
    release: bool,
    charge: i32,
}

#[derive(Clone, Debug)]
struct Elem {
    keys: Vec<Key>,
}

#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    elems: Vec<Elem>,
    time: i32,
    buffer: i32,
}

#[derive(Default)]
pub struct CommandSet {
    pub cmds: Vec<Command>,
    pub by_name: HashMap<String, Vec<usize>>,
}

fn parse_key(s: &str) -> Option<Key> {
    let mut k = Key { bits: 0, dir: false, four_way: false, hold: false, release: false, charge: 0 };
    let mut rest = s.trim();
    loop {
        if let Some(r) = rest.strip_prefix('/') {
            k.hold = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix('~') {
            k.release = true;
            let digits = r.bytes().take_while(u8::is_ascii_digit).count();
            k.charge = r[..digits].parse().unwrap_or(0);
            rest = &r[digits..];
        } else if let Some(r) = rest.strip_prefix('$') {
            k.four_way = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix('>') {
            rest = r;
        } else {
            break;
        }
        rest = rest.trim_start();
    }
    let (bits, dir) = match rest {
        "U" => (U, true),
        "D" => (D, true),
        "B" => (B, true),
        "F" => (F, true),
        "UB" => (U | B, true),
        "UF" => (U | F, true),
        "DB" => (D | B, true),
        "DF" => (D | F, true),
        "a" => (IN_A, false),
        "b" => (IN_B, false),
        "c" => (IN_C, false),
        "x" => (IN_X, false),
        "y" => (IN_Y, false),
        "z" => (IN_Z, false),
        "s" => (IN_S, false),
        _ => return None,
    };
    k.bits = bits;
    k.dir = dir;
    Some(k)
}

impl Command {
    /// ¿Es una secuencia (un especial) y no una tecla suelta?
    pub fn is_motion(&self) -> bool {
        self.elems.len() >= 3
    }
}

impl CommandSet {
    pub fn parse(secs: &[Section]) -> CommandSet {
        let mut set = CommandSet::default();
        let (mut def_time, mut def_buffer) = (15, 1);
        for s in secs {
            if s.name == "defaults" {
                def_time = s.i32("command.time", 15);
                def_buffer = s.i32("command.buffer.time", 1);
            }
            if s.name != "command" {
                continue;
            }
            let (Some(name), Some(text)) = (s.str("name"), s.get("command")) else { continue };
            let mut elems = Vec::new();
            let mut ok = true;
            for part in text.split(',') {
                let keys: Option<Vec<Key>> = part.split('+').map(parse_key).collect();
                match keys {
                    Some(keys) => elems.push(Elem { keys }),
                    None => ok = false,
                }
            }
            if !ok || elems.is_empty() {
                continue;
            }
            let name = name.to_ascii_lowercase();
            set.by_name.entry(name.clone()).or_default().push(set.cmds.len());
            set.cmds.push(Command {
                name,
                elems,
                time: s.i32("time", def_time).max(1),
                buffer: s.i32("buffer.time", def_buffer).clamp(1, 30),
            });
        }
        set
    }

    pub fn load(path: &str) -> std::io::Result<(CommandSet, Vec<Section>)> {
        let secs = ini::load(path)?;
        Ok((CommandSet::parse(&secs), secs))
    }
}

const HIST: usize = 64;

/// Historial de entradas y comandos activos de un jugador.
pub struct CommandState {
    hist: [u16; HIST],
    now: usize,
    /// Ticks que le quedan activo a cada comando.
    active: Vec<u8>,
}

impl CommandState {
    pub fn new(set: &CommandSet) -> CommandState {
        CommandState { hist: [0; HIST], now: 0, active: vec![0; set.cmds.len()] }
    }

    /// Convierte las teclas de la plataforma a relativas según hacia dónde
    /// mira el jugador (`facing` = 1 mira a la derecha).
    pub fn relative(keys: u16, facing: i32) -> u16 {
        let mut k = keys & !(IN_LEFT | IN_RIGHT);
        let (back, fwd) = if facing >= 0 { (IN_LEFT, IN_RIGHT) } else { (IN_RIGHT, IN_LEFT) };
        if keys & back != 0 {
            k |= B;
        }
        if keys & fwd != 0 {
            k |= F;
        }
        // Direcciones opuestas a la vez se anulan.
        if k & (B | F) == (B | F) {
            k &= !(B | F);
        }
        if k & (U | D) == (U | D) {
            k &= !(U | D);
        }
        k
    }

    /// Estado de teclas hace `ago` ticks.
    fn at(&self, ago: i32) -> u16 {
        if ago < 0 || ago as usize >= HIST {
            return 0;
        }
        self.hist[(self.now + HIST - ago as usize) % HIST]
    }

    pub fn current(&self) -> u16 {
        self.at(0)
    }

    fn down(&self, k: &Key, ago: i32) -> bool {
        let s = self.at(ago);
        if k.dir && !k.four_way {
            s & DIRS == k.bits
        } else {
            s & k.bits == k.bits
        }
    }

    fn elem_at(&self, e: &Elem, ago: i32) -> bool {
        let mut any_new = false;
        let mut needs_new = false;
        for k in &e.keys {
            if k.hold {
                if !self.down(k, ago) {
                    return false;
                }
            } else if k.release {
                if self.down(k, ago) || !self.down(k, ago + 1) {
                    return false;
                }
                if (2..=k.charge).any(|t| !self.down(k, ago + t)) {
                    return false;
                }
            } else {
                needs_new = true;
                if !self.down(k, ago) {
                    return false;
                }
                any_new |= !self.down(k, ago + 1);
            }
        }
        !needs_new || any_new
    }

    fn matches(&self, c: &Command) -> bool {
        let last = c.elems.len() - 1;
        if !self.elem_at(&c.elems[last], 0) {
            return false;
        }
        let mut ago = 0;
        for i in (0..last).rev() {
            let (prev, cur) = (&c.elems[i], &c.elems[i + 1]);
            // Mismo tick sólo para "soltar y luego…" o dirección seguida de botón.
            let same_tick = prev.keys.iter().all(|k| k.release || k.hold)
                || (prev.keys.iter().all(|k| k.dir) && cur.keys.iter().all(|k| !k.dir));
            let mut t = if same_tick { ago } else { ago + 1 };
            loop {
                if t > c.time {
                    return false;
                }
                if self.elem_at(prev, t) {
                    break;
                }
                t += 1;
            }
            ago = t;
        }
        true
    }

    /// Registra las teclas (ya relativas) de este tick y actualiza los comandos.
    pub fn update(&mut self, set: &CommandSet, keys: u16) {
        self.now = (self.now + 1) % HIST;
        self.hist[self.now] = keys;
        for (i, c) in set.cmds.iter().enumerate() {
            if self.matches(c) {
                self.active[i] = c.buffer as u8;
            } else if self.active[i] > 0 {
                self.active[i] -= 1;
            }
        }
    }

    pub fn is_active(&self, set: &CommandSet, name: &str) -> bool {
        set.by_name.get(name).is_some_and(|v| v.iter().any(|&i| self.active[i] > 0))
    }

    /// Activa un comando sin entrada (lo usa la IA).
    pub fn force(&mut self, set: &CommandSet, name: &str) {
        if let Some(v) = set.by_name.get(name) {
            for &i in v {
                self.active[i] = self.active[i].max(1);
            }
        }
    }

    pub fn clear(&mut self) {
        self.hist = [0; HIST];
        self.active.iter_mut().for_each(|a| *a = 0);
    }
}
