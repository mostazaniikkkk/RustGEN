//! Sprites: SFF v1 (PCX de 8 bits, el formato de MUGEN para DOS) y SFF v2
//! (RLE8, RLE5, LZ5 y PNG con paleta).
//!
//! Hay tres formas de tener un SFF, según su tamaño:
//!
//! * pequeño: se descomprime entero al cargar;
//! * descomprimido pasa de [`stream_threshold`]: se queda comprimido en
//!   memoria y cada sprite se descomprime la primera vez que se dibuja;
//! * el propio archivo pasa del umbral (SFF v2): ni siquiera se lee entero.
//!   Sólo se cargan su índice y sus paletas, y los datos de cada sprite se
//!   leen del archivo cuando hacen falta.
//!
//! En los dos últimos casos los sprites descomprimidos se guardan en una
//! caché de tamaño fijo que desecha los que llevan más tiempo sin usarse.

use crate::vfs::{self, Bytes};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Paleta de 256 colores, cada uno `r | g << 8 | b << 16 | a << 24`.
pub type Pal = [u32; 256];

/// A partir de cuántos bytes descomprimidos un SFF se carga bajo demanda.
static THRESHOLD: AtomicUsize = AtomicUsize::new(4 << 20);
/// Memoria que puede ocupar la caché de sprites de cada SFF bajo demanda.
static CACHE_BUDGET: AtomicUsize = AtomicUsize::new(1 << 20);

pub fn stream_threshold() -> usize {
    THRESHOLD.load(Ordering::Relaxed)
}

/// Cambia el umbral (en bytes descomprimidos) y el tamaño de la caché.
pub fn set_streaming(threshold: usize, cache: usize) {
    THRESHOLD.store(threshold, Ordering::Relaxed);
    CACHE_BUDGET.store(cache.max(64 << 10), Ordering::Relaxed);
}

/// Dónde están los datos comprimidos de un sprite dentro del archivo.
#[derive(Clone, Copy)]
struct Packed {
    off: usize,
    len: usize,
    /// 0 sin comprimir, 2 RLE8, 3 RLE5, 4 LZ5, 10 PNG con paleta, 255 PCX.
    fmt: u8,
}

pub struct Sprite {
    pub group: u16,
    pub image: u16,
    pub w: u16,
    pub h: u16,
    pub ax: i16,
    pub ay: i16,
    /// Índice de la paleta dentro del SFF.
    pub pal: u16,
    /// Píxeles indexados, `w * h`. Vacío si el SFF es bajo demanda (o si no
    /// se pudo decodificar): usa [`Sff::with_pixels`].
    pub pix: Vec<u8>,
    packed: Option<Packed>,
}

impl Sprite {
    /// Sprite ya descomprimido.
    #[allow(clippy::too_many_arguments)]
    pub fn new(group: u16, image: u16, w: u16, h: u16, ax: i16, ay: i16, pal: u16, pix: Vec<u8>) -> Sprite {
        Sprite { group, image, w, h, ax, ay, pal, pix, packed: None }
    }
}

/// Datos comprimidos de un SFF bajo demanda.
enum Store {
    /// El archivo entero, en memoria.
    Memory(Bytes),
    /// La ruta del archivo: cada sprite se lee cuando hace falta.
    File(String),
}

#[derive(Default)]
struct Cache {
    /// Sprite -> (píxeles, último uso).
    map: HashMap<u32, (Rc<Vec<u8>>, u64)>,
    bytes: usize,
    clock: u64,
    decodes: u64,
}

#[derive(Default)]
pub struct Sff {
    pub sprites: Vec<Sprite>,
    pub index: HashMap<(u16, u16), u32>,
    pub pals: Vec<Pal>,
    /// (grupo, número) de cada paleta, sólo en SFF v2.
    pub pal_index: HashMap<(u16, u16), u16>,
    pub version: u8,
    /// De dónde salen los datos comprimidos, si los sprites se descomprimen
    /// bajo demanda.
    store: Option<Store>,
    cache: RefCell<Cache>,
}

fn u16le(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32le(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    r as u32 | (g as u32) << 8 | (b as u32) << 16 | (a as u32) << 24
}

/// 256 tripletes RGB; el índice 0 es transparente.
fn pal_from_rgb(d: &[u8]) -> Pal {
    let mut p = [0u32; 256];
    for (i, c) in d.chunks_exact(3).take(256).enumerate() {
        p[i] = rgba(c[0], c[1], c[2], if i == 0 { 0 } else { 255 });
    }
    p
}

/// Paleta `.act`: 256 tripletes RGB guardados en orden inverso.
pub fn load_act(path: &str) -> Option<Pal> {
    let d = vfs::read(path).ok()?;
    if d.len() < 768 {
        return None;
    }
    let mut p = [0u32; 256];
    for i in 0..256 {
        let c = &d[(255 - i) * 3..];
        p[i] = rgba(c[0], c[1], c[2], if i == 0 { 0 } else { 255 });
    }
    Some(p)
}

/// Tamaño de una imagen PCX de 8 bits, leyendo sólo la cabecera.
fn pcx_size(d: &[u8]) -> Option<(u16, u16)> {
    if d.len() < 128 || d[3] != 8 {
        return None;
    }
    let w = (u16le(d, 8) as i32 - u16le(d, 4) as i32 + 1).max(0);
    let h = (u16le(d, 10) as i32 - u16le(d, 6) as i32 + 1).max(0);
    Some((w as u16, h as u16))
}

/// Imagen PCX de 8 bits: (ancho, alto, píxeles).
pub fn decode_pcx(d: &[u8]) -> Option<(u16, u16, Vec<u8>)> {
    let (w, h) = pcx_size(d)?;
    let (w, h) = (w as usize, h as usize);
    let bpl = u16le(d, 66) as usize;
    let rle = d[2] == 1;
    let mut pix = vec![0u8; w * h];
    let mut i = 128;
    for y in 0..h {
        let mut x = 0;
        while x < bpl.max(w) {
            let Some(&b) = d.get(i) else { return Some((w as u16, h as u16, pix)) };
            i += 1;
            let (n, v) = if rle && b >= 0xc0 {
                let v = d.get(i).copied().unwrap_or(0);
                i += 1;
                ((b & 0x3f) as usize, v)
            } else {
                (1, b)
            };
            for _ in 0..n {
                if x < w {
                    pix[y * w + x] = v;
                }
                x += 1;
            }
        }
    }
    Some((w as u16, h as u16, pix))
}

fn rle8(src: &[u8], out: &mut [u8]) {
    let (mut i, mut j) = (0, 0);
    while j < out.len() && i < src.len() {
        let mut b = src[i];
        i += 1;
        let mut n = 1;
        if b & 0xc0 == 0x40 {
            n = (b & 0x3f) as usize;
            b = src.get(i).copied().unwrap_or(0);
            i += 1;
        }
        for _ in 0..n {
            if j < out.len() {
                out[j] = b;
                j += 1;
            }
        }
    }
}

fn rle5(src: &[u8], out: &mut [u8]) {
    let at = |i: usize| src.get(i).copied().unwrap_or(0);
    let (mut i, mut j) = (0, 0);
    while j < out.len() && i < src.len() {
        let mut rl = at(i) as i32;
        i += 1;
        let mut dl = (at(i) & 0x7f) as i32;
        let mut c = 0u8;
        if at(i) >> 7 != 0 {
            i += 1;
            c = at(i);
        }
        i += 1;
        loop {
            if j < out.len() {
                out[j] = c;
                j += 1;
            }
            rl -= 1;
            if rl < 0 {
                dl -= 1;
                if dl < 0 {
                    break;
                }
                c = at(i) & 0x1f;
                rl = (at(i) >> 5) as i32;
                i += 1;
            }
        }
    }
}

fn lz5(src: &[u8], out: &mut [u8]) {
    let at = |i: usize| src.get(i).copied().unwrap_or(0);
    let (mut i, mut j) = (0usize, 0usize);
    let mut ct = at(i);
    i += 1;
    let mut cts = 0u32;
    let (mut rb, mut rbc) = (0u8, 0u32);
    while j < out.len() && i < src.len() {
        let mut d = at(i) as usize;
        i += 1;
        if ct & (1 << cts) != 0 {
            let n;
            if d & 0x3f == 0 {
                d = ((d << 2) | at(i) as usize) + 1;
                i += 1;
                n = at(i) as usize + 2;
                i += 1;
            } else {
                rb |= ((d & 0xc0) >> rbc) as u8;
                rbc += 2;
                n = d & 0x3f;
                if rbc < 8 {
                    d = at(i) as usize + 1;
                    i += 1;
                } else {
                    d = rb as usize + 1;
                    rb = 0;
                    rbc = 0;
                }
            }
            for _ in 0..=n {
                if j < out.len() {
                    out[j] = if j >= d { out[j - d] } else { 0 };
                    j += 1;
                }
            }
        } else {
            let n;
            if d & 0xe0 == 0 {
                n = at(i) as usize + 8;
                i += 1;
            } else {
                n = d >> 5;
                d &= 0x1f;
            }
            for _ in 0..n {
                if j < out.len() {
                    out[j] = d as u8;
                    j += 1;
                }
            }
        }
        cts += 1;
        if cts >= 8 {
            ct = at(i);
            cts = 0;
            i += 1;
        }
    }
}

/// Descomprime los píxeles de un sprite a partir de sus datos comprimidos.
/// Vacío si el formato no se soporta o los datos están mal.
fn unpack(data: &[u8], fmt: u8, w: u16, h: u16) -> Vec<u8> {
    let size = w as usize * h as usize;
    match fmt {
        255 => return decode_pcx(data).map_or(Vec::new(), |(_, _, pix)| pix),
        10 => {
            return match crate::png::decode_indexed(data) {
                Some((pw, ph, pix)) if pw * ph == size => pix,
                _ => Vec::new(),
            };
        }
        _ => {}
    }
    let mut pix = vec![0u8; size];
    let body = &data[4.min(data.len())..];
    match fmt {
        0 => {
            let n = pix.len().min(data.len());
            pix[..n].copy_from_slice(&data[..n]);
        }
        2 => rle8(body, &mut pix),
        3 => rle5(body, &mut pix),
        4 => lz5(body, &mut pix),
        _ => pix.clear(),
    }
    pix
}

impl Sff {
    /// Carga un SFF; bajo demanda si pasa del umbral.
    pub fn load(path: &str) -> Result<Sff, String> {
        Sff::load_mode(path, None)
    }

    /// Como [`Sff::load`], pero `lazy` fuerza la carga bajo demanda (`true`)
    /// o completa (`false`).
    pub fn load_mode(path: &str, lazy: Option<bool>) -> Result<Sff, String> {
        let err = |e: std::io::Error| format!("{path}: {e}");
        let size = vfs::size(path).map_err(err)?;
        let head = vfs::read_range(path, 0, size.min(64)).map_err(err)?;
        if head.len() < 64 || &head[..12] != b"ElecbyteSpr\0" {
            return Err(format!("{path}: no es un SFF"));
        }
        let version = head[15];
        // Un SFF v2 más grande que el umbral no se lee entero.
        let from_file = version == 2 && lazy != Some(false) && size > stream_threshold();
        let whole = if from_file { None } else { Some(vfs::read(path).map_err(err)?) };
        let mut sff = match (version, &whole) {
            (1, Some(d)) => Sff::parse_v1(d),
            (2, Some(d)) => Sff::parse_v2(&|off, len| d.get(off..off + len).map(<[u8]>::to_vec)),
            (2, None) => Sff::parse_v2(&|off, len| vfs::read_range(path, off, len).ok().map(|b| b.to_vec())),
            (v, _) => return Err(format!("{path}: SFF versión {v} no soportada")),
        };
        for (i, s) in sff.sprites.iter().enumerate() {
            sff.index.entry((s.group, s.image)).or_insert(i as u32);
        }
        if sff.pals.is_empty() {
            sff.pals.push([0; 256]);
        }
        match whole {
            None => sff.store = Some(Store::File(path.to_string())),
            Some(d) if lazy.unwrap_or(sff.decoded_size() > stream_threshold()) => sff.store = Some(Store::Memory(d)),
            Some(d) => {
                // Los sprites enlazados comparten datos con otro: se copian.
                for i in 0..sff.sprites.len() {
                    let (p, w, h) = (sff.sprites[i].packed, sff.sprites[i].w, sff.sprites[i].h);
                    if let Some(p) = p {
                        sff.sprites[i].pix = d.get(p.off..p.off + p.len).map_or(Vec::new(), |data| unpack(data, p.fmt, w, h));
                    }
                }
            }
        }
        Ok(sff)
    }

    /// ¿Los datos de los sprites se leen del archivo cuando hacen falta?
    pub fn is_file_backed(&self) -> bool {
        matches!(self.store, Some(Store::File(_)))
    }

    /// ¿Los sprites se descomprimen bajo demanda?
    pub fn is_streamed(&self) -> bool {
        self.store.is_some()
    }

    /// Bytes que ocupan todos los sprites descomprimidos.
    pub fn decoded_size(&self) -> usize {
        self.sprites.iter().map(|s| s.w as usize * s.h as usize).sum()
    }

    /// (sprites descomprimidos hasta ahora, bytes en la caché).
    pub fn cache_stats(&self) -> (u64, usize) {
        let c = self.cache.borrow();
        (c.decodes, c.bytes)
    }

    fn parse_v1(d: &[u8]) -> Sff {
        let mut sff = Sff { version: 1, ..Default::default() };
        let count = u32le(d, 20) as usize;
        let mut off = u32le(d, 24) as usize;
        for i in 0..count {
            if off + 32 > d.len() {
                break;
            }
            let next = u32le(d, off) as usize;
            let len = u32le(d, off + 4) as usize;
            let mut spr = Sprite::new(u16le(d, off + 12), u16le(d, off + 14), 0, 0, u16le(d, off + 8) as i16, u16le(d, off + 10) as i16, 0, Vec::new());
            let link = u16le(d, off + 16) as usize;
            let same_pal = d[off + 18] != 0;
            if len == 0 {
                // Enlazado: reutiliza la imagen de un sprite anterior.
                if let Some(src) = sff.sprites.get(link) {
                    spr.w = src.w;
                    spr.h = src.h;
                    spr.pal = src.pal;
                    spr.packed = src.packed;
                }
            } else {
                let end = if next > off + 32 && next <= d.len() { next } else { (off + 32 + len).min(d.len()) };
                let data = &d[off + 32..end];
                if let Some((w, h)) = pcx_size(data) {
                    spr.w = w;
                    spr.h = h;
                    spr.packed = Some(Packed { off: off + 32, len: end - off - 32, fmt: 255 });
                }
                if same_pal && i > 0 {
                    spr.pal = sff.sprites.last().map_or(0, |p| p.pal);
                } else if data.len() >= 768 {
                    spr.pal = sff.pals.len() as u16;
                    sff.pals.push(pal_from_rgb(&data[data.len() - 768..]));
                }
            }
            sff.sprites.push(spr);
            if next == 0 || next <= off {
                break;
            }
            off = next;
        }
        sff
    }

    /// `read(pos, tamaño)` devuelve un trozo del archivo.
    fn parse_v2(read: &dyn Fn(usize, usize) -> Option<Vec<u8>>) -> Sff {
        let mut sff = Sff { version: 2, ..Default::default() };
        let Some(d) = read(0, 68) else { return sff };
        let spr_off = u32le(&d, 36) as usize;
        let spr_count = u32le(&d, 40) as usize;
        let pal_off = u32le(&d, 44) as usize;
        let pal_count = u32le(&d, 48) as usize;
        let ldata = u32le(&d, 52) as usize;
        let tdata = u32le(&d, 60) as usize;

        // Paletas: sus datos suelen ir seguidos, así que se leen de una vez.
        let table = read(pal_off, pal_count * 16).unwrap_or_default();
        let entry = |i: usize| (ldata + u32le(&table, i * 16 + 8) as usize, u32le(&table, i * 16 + 12) as usize);
        let used: Vec<(usize, usize)> = (0..table.len() / 16).map(entry).filter(|e| e.1 > 0).collect();
        let lo = used.iter().map(|e| e.0).min().unwrap_or(0);
        let hi = used.iter().map(|e| e.0 + e.1).max().unwrap_or(0);
        let block = if hi - lo <= 8 << 20 { read(lo, hi - lo) } else { None };
        for i in 0..table.len() / 16 {
            let o = i * 16;
            let (group, item) = (u16le(&table, o), u16le(&table, o + 2));
            let link = u16le(&table, o + 6) as usize;
            let (ofs, len) = entry(i);
            let mut p = [0u32; 256];
            if len == 0 {
                if let Some(src) = sff.pals.get(link) {
                    p = *src;
                }
            } else {
                let data = match &block {
                    Some(b) => b.get(ofs - lo..ofs - lo + len).map(<[u8]>::to_vec),
                    None => read(ofs, len),
                };
                for (k, c) in data.unwrap_or_default().chunks_exact(4).take(256).enumerate() {
                    p[k] = rgba(c[0], c[1], c[2], if k == 0 { 0 } else { 255 });
                }
            }
            sff.pal_index.entry((group, item)).or_insert(i as u16);
            sff.pals.push(p);
        }

        let table = read(spr_off, spr_count * 28).unwrap_or_default();
        for i in 0..table.len() / 28 {
            let o = i * 28;
            let d = &table;
            let mut spr =
                Sprite::new(u16le(d, o), u16le(d, o + 2), u16le(d, o + 4), u16le(d, o + 6), u16le(d, o + 8) as i16, u16le(d, o + 10) as i16, u16le(d, o + 24), Vec::new());
            let link = u16le(d, o + 12) as usize;
            let (fmt, depth) = (d[o + 14], d[o + 15]);
            let len = u32le(d, o + 20) as usize;
            let base = if u16le(d, o + 26) & 1 != 0 { tdata } else { ldata };
            let ofs = base + u32le(d, o + 16) as usize;
            if spr.pal as usize >= sff.pals.len() {
                spr.pal = 0;
            }
            if len == 0 {
                spr.packed = sff.sprites.get(link).and_then(|s| s.packed);
            } else if depth <= 8 && matches!(fmt, 0 | 2 | 3 | 4 | 10) {
                spr.packed = Some(Packed { off: ofs, len, fmt });
            }
            sff.sprites.push(spr);
        }
        sff
    }

    pub fn find(&self, group: i32, image: i32) -> Option<u32> {
        if group < 0 || image < 0 {
            return None;
        }
        self.index.get(&(group as u16, image as u16)).copied()
    }

    pub fn get(&self, group: i32, image: i32) -> Option<&Sprite> {
        self.find(group, image).map(|i| &self.sprites[i as usize])
    }

    /// Llama a `f` con los píxeles del sprite `i` (descomprimiéndolo si hace
    /// falta). `None` si el sprite no existe o no se pudo decodificar.
    pub fn with_pixels<R>(&self, i: u32, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let spr = self.sprites.get(i as usize)?;
        let size = spr.w as usize * spr.h as usize;
        let Some(store) = &self.store else {
            return if spr.pix.len() >= size && size > 0 { Some(f(&spr.pix)) } else { None };
        };
        let packed = spr.packed?;
        let pix = {
            let mut cache = self.cache.borrow_mut();
            cache.clock += 1;
            let now = cache.clock;
            if let Some(entry) = cache.map.get_mut(&i) {
                entry.1 = now;
                entry.0.clone()
            } else {
                let pix = Rc::new(match store {
                    Store::Memory(d) => d.get(packed.off..packed.off + packed.len).map_or(Vec::new(), |data| unpack(data, packed.fmt, spr.w, spr.h)),
                    // Del disco: sólo los datos de este sprite.
                    Store::File(path) => vfs::read_range(path, packed.off, packed.len).map_or(Vec::new(), |data| unpack(&data, packed.fmt, spr.w, spr.h)),
                });
                cache.decodes += 1;
                cache.bytes += pix.len();
                cache.map.insert(i, (pix.clone(), now));
                // Si no cabe, fuera los que llevan más tiempo sin usarse.
                let budget = CACHE_BUDGET.load(Ordering::Relaxed);
                while cache.bytes > budget && cache.map.len() > 1 {
                    let Some(oldest) = cache.map.iter().filter(|(k, _)| **k != i).min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else { break };
                    if let Some((old, _)) = cache.map.remove(&oldest) {
                        cache.bytes -= old.len();
                    }
                }
                pix
            }
        };
        if pix.len() >= size && size > 0 {
            Some(f(&pix))
        } else {
            None
        }
    }
}
