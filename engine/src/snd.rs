//! Sonidos: contenedor SND de MUGEN con archivos WAV dentro.
//!
//! Un SND pequeño se decodifica entero al cargar. De uno grande sólo se lee
//! el índice; cada sonido se lee del archivo y se decodifica cuando alguien
//! lo pide con [`Snd::with_pcm`].

use crate::vfs;
use std::collections::HashMap;

/// A partir de este tamaño de archivo, los sonidos se leen bajo demanda.
const LAZY_BYTES: usize = 4 << 20;

pub struct Sample {
    pub rate: u32,
    /// Duración en muestras (ya en mono).
    pub frames: usize,
    /// PCM mono de 16 bits; vacío si el SND es bajo demanda.
    pcm: Vec<i16>,
    /// Posición y tamaño del WAV dentro del archivo.
    wav: (usize, usize),
}

#[derive(Default)]
pub struct Snd {
    pub samples: Vec<Sample>,
    pub index: HashMap<(i32, i32), u32>,
    /// Ruta del archivo, si los sonidos se leen bajo demanda.
    path: Option<String>,
}

fn u32le(d: &[u8], o: usize) -> u32 {
    d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn u16le(d: &[u8], o: usize) -> u16 {
    d.get(o..o + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]))
}

/// Cabecera de un WAV PCM: canales, frecuencia, bits, y posición y tamaño de
/// los datos. `None` si no es PCM o si los datos no empiezan dentro de `d`.
fn wav_info(d: &[u8]) -> Option<(usize, u32, u16, usize, usize)> {
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return None;
    }
    let (mut channels, mut rate, mut bits) = (1usize, 11025u32, 8u16);
    let mut o = 12;
    while o + 8 <= d.len() {
        let id = &d[o..o + 4];
        let len = u32le(d, o + 4) as usize;
        let body = o + 8;
        if id == b"fmt " {
            if u16le(d, body) != 1 {
                return None;
            }
            channels = u16le(d, body + 2).max(1) as usize;
            rate = u32le(d, body + 4);
            bits = u16le(d, body + 14);
        } else if id == b"data" {
            return Some((channels, rate, bits, body, len));
        }
        o = body + len + (len & 1);
    }
    None
}

/// WAV PCM de 8 o 16 bits, mono o estéreo, a mono de 16 bits.
pub fn decode_wav(d: &[u8]) -> Option<(u32, Vec<i16>)> {
    let (channels, rate, bits, body, len) = wav_info(d)?;
    let data = &d[body..(body + len).min(d.len())];
    let bytes = (bits as usize / 8).max(1);
    let mut pcm = Vec::with_capacity(data.len() / (bytes * channels));
    for f in data.chunks_exact(bytes * channels) {
        let mut sum = 0i32;
        for c in 0..channels {
            let s = &f[c * bytes..];
            sum += if bytes == 1 { (s[0] as i32 - 128) << 8 } else { i16::from_le_bytes([s[0], s[1]]) as i32 };
        }
        pcm.push((sum / channels as i32) as i16);
    }
    Some((rate, pcm))
}

impl Snd {
    pub fn load(path: &str) -> Result<Snd, String> {
        let err = |e: std::io::Error| format!("{path}: {e}");
        let size = vfs::size(path).map_err(err)?;
        let lazy = size > LAZY_BYTES;
        // Entero en memoria, o sólo la cabecera y después trozo a trozo.
        let whole = if lazy { None } else { Some(vfs::read(path).map_err(err)?) };
        let range = |off: usize, len: usize| -> Option<Vec<u8>> {
            let len = len.min(size.saturating_sub(off));
            match &whole {
                Some(d) => d.get(off..off + len).map(<[u8]>::to_vec),
                None => vfs::read_range(path, off, len).ok().map(|b| b.to_vec()),
            }
        };
        let head = range(0, 24).ok_or(format!("{path}: no es un SND"))?;
        if head.len() < 24 || &head[..12] != b"ElecbyteSnd\0" {
            return Err(format!("{path}: no es un SND"));
        }
        let mut snd = Snd { path: if lazy { Some(path.to_string()) } else { None }, ..Default::default() };
        let mut off = u32le(&head, 20) as usize;
        while off != 0 && off + 16 <= size {
            let Some(h) = range(off, 16) else { break };
            let next = u32le(&h, 0) as usize;
            let len = (u32le(&h, 4) as usize).min(size - off - 16);
            let key = (u32le(&h, 8) as i32, u32le(&h, 12) as i32);
            let wav = (off + 16, len);
            let sample = if lazy {
                // Con la cabecera basta para saber la duración.
                range(wav.0, len.min(4096)).and_then(|d| wav_info(&d)).map(|(channels, rate, bits, body, data)| {
                    let frame = (bits as usize / 8).max(1) * channels;
                    Sample { rate, frames: data.min(len.saturating_sub(body)) / frame, pcm: Vec::new(), wav }
                })
            } else {
                range(wav.0, len).and_then(|d| decode_wav(&d)).map(|(rate, pcm)| Sample { rate, frames: pcm.len(), pcm, wav })
            };
            if let Some(s) = sample {
                snd.index.entry(key).or_insert(snd.samples.len() as u32);
                snd.samples.push(s);
            }
            if next <= off {
                break;
            }
            off = next;
        }
        Ok(snd)
    }

    pub fn find(&self, group: i32, number: i32) -> Option<u32> {
        self.index.get(&(group, number)).copied()
    }

    /// ¿Los sonidos se leen del archivo bajo demanda?
    pub fn is_streamed(&self) -> bool {
        self.path.is_some()
    }

    /// Llama a `f` con el PCM mono de 16 bits del sonido `i`, leyéndolo del
    /// archivo si hace falta.
    pub fn with_pcm<R>(&self, i: u32, f: impl FnOnce(&[i16]) -> R) -> Option<R> {
        let s = self.samples.get(i as usize)?;
        match &self.path {
            None => Some(f(&s.pcm)),
            Some(path) => {
                let d = vfs::read_range(path, s.wav.0, s.wav.1).ok()?;
                decode_wav(&d).map(|(_, pcm)| f(&pcm))
            }
        }
    }
}
