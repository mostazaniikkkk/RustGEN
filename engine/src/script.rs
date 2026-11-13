//! Guion de teclas para pruebas: reproduce la misma entrada en cualquier
//! plataforma.
//!
//! Cada línea es `tick jugador teclas duración`, por ejemplo `300 1 Rx 2`
//! (en el tick 300 el jugador 1 mantiene derecha y X durante 2 ticks).
//! Teclas: `U D L R a b c x y z s`.

use crate::cmd::*;
use crate::ini;

pub struct Script {
    /// (tick inicial, tick final, jugador, teclas)
    events: Vec<(i32, i32, usize, u16)>,
}

pub fn parse_keys(s: &str) -> u16 {
    s.chars().fold(0, |k, ch| {
        k | match ch {
            'U' => IN_UP,
            'D' => IN_DOWN,
            'L' => IN_LEFT,
            'R' => IN_RIGHT,
            'a' => IN_A,
            'b' => IN_B,
            'c' => IN_C,
            'x' => IN_X,
            'y' => IN_Y,
            'z' => IN_Z,
            's' => IN_S,
            _ => 0,
        }
    })
}

impl Script {
    pub fn parse(text: &str) -> Script {
        let mut events = Vec::new();
        for line in text.lines() {
            let p: Vec<&str> = ini::clean(line).split_whitespace().collect();
            if p.len() < 3 {
                continue;
            }
            let (Ok(t), Ok(player)) = (p[0].parse::<i32>(), p[1].parse::<usize>()) else { continue };
            let dur = p.get(3).and_then(|d| d.parse::<i32>().ok()).unwrap_or(1);
            events.push((t, t + dur, player.clamp(1, 2) - 1, parse_keys(p[2])));
        }
        Script { events }
    }

    pub fn load(path: &str) -> std::io::Result<Script> {
        Ok(Script::parse(&ini::read_text(path)?))
    }

    pub fn keys(&self, tick: i32) -> [u16; 2] {
        let mut k = [0u16; 2];
        for &(a, b, p, keys) in &self.events {
            if tick >= a && tick < b {
                k[p] |= keys;
            }
        }
        k
    }

    pub fn end(&self) -> i32 {
        self.events.iter().map(|e| e.1).max().unwrap_or(0)
    }
}
