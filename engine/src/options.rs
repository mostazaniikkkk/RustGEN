//! Opciones del juego (el menú OPTIONS): dificultad, vida, tiempo de ronda,
//! velocidad, volumen y los botones de cada jugador.
//!
//! El motor trabaja con "funciones" (arriba, abajo, a, b, c…: los bits
//! `cmd::IN_*`). La plataforma entrega los botones físicos de cada mando, un
//! bit por botón, y aquí se traducen según lo que haya elegido el jugador.
//! Qué botones hay y cómo se llaman lo dice el [`PadLayout`] de la plataforma.

use crate::cmd::*;
use crate::ini;

/// Funciones que se pueden asignar, en el orden de los bits `IN_*`.
pub const FUNCS: usize = 11;
pub const FUNC_NAMES: [&str; FUNCS] = ["Up", "Down", "Left", "Right", "A", "B", "C", "X", "Y", "Z", "Start"];
const FUNC_KEYS: [&str; FUNCS] = ["up", "down", "left", "right", "a", "b", "c", "x", "y", "z", "start"];
const FUNC_BITS: [u16; FUNCS] = [IN_UP, IN_DOWN, IN_LEFT, IN_RIGHT, IN_A, IN_B, IN_C, IN_X, IN_Y, IN_Z, IN_S];

/// Los botones de un mando: nombre de cada uno de sus 16 bits (vacío = no
/// existe), qué botón hace cada función de fábrica, y cuál es el botón de
/// sistema (SELECT), que no se puede cambiar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PadLayout {
    pub names: [&'static str; 16],
    pub default: [u8; FUNCS],
    pub select: u8,
}

impl PadLayout {
    /// Mando genérico: cada botón es directamente una función del motor.
    /// Es el de las pruebas y el de la herramienta de PC.
    pub const GENERIC: PadLayout = PadLayout {
        names: ["Up", "Down", "Left", "Right", "A", "B", "C", "X", "Y", "Z", "Start", "Select", "", "", "", ""],
        default: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        select: 11,
    };

    /// Mando de PlayStation 2, con los bits en el orden en que los da PADMAN.
    pub const PS2: PadLayout = PadLayout {
        names: [
            "Select", "L3", "R3", "Start", "Up", "Right", "Down", "Left", "L2", "R2", "L1", "R1", "Triangle", "Circle", "Cross", "Square",
        ],
        // Cruceta; equis, círculo y R2 = a, b, c; cuadrado, triángulo y R1 =
        // x, y, z; START.
        default: [4, 6, 7, 5, 14, 13, 9, 15, 12, 11, 3],
        select: 0,
    };

    /// Traduce botones a funciones con una asignación dada.
    pub fn map(&self, keys: &[u8; FUNCS], raw: u16) -> u16 {
        let mut out = if raw & (1 << self.select) != 0 { IN_SELECT } else { 0 };
        for (f, &button) in keys.iter().enumerate() {
            if raw & (1u16 << (button & 15)) != 0 {
                out |= FUNC_BITS[f];
            }
        }
        out
    }

    /// Lo contrario, con la asignación de fábrica: qué botones habría que
    /// pulsar para obtener esas funciones (para los guiones de teclas).
    pub fn raw_from_keys(&self, keys: u16) -> u16 {
        let mut raw = if keys & IN_SELECT != 0 { 1 << self.select } else { 0 };
        for (f, &button) in self.default.iter().enumerate() {
            if keys & FUNC_BITS[f] != 0 {
                raw |= 1 << button;
            }
        }
        raw
    }

    /// Nombre de un botón.
    pub fn name(&self, button: u8) -> &'static str {
        self.names[(button & 15) as usize]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// 1 (fácil) a 8 (difícil).
    pub difficulty: i32,
    /// Vida inicial, en tanto por ciento: 30 a 200.
    pub life: i32,
    /// Segundos del reloj de la ronda; 0 = sin límite.
    pub time: i32,
    /// -9 (lento) a 9 (rápido); 0 = normal. Cada paso es un 5 %.
    pub speed: i32,
    /// 0 a 100.
    pub volume: i32,
    /// Botón de cada función, por jugador.
    pub keys: [[u8; FUNCS]; 2],
}

/// Tiempos de ronda que ofrece el menú, en orden.
const TIMES: [i32; 9] = [30, 40, 50, 60, 70, 80, 90, 99, 0];

impl Options {
    pub fn new(layout: &PadLayout) -> Options {
        Options { difficulty: 4, life: 100, time: 99, speed: 0, volume: 100, keys: [layout.default; 2] }
    }

    /// Ticks de pelea por cada 60 frames de vídeo.
    pub fn ticks_per_second(&self) -> i32 {
        60 + self.speed * 3
    }

    /// Cambia el valor de la opción `item` (0 = dificultad, 1 = vida,
    /// 2 = tiempo, 3 = velocidad, 4 = volumen) un paso arriba o abajo.
    /// Devuelve si cambió.
    pub fn step(&mut self, item: usize, dir: i32) -> bool {
        let before = self.clone();
        match item {
            0 => self.difficulty = (self.difficulty + dir).clamp(1, 8),
            1 => self.life = (self.life + dir * 10).clamp(30, 200),
            2 => {
                let at = TIMES.iter().position(|&t| t == self.time).unwrap_or(7) as i32;
                self.time = TIMES[(at + dir).clamp(0, TIMES.len() as i32 - 1) as usize];
            }
            3 => self.speed = (self.speed + dir).clamp(-9, 9),
            4 => self.volume = (self.volume + dir * 10).clamp(0, 100),
            _ => {}
        }
        *self != before
    }

    /// Texto del valor de la opción `item`, como lo enseña el menú.
    pub fn value_text(&self, item: usize) -> String {
        match item {
            0 => {
                let word = match self.difficulty {
                    1..=2 => "Easy",
                    3..=5 => "Medium",
                    _ => "Hard",
                };
                format!("{word} {}", self.difficulty)
            }
            1 => format!("{}%", self.life),
            2 if self.time == 0 => "None".to_string(),
            2 => self.time.to_string(),
            3 => match self.speed {
                0 => "Normal".to_string(),
                s if s > 0 => format!("Fast {s}"),
                s => format!("Slow {}", -s),
            },
            4 => format!("{}%", self.volume),
            _ => String::new(),
        }
    }

    /// Lee opciones de un texto `clave = valor` encima de las actuales: lo
    /// que falte o no valga se queda como estaba.
    pub fn apply_text(&mut self, text: &str) {
        for line in text.lines() {
            let line = ini::clean(line);
            let Some((key, value)) = line.split_once('=') else { continue };
            let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
            let num = ini::num(value).map(|v| v as i32);
            match (key.as_str(), num) {
                ("difficulty", Some(v)) => self.difficulty = v.clamp(1, 8),
                ("life", Some(v)) => self.life = (v / 10 * 10).clamp(30, 200),
                ("time", Some(v)) => self.time = if TIMES.contains(&v) { v } else { 99 },
                ("speed", Some(v)) => self.speed = v.clamp(-9, 9),
                ("volume", Some(v)) => self.volume = v.clamp(0, 100),
                _ => {
                    // `p1.x = 15`: botón de una función de un jugador.
                    let Some((player, func)) = key.split_once('.') else { continue };
                    let p = match player {
                        "p1" => 0,
                        "p2" => 1,
                        _ => continue,
                    };
                    if let (Some(f), Some(v)) = (FUNC_KEYS.iter().position(|k| *k == func), num) {
                        if (0..16).contains(&v) {
                            self.keys[p][f] = v as u8;
                        }
                    }
                }
            }
        }
    }

    /// Las opciones como texto, para guardarlas.
    pub fn to_text(&self) -> String {
        let mut s = format!(
            "difficulty = {}\nlife = {}\ntime = {}\nspeed = {}\nvolume = {}\n",
            self.difficulty, self.life, self.time, self.speed, self.volume
        );
        for p in 0..2 {
            for f in 0..FUNCS {
                s.push_str(&format!("p{}.{} = {}\n", p + 1, FUNC_KEYS[f], self.keys[p][f]));
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip_and_bad_values() {
        let mut o = Options::new(&PadLayout::PS2);
        o.difficulty = 7;
        o.life = 150;
        o.time = 0;
        o.speed = -3;
        o.volume = 40;
        o.keys[1][4] = 10;
        let mut back = Options::new(&PadLayout::PS2);
        back.apply_text(&o.to_text());
        assert_eq!(back, o);
        // Valores fuera de rango, claves desconocidas y basura no rompen nada.
        back.apply_text("difficulty = 99\nlife = 5\ntime = 17\nspeed = -40\nvolume = 300\np3.x = 2\np1.x = 99\nhola\n; nada\np2.start = 8");
        assert_eq!((back.difficulty, back.life, back.time, back.speed, back.volume), (8, 30, 99, -9, 100));
        assert_eq!(back.keys[0][7], o.keys[0][7]);
        assert_eq!(back.keys[1][10], 8);
    }

    #[test]
    fn layouts_map_buttons_to_functions() {
        let g = PadLayout::GENERIC;
        // El mando genérico es la identidad.
        for keys in [0, IN_UP | IN_X, IN_S | IN_SELECT, IN_DOWN | IN_RIGHT | IN_C | IN_Z] {
            assert_eq!(g.map(&g.default, keys), keys);
            assert_eq!(g.raw_from_keys(keys), keys);
        }
        let p = PadLayout::PS2;
        assert_eq!(p.map(&p.default, 0x8000 | 0x0010), IN_X | IN_UP);
        assert_eq!(p.map(&p.default, 0x0001 | 0x0008), IN_SELECT | IN_S);
        assert_eq!(p.map(&p.default, p.raw_from_keys(IN_A | IN_LEFT | IN_Z)), IN_A | IN_LEFT | IN_Z);
        // Cambiar un botón: L1 pasa a ser "a".
        let mut keys = p.default;
        keys[4] = 10;
        assert_eq!(p.map(&keys, 1 << 10), IN_A);
        assert_eq!(p.map(&keys, 1 << 14), 0);
        assert_eq!(p.name(10), "L1");
    }

    #[test]
    fn steps_stay_in_range() {
        let mut o = Options::new(&PadLayout::GENERIC);
        for item in 0..5 {
            for _ in 0..40 {
                o.step(item, 1);
            }
        }
        assert_eq!((o.difficulty, o.life, o.time, o.speed, o.volume), (8, 200, 0, 9, 100));
        assert!(!o.step(0, 1));
        for item in 0..5 {
            for _ in 0..40 {
                o.step(item, -1);
            }
        }
        assert_eq!((o.difficulty, o.life, o.time, o.speed, o.volume), (1, 30, 30, -9, 0));
        assert_eq!(o.value_text(0), "Easy 1");
        assert_eq!(o.value_text(3), "Slow 9");
        assert_eq!(o.ticks_per_second(), 33);
    }
}
