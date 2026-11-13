//! Animaciones (formato AIR) y su reproducción.

use crate::gfx::Trans;
use crate::ini;
use std::collections::HashMap;
use std::rc::Rc;

pub type Clsn = Rc<Vec<[i32; 4]>>;

#[derive(Clone, Debug)]
pub struct AFrame {
    pub group: i32,
    pub image: i32,
    pub x: i32,
    pub y: i32,
    /// Duración en ticks; -1 = para siempre.
    pub time: i32,
    pub hflip: bool,
    pub vflip: bool,
    pub trans: Trans,
    pub clsn1: Clsn,
    pub clsn2: Clsn,
}

#[derive(Debug, Default)]
pub struct Anim {
    pub frames: Vec<AFrame>,
    pub loopstart: usize,
    /// Tick en que empieza cada elemento.
    pub starts: Vec<i32>,
    /// Suma de las duraciones finitas.
    pub total: i32,
    pub infinite: bool,
}

pub type AnimTable = HashMap<i32, Rc<Anim>>;

impl Anim {
    /// Cuadro que toca en el tick `t`, sin llevar estado. Si no se repite,
    /// devuelve `None` al terminar.
    pub fn frame_at(&self, t: i32, looping: bool) -> Option<&AFrame> {
        if self.frames.is_empty() || t < 0 {
            return None;
        }
        let mut t = t;
        if !self.infinite && self.total > 0 && t >= self.total {
            if !looping {
                return None;
            }
            let start = self.starts[self.loopstart];
            let span = (self.total - start).max(1);
            t = start + (t - self.total) % span;
        }
        let mut cur = &self.frames[0];
        for (i, f) in self.frames.iter().enumerate() {
            if self.starts[i] > t {
                break;
            }
            cur = f;
        }
        Some(cur)
    }
}

pub fn parse_trans(s: &str) -> Trans {
    let s = s.trim().to_ascii_lowercase();
    if s == "a" {
        Trans::ADD
    } else if s == "a1" {
        Trans::ADD1
    } else if s.starts_with('s') {
        Trans::Sub
    } else if let Some(rest) = s.strip_prefix("as") {
        let mut it = rest.split('d');
        let a = it.next().and_then(ini::num).unwrap_or(256.0) as u16;
        let d = it.next().and_then(ini::num).unwrap_or(256.0) as u16;
        Trans::Add { src: a.min(256), dst: d.min(256) }
    } else {
        Trans::None
    }
}

fn finish(table: &mut AnimTable, no: i32, mut a: Anim) {
    let mut t = 0;
    for f in &a.frames {
        a.starts.push(t);
        if f.time < 0 {
            a.infinite = true;
        } else {
            t += f.time;
        }
    }
    a.total = t;
    if a.loopstart >= a.frames.len() {
        a.loopstart = 0;
    }
    // Si la acción se repite, vale la primera.
    table.entry(no).or_insert_with(|| Rc::new(a));
}

/// Extrae todas las secciones `[Begin Action n]` del texto.
pub fn parse(text: &str) -> AnimTable {
    let mut table = AnimTable::new();
    let empty: Clsn = Rc::new(Vec::new());
    let mut cur: Option<(i32, Anim)> = None;
    let mut def: [Clsn; 2] = [empty.clone(), empty.clone()];
    let mut next: [Option<Clsn>; 2] = [None, None];
    // Cajas que se están leyendo: (clase 0/1, por defecto, lista).
    let mut reading: Option<(usize, bool, Vec<[i32; 4]>)> = None;

    macro_rules! close_boxes {
        () => {
            if let Some((k, is_def, boxes)) = reading.take() {
                let rc: Clsn = Rc::new(boxes);
                if is_def {
                    def[k] = rc;
                } else {
                    next[k] = Some(rc);
                }
            }
        };
    }

    for raw in text.lines() {
        let line = ini::clean(raw);
        if line.is_empty() {
            continue;
        }
        let low = line.to_ascii_lowercase();
        if low.starts_with('[') {
            close_boxes!();
            if let Some((no, a)) = cur.take() {
                finish(&mut table, no, a);
            }
            let name = low.trim_matches(|c| c == '[' || c == ']').trim().to_string();
            if let Some(n) = name.strip_prefix("begin action") {
                if let Some(no) = ini::num(n) {
                    cur = Some((no as i32, Anim::default()));
                    def = [empty.clone(), empty.clone()];
                    next = [None, None];
                }
            }
            continue;
        }
        let Some((_, anim)) = cur.as_mut() else { continue };
        if low.starts_with("clsn") {
            let k = if low.as_bytes().get(4) == Some(&b'1') { 0 } else { 1 };
            if let Some(eq) = low.find('=') {
                // Clsn2[0] = x1, y1, x2, y2
                let v = ini::nums(&low[eq + 1..], 0.0);
                if v.len() >= 4 {
                    if let Some(r) = reading.as_mut() {
                        let (x1, y1, x2, y2) = (v[0] as i32, v[1] as i32, v[2] as i32, v[3] as i32);
                        r.2.push([x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2)]);
                    }
                }
            } else {
                close_boxes!();
                reading = Some((k, low.contains("default"), Vec::new()));
            }
            continue;
        }
        if low.starts_with("loopstart") {
            anim.loopstart = anim.frames.len();
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 5 || ini::num(parts[0]).is_none() {
            continue;
        }
        close_boxes!();
        let n = |i: usize| parts.get(i).and_then(|s| ini::num(s)).unwrap_or(0.0) as i32;
        let flip = parts.get(5).map(|s| s.trim().to_ascii_lowercase()).unwrap_or_default();
        anim.frames.push(AFrame {
            group: n(0),
            image: n(1),
            x: n(2),
            y: n(3),
            time: n(4).max(-1),
            hflip: flip.contains('h'),
            vflip: flip.contains('v'),
            trans: parts.get(6).map_or(Trans::None, |s| parse_trans(s)),
            clsn1: next[0].take().unwrap_or_else(|| def[0].clone()),
            clsn2: next[1].take().unwrap_or_else(|| def[1].clone()),
        });
    }
    if let Some((no, a)) = cur.take() {
        finish(&mut table, no, a);
    }
    table
}

pub fn load(path: &str) -> std::io::Result<AnimTable> {
    Ok(parse(&ini::read_text(path)?))
}

/// Estado de reproducción de una animación.
#[derive(Clone, Default)]
pub struct AnimState {
    pub anim: Option<Rc<Anim>>,
    pub no: i32,
    pub elem: usize,
    pub elem_time: i32,
    /// La animación acaba de dar la vuelta en el último `update`.
    pub looped: bool,
    /// Ticks transcurridos desde que se puso la animación.
    pub ticks: i32,
}

impl AnimState {
    pub fn set(&mut self, anim: Option<Rc<Anim>>, no: i32) {
        *self = AnimState { anim, no, ..Default::default() };
    }

    /// Salta al elemento `n` (empieza en 1).
    pub fn set_elem(&mut self, n: i32) {
        if let Some(a) = &self.anim {
            self.elem = ((n - 1).max(0) as usize).min(a.frames.len().saturating_sub(1));
            self.elem_time = 0;
            self.looped = false;
        }
    }

    pub fn frame(&self) -> Option<&AFrame> {
        self.anim.as_ref().and_then(|a| a.frames.get(self.elem))
    }

    pub fn update(&mut self) {
        let Some(a) = self.anim.clone() else { return };
        if a.frames.is_empty() {
            return;
        }
        self.looped = false;
        self.ticks += 1;
        self.elem_time += 1;
        for _ in 0..=a.frames.len() {
            let t = a.frames[self.elem].time;
            if t < 0 || self.elem_time < t {
                break;
            }
            self.elem_time -= t;
            self.elem += 1;
            if self.elem >= a.frames.len() {
                self.elem = a.loopstart;
                self.looped = true;
                self.elem_time = 0;
            }
        }
    }

    fn pos(&self) -> i32 {
        self.anim.as_ref().map_or(0, |a| a.starts.get(self.elem).copied().unwrap_or(0) + self.elem_time)
    }

    /// Trigger `AnimTime`: 0 justo al terminar, negativo mientras se reproduce.
    pub fn anim_time(&self) -> i32 {
        let Some(a) = &self.anim else { return 0 };
        if a.infinite {
            (self.pos() - a.total - 1).min(-1)
        } else if self.looped {
            0
        } else {
            self.pos() - a.total
        }
    }

    /// Elemento actual, empezando en 1.
    pub fn elem_no(&self) -> i32 {
        self.elem as i32 + 1
    }

    /// Trigger `AnimElemTime(n)`: ticks desde que empezó el elemento `n`.
    pub fn elem_time_of(&self, n: i32) -> i32 {
        let Some(a) = &self.anim else { return 0 };
        match a.starts.get((n - 1).max(0) as usize) {
            Some(&s) if n >= 1 => self.pos() - s,
            _ => -1,
        }
    }

    /// Animación de una sola pasada ya terminada (para efectos).
    pub fn finished(&self) -> bool {
        match &self.anim {
            None => true,
            Some(a) => a.frames.is_empty() || (!a.infinite && self.looped),
        }
    }
}
