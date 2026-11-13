//! Sonido con el SPU2, a través de los módulos `libsd` y `sdrdrv` de PS2SDK,
//! que van dentro del ejecutable y se cargan en el IOP desde memoria.
//!
//! Cada sonido del juego se codifica a PS-ADPCM al cargar y se sube a la
//! memoria del SPU2. Reproducirlo es encender una de las 24 voces del
//! núcleo 1 apuntando a su muestra: la mezcla la hace el chip.

use crate::iop::{alloc_iop, copy_to_iop, load_module, Iop, Rpc, CHUNK};
use mugen::adpcm;
use mugen::gfx::SoundCmd;
use mugen::snd::Snd;

const SID_SDR: u32 = 0x8000_0701;
const SD_INIT: u32 = 0x8000;
const SD_SET_PARAM: u32 = 0x8010;
const SD_GET_PARAM: u32 = 0x8020;
const SD_SET_SWITCH: u32 = 0x8030;
const SD_SET_ADDR: u32 = 0x8050;
const SD_GET_ADDR: u32 = 0x8060;
const SD_VOICE_TRANS: u32 = 0x80D0;
const SD_VOICE_TRANS_STATUS: u32 = 0x80F0;

const CORE1: u32 = 1;
const P_VOLL: u32 = 0x00 << 8;
const P_VOLR: u32 = 0x01 << 8;
const P_PITCH: u32 = 0x02 << 8;
const P_ADSR1: u32 = 0x03 << 8;
const P_ADSR2: u32 = 0x04 << 8;
const P_ENVX: u32 = 0x05 << 8;
const P_MVOLL: u32 = 0x09 << 8 | 0x80;
const P_MVOLR: u32 = 0x0A << 8 | 0x80;
const P_AVOLL: u32 = 0x0D << 8 | 0x80;
const P_AVOLR: u32 = 0x0E << 8 | 0x80;
const P_BVOLL: u32 = 0x0F << 8 | 0x80;
const P_BVOLR: u32 = 0x10 << 8 | 0x80;
const S_KON: u32 = 0x15 << 8;
const S_KOFF: u32 = 0x16 << 8;
const A_SSA: u32 = 0x20 << 8 | 1 << 6;
const A_NAX: u32 = 0x22 << 8 | 1 << 6;

const VOICES: usize = 24;
/// Primera dirección libre de la memoria del SPU2.
const SPU_BASE: u32 = 0x5010;
const SPU_END: u32 = 0x1f_fff0;
static LIBSD: &[u8] = include_bytes!("../modules/libsd.irx");
static SDRDRV: &[u8] = include_bytes!("../modules/sdrdrv.irx");

struct Sample {
    addr: u32,
    pitch: u32,
    /// Duración en frames (1/60 s).
    frames: u32,
}

#[derive(Clone, Copy, Default)]
struct Voice {
    chan: i32,
    busy_until: u32,
    started: u32,
}

pub struct Audio {
    rpc: Option<Rpc>,
    /// Muestras por banco de sonido, en el mismo orden que `Snd::samples`.
    banks: Vec<Vec<Option<Sample>>>,
    voices: [Voice; VOICES],
    frame: u32,
    pub loaded: usize,
    pub spu_used: u32,
    /// Última voz encendida (para pruebas).
    pub last_voice: u32,
    /// Búfer de paso en el IOP y su copia de origen en el EE.
    iop_buf: u32,
    stage: *mut u8,
}

impl Audio {
    fn sd(&mut self, cmd: u32, a: u32, b: u32) -> u32 {
        let Some(rpc) = self.rpc.as_mut() else { return 0 };
        let addr = rpc.addr();
        rpc.set(0, addr);
        rpc.set(1, a);
        rpc.set(2, b);
        rpc.call(cmd, 64, 16);
        rpc.get(0)
    }

    /// Inicializa el SPU2. Los sonidos se suben después con [`Audio::load`].
    pub fn init(iop: Option<&mut Iop>) -> Audio {
        let mut audio = Audio {
            rpc: None,
            banks: Vec::new(),
            voices: [Voice { chan: -1, busy_until: 0, started: 0 }; VOICES],
            frame: 0,
            loaded: 0,
            spu_used: 0,
            last_voice: 0,
            iop_buf: 0,
            stage: std::ptr::null_mut(),
        };
        let Some(iop) = iop else { return audio };
        // `libsd` habla con el SPU2; si no carga, vale la versión de la BIOS.
        if iop.exec_module("libsd", LIBSD) < 0 && !load_module("rom0:LIBSD") {
            return audio;
        }
        if iop.exec_module("sdrdrv", SDRDRV) < 0 {
            return audio;
        }
        // El cargador de módulos de la BIOS es antiguo: interpreta el valor
        // que devuelve `sdrdrv` al arrancar como "ya terminé" y da por libre
        // su memoria, aunque el módulo sigue funcionando en ella. La
        // siguiente reserva caería justo encima, así que se reserva ese
        // hueco ahora mismo y no se toca.
        let guard = alloc_iop(0x4000);
        // El nombre del módulo ("sdrdrv") está al principio de su memoria.
        let covered = guard.is_some_and(|g| iop.peek(g + 0x3c).to_le_bytes() == *b"sdrd");
        println!("audio: IOP: búfer en {:#x}, resguardo de sdrdrv en {:x?} ({})", iop.buf, guard, if covered { "correcto" } else { "NO lo cubre" });
        audio.iop_buf = iop.buf;
        audio.stage = iop.stage;
        audio.rpc = Rpc::bind(SID_SDR, 200);
        if audio.rpc.is_none() {
            println!("audio: SDRDRV no responde");
            return audio;
        }
        audio.sd(SD_INIT, 0, 0);
        // El núcleo 0 pasa por el 1, que es el que suena.
        for (reg, v) in [(P_AVOLL, 0x7fff), (P_AVOLR, 0x7fff), (P_BVOLL, 0), (P_BVOLR, 0), (P_MVOLL, 0x3fff), (P_MVOLR, 0x3fff)] {
            audio.sd(SD_SET_PARAM, CORE1 | reg, v);
        }
        audio.sd(SD_SET_PARAM, P_MVOLL, 0);
        audio.sd(SD_SET_PARAM, P_MVOLR, 0);
        audio.sd(SD_SET_SWITCH, CORE1 | S_KOFF, 0xff_ffff);
        for v in 0..VOICES as u32 {
            let voice = CORE1 | v << 1;
            audio.sd(SD_SET_PARAM, voice | P_VOLL, 0x3fff);
            audio.sd(SD_SET_PARAM, voice | P_VOLR, 0x3fff);
            // Ataque inmediato, sostenido constante, liberación inmediata.
            audio.sd(SD_SET_PARAM, voice | P_ADSR1, 0x00ff);
            audio.sd(SD_SET_PARAM, voice | P_ADSR2, 0x1fc0);
        }
        audio
    }

    /// Sustituye los sonidos cargados por los de `snds`: los codifica a
    /// PS-ADPCM y los sube a la memoria del SPU2.
    pub fn load(&mut self, snds: &[Snd]) {
        if self.rpc.is_none() {
            return;
        }
        let audio = self;
        audio.sd(SD_SET_SWITCH, CORE1 | S_KOFF, 0xff_ffff);
        audio.voices = [Voice { chan: -1, busy_until: 0, started: 0 }; VOICES];
        audio.banks.clear();
        audio.loaded = 0;
        let (stage, iop_buf) = (audio.stage, audio.iop_buf);
        // Si todos los sonidos no caben en los 2 MB del SPU2 a su
        // frecuencia original, se baja la frecuencia máxima hasta que quepan.
        let adpcm_bytes = |frames: u64| (frames / 28 + 3) * 16;
        let total = |cap: u32| -> u64 {
            snds.iter().flat_map(|s| &s.samples).filter(|s| s.rate > 0).map(|s| adpcm_bytes(s.frames as u64 * s.rate.min(cap) as u64 / s.rate as u64)).sum()
        };
        let room = (SPU_END - SPU_BASE) as u64;
        let cap = [48000u32, 32000, 24000, 22050, 16000, 11025, 8000, 6000, 4000].into_iter().find(|&c| total(c) <= room).unwrap_or(4000);
        if cap < 48000 && total(48000) > room {
            println!("audio: {} KB de sonido no caben en el SPU2: frecuencia limitada a {cap} Hz", total(48000) / 1024);
        }
        let mut next = SPU_BASE;
        for snd in snds {
            let mut bank = Vec::new();
            for (si, s) in snd.samples.iter().enumerate() {
                let rate = s.rate.min(cap);
                // Leer (del disco si hace falta), bajar de frecuencia y codificar.
                let data = snd.with_pcm(si as u32, |pcm| {
                    if rate == s.rate || s.rate == 0 {
                        return adpcm::encode(pcm);
                    }
                    let n = (pcm.len() as u64 * rate as u64 / s.rate as u64) as usize;
                    let low: Vec<i16> = (0..n)
                        .map(|i| {
                            // Media de las muestras que caen en cada intervalo.
                            let (from, to) = (i * pcm.len() / n, ((i + 1) * pcm.len() / n).max(i * pcm.len() / n + 1));
                            let chunk = &pcm[from..to.min(pcm.len())];
                            (chunk.iter().map(|&v| v as i32).sum::<i32>() / chunk.len().max(1) as i32) as i16
                        })
                        .collect();
                    adpcm::encode(&low)
                });
                let Some(data) = data.filter(|d| next + d.len() as u32 <= SPU_END && s.frames > 0 && s.rate > 0) else {
                    bank.push(None);
                    continue;
                };
                for (i, part) in data.chunks(CHUNK).enumerate() {
                    unsafe { std::ptr::copy_nonoverlapping(part.as_ptr(), stage, part.len()) };
                    copy_to_iop(stage as u32 & 0x1fff_ffff, iop_buf, part.len() as u32);
                    let rpc = audio.rpc.as_mut().unwrap();
                    let addr = rpc.addr();
                    rpc.set(0, addr);
                    rpc.set(1, 0); // canal de transferencia
                    rpc.set(2, 0); // escribir, por DMA
                    rpc.set(3, iop_buf);
                    rpc.set(4, next + (i * CHUNK) as u32);
                    rpc.set(5, part.len() as u32);
                    rpc.call(SD_VOICE_TRANS, 64, 16);
                    // Espera a que termine la transferencia.
                    audio.sd(SD_VOICE_TRANS_STATUS, 0, 1);
                }
                bank.push(Some(Sample {
                    addr: next,
                    pitch: (rate * 4096 / 48000).clamp(1, 0x3fff),
                    frames: (s.frames as u64 * 60 / s.rate as u64) as u32 + 3,
                }));
                next += data.len() as u32;
                audio.loaded += 1;
            }
            audio.banks.push(bank);
        }
        audio.spu_used = next - SPU_BASE;
    }

    /// Atiende los sonidos de un frame.
    pub fn play(&mut self, cmds: &[SoundCmd]) {
        self.frame += 1;
        if self.rpc.is_none() {
            return;
        }
        for c in cmds {
            if c.stop {
                for v in 0..VOICES {
                    if self.voices[v].chan == c.chan && self.voices[v].busy_until > self.frame {
                        self.sd(SD_SET_SWITCH, CORE1 | S_KOFF, 1 << v);
                        self.voices[v].busy_until = 0;
                    }
                }
                continue;
            }
            let Some(&Sample { addr, pitch, frames }) = self.banks.get(c.bank as usize).and_then(|b| b.get(c.idx as usize)).and_then(|s| s.as_ref())
            else {
                continue;
            };
            // La voz del mismo canal, una libre o, si no hay, la más antigua.
            let same = if c.chan >= 0 { (0..VOICES).find(|&v| self.voices[v].chan == c.chan) } else { None };
            let v = same
                .or_else(|| (0..VOICES).find(|&v| self.voices[v].busy_until <= self.frame))
                .unwrap_or_else(|| (0..VOICES).min_by_key(|&v| self.voices[v].started).unwrap());
            let pitch = ((pitch as f32 * c.freq) as u32).clamp(1, 0x3fff);
            let vol = c.vol.clamp(0.0, 1.0) * 0x3fff as f32;
            let left = (vol * (1.0 - c.pan.max(0.0))) as u32;
            let right = (vol * (1.0 + c.pan.min(0.0))) as u32;
            let voice = CORE1 | (v as u32) << 1;
            self.sd(SD_SET_PARAM, voice | P_VOLL, left);
            self.sd(SD_SET_PARAM, voice | P_VOLR, right);
            self.sd(SD_SET_PARAM, voice | P_PITCH, pitch);
            self.sd(SD_SET_ADDR, voice | A_SSA, addr);
            self.sd(SD_SET_SWITCH, CORE1 | S_KON, 1 << v);
            self.voices[v] = Voice { chan: c.chan, busy_until: self.frame + (frames as f32 / c.freq.max(0.1)) as u32, started: self.frame };
            self.last_voice = v as u32;
        }
    }

    /// Para pruebas: (dirección de reproducción, envolvente) de una voz.
    pub fn probe(&mut self, v: u32) -> (u32, u32) {
        let voice = CORE1 | v << 1;
        (self.sd(SD_GET_ADDR, voice | A_NAX, 0), self.sd(SD_GET_PARAM, voice | P_ENVX, 0))
    }

}
