//! Graphics Synthesizer: dibuja la lista de sprites del motor con texturas
//! de 8 bits y paleta (CLUT).
//!
//! El juego es de 320×240, pero una tele NTSC sólo enseña unas 224 líneas
//! por campo. Hay dos modos:
//!
//! * `Interlaced` (por defecto): 320×448 entrelazado; las 240 líneas del
//!   juego se estiran a 448 y se ve la imagen completa.
//! * `Progressive`: 320×240 sin entrelazar, píxel a píxel; según la tele (o
//!   el emulador) se pierden unas 8 líneas arriba y abajo.
//!
//! La VRAM (4 MB) se reparte así: dos framebuffers, una zona de paletas y el
//! resto como caché de texturas, que se vacía entera cuando se llena.

use mugen::gfx::{Cmd, Frame, PalRef, SpriteCmd, Trans, SCREEN_H, SCREEN_W};
use mugen::sff::{Pal, Sff};
use ps2::kernel::{self, read32, read64, write32, write64};
use std::cell::UnsafeCell;
use std::collections::HashMap;

// Registros privilegiados del GS.
const GS_PMODE: u32 = 0x1200_0000;
const GS_DISPFB2: u32 = 0x1200_0090;
const GS_DISPLAY2: u32 = 0x1200_00A0;
const GS_BGCOLOR: u32 = 0x1200_00E0;
const GS_CSR: u32 = 0x1200_1000;

// Canal 2 de DMA (GIF).
const D2_CHCR: u32 = 0x1000_A000;
const D2_MADR: u32 = 0x1000_A010;
const D2_QWC: u32 = 0x1000_A020;
const D_CTRL: u32 = 0x1000_E000;
const GIF_CTRL: u32 = 0x1000_3000;

// Registros generales (direcciones A+D).
const PRIM: u64 = 0x00;
const RGBAQ: u64 = 0x01;
const UV: u64 = 0x03;
const XYZ2: u64 = 0x05;
const TEX0_1: u64 = 0x06;
const CLAMP_1: u64 = 0x08;
const TEX1_1: u64 = 0x14;
const XYOFFSET_1: u64 = 0x18;
const PRMODECONT: u64 = 0x1A;
const TEXA: u64 = 0x3B;
const TEXFLUSH: u64 = 0x3F;
const SCISSOR_1: u64 = 0x40;
const ALPHA_1: u64 = 0x42;
const DTHE: u64 = 0x45;
const COLCLAMP: u64 = 0x46;
const TEST_1: u64 = 0x47;
const PABE: u64 = 0x49;
const FBA_1: u64 = 0x4A;
const FRAME_1: u64 = 0x4C;
const ZBUF_1: u64 = 0x4E;
const BITBLTBUF: u64 = 0x50;
const TRXPOS: u64 = 0x51;
const TRXREG: u64 = 0x52;
const TRXDIR: u64 = 0x53;
const FINISH: u64 = 0x61;

const PSMCT32: u64 = 0x00;
const PSMT8: u64 = 0x13;

/// Origen de la ventana dentro del espacio de 4096×4096 del GS.
const ORIGIN: i32 = 1024;

/// Tamaño del paquete, en palabras de 128 bits.
const PACKET_QW: usize = 16384;

// La VRAM se mide en bloques de 256 bytes (una página son 32 bloques).
const CLUT_SLOTS: u32 = 128;
/// Las primeras ranuras son para paletas que cambian cada frame.
const CLUT_DYN: u32 = 16;
const TEX_END: u32 = 16384;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VideoMode {
    /// 320×448 entrelazado: imagen completa, estirada en vertical.
    Interlaced,
    /// 320×240 sin entrelazar: píxel a píxel.
    Progressive,
}

#[repr(C, align(64))]
struct PacketBuffer(UnsafeCell<[u64; PACKET_QW * 2]>);

// Sólo lo usa la única instancia de `Gs`.
unsafe impl Sync for PacketBuffer {}

static PACKET: PacketBuffer = PacketBuffer(UnsafeCell::new([0; PACKET_QW * 2]));

#[derive(Clone, Copy)]
struct Tex {
    tbp: u32,
    tbw: u32,
    tw: u32,
    th: u32,
}

pub struct Gs {
    /// Líneas del framebuffer y escala vertical respecto a las 240 del juego.
    height: i32,
    ys: f32,
    clut_base: u32,
    tex_base: u32,
    buf: *mut u64,
    /// Registros A+D acumulados.
    len: usize,
    fbp: [u32; 2],
    draw: usize,
    pub frames: u32,
    tex: HashMap<(u16, u32), Tex>,
    tex_next: u32,
    pals: HashMap<(u16, u16), u32>,
    pal_next: u32,
    dyn_next: u32,
    /// Ranura de CLUT de cada paleta dinámica de este frame.
    dyn_slot: Vec<Option<u32>>,
    last_tex0: u64,
    last_alpha: u64,
    /// Estadísticas del último frame.
    pub uploads: u32,
    pub sprites: u32,
}

fn log2_ceil(v: u32) -> u32 {
    32 - (v.max(1) - 1).leading_zeros()
}

impl Gs {
    /// Enciende el vídeo NTSC (60 Hz) en el modo pedido.
    pub fn init(mode: VideoMode) -> Gs {
        let height = if mode == VideoMode::Interlaced { 448 } else { SCREEN_H };
        // Cada framebuffer ocupa páginas enteras de 64×32 píxeles.
        let fb_pages = (SCREEN_W as u32 / 64) * (height as u32).div_ceil(32);
        let clut_base = fb_pages * 2 * 32;
        let tex_base = clut_base + CLUT_SLOTS * 4;
        let mut gs = Gs {
            height,
            ys: height as f32 / SCREEN_H as f32,
            clut_base,
            tex_base,
            buf: (PACKET.0.get() as usize | 0x2000_0000) as *mut u64,
            len: 0,
            fbp: [0, fb_pages],
            draw: 0,
            frames: 0,
            tex: HashMap::new(),
            tex_next: tex_base,
            pals: HashMap::new(),
            pal_next: CLUT_DYN,
            dyn_next: 0,
            dyn_slot: Vec::new(),
            last_tex0: u64::MAX,
            last_alpha: u64::MAX,
            uploads: 0,
            sprites: 0,
        };
        unsafe {
            write32(D_CTRL, read32(D_CTRL) | 1);
            write32(GIF_CTRL, 1);
            write64(GS_CSR, 0x200);
            kernel::gs_put_imr(0xff00);
            let interlaced = mode == VideoMode::Interlaced;
            kernel::set_gs_crt(interlaced, 2, !interlaced);
            write64(GS_PMODE, 0x8046);
            gs.show(1);
            // DX, DY | MAGH = 7 (2560 / 320 - 1) | DW = 2559 | DH = alto - 1.
            let dy: u64 = if interlaced { 50 } else { 18 };
            let display = 652u64 | dy << 12 | 7 << 23 | 2559 << 32 | ((height - 1) as u64) << 44;
            write64(GS_DISPLAY2, display);
            write64(GS_BGCOLOR, 0);
        }
        gs.push(PRMODECONT, 1);
        gs.push(XYOFFSET_1, (ORIGIN as u64) << 4 | ((ORIGIN as u64) << 4) << 32);
        gs.push(SCISSOR_1, ((SCREEN_W - 1) as u64) << 16 | ((height - 1) as u64) << 48);
        gs.push(ZBUF_1, (fb_pages * 2) as u64 | 1 << 32);
        // Prueba alfa: sólo pasan los píxeles con alfa > 0 (el color 0 de la
        // paleta es transparente). Sin prueba de profundidad.
        gs.push(TEST_1, 1 | 6 << 1 | 1 << 16 | 1 << 17);
        gs.push(COLCLAMP, 1);
        gs.push(DTHE, 0);
        gs.push(PABE, 0);
        gs.push(FBA_1, 0);
        gs.push(TEXA, 0x80 | 0x80 << 32);
        gs.push(CLAMP_1, 1 | 1 << 2);
        // Sin filtrado: el texel más cercano.
        gs.push(TEX1_1, 1);
        for target in [1usize, 0] {
            gs.draw = target;
            gs.select_frame();
            gs.rect(0, 0, SCREEN_W, SCREEN_H, 0xff00_0000, Trans::None);
        }
        gs.flush();
        gs
    }

    unsafe fn show(&self, index: usize) {
        write64(GS_DISPFB2, self.fbp[index] as u64 | (SCREEN_W as u64 / 64) << 9);
    }

    fn select_frame(&mut self) {
        self.push(FRAME_1, self.fbp[self.draw] as u64 | (SCREEN_W as u64 / 64) << 16);
    }

    fn push(&mut self, reg: u64, value: u64) {
        unsafe {
            let slot = self.buf.add(2 * (self.len + 1));
            slot.write_volatile(value);
            slot.add(1).write_volatile(reg);
        }
        self.len += 1;
        if self.len == PACKET_QW - 1 {
            self.flush();
        }
    }

    /// Envía `qwords` palabras del paquete por DMA y espera a que salgan.
    unsafe fn kick(&self, qwords: usize) {
        while read32(D2_CHCR) & 0x100 != 0 {}
        write32(D2_MADR, PACKET.0.get() as u32 & 0x1fff_ffff);
        write32(D2_QWC, qwords as u32);
        write32(D2_CHCR, 0x101);
        while read32(D2_CHCR) & 0x100 != 0 {}
    }

    /// Envía al GS los registros acumulados.
    pub fn flush(&mut self) {
        if self.len == 0 {
            return;
        }
        unsafe {
            // GIFtag: NLOOP = len, EOP, modo PACKED, un registro (A+D).
            self.buf.write_volatile(self.len as u64 | 1 << 15 | 1 << 60);
            self.buf.add(1).write_volatile(0xE);
            self.kick(self.len + 1);
        }
        self.len = 0;
    }

    /// Copia una imagen a la VRAM. `bpp` son los bytes por píxel.
    fn upload(&mut self, dbp: u32, dbw: u32, psm: u64, w: u32, h: u32, bpp: u32, data: &[u8]) {
        self.flush();
        let row = (w * bpp) as usize;
        if row == 0 || h == 0 {
            return;
        }
        let max_rows = (((PACKET_QW - 8) * 16) / row).max(1) as u32;
        let mut y = 0;
        while y < h {
            let rows = max_rows.min(h - y);
            let bytes = row * rows as usize;
            let src = &data[row * y as usize..];
            let bytes = bytes.min(src.len());
            let nq = bytes.div_ceil(16);
            unsafe {
                let b = self.buf;
                b.write_volatile(4 | 1 << 60);
                b.add(1).write_volatile(0xE);
                b.add(2).write_volatile((dbp as u64) << 32 | (dbw as u64) << 48 | psm << 56);
                b.add(3).write_volatile(BITBLTBUF);
                b.add(4).write_volatile((y as u64) << 48);
                b.add(5).write_volatile(TRXPOS);
                b.add(6).write_volatile(w as u64 | (rows as u64) << 32);
                b.add(7).write_volatile(TRXREG);
                b.add(8).write_volatile(0);
                b.add(9).write_volatile(TRXDIR);
                // GIFtag en modo IMAGE.
                b.add(10).write_volatile(nq as u64 | 1 << 15 | 2 << 58);
                b.add(11).write_volatile(0);
                let dst = b.add(12) as *mut u8;
                // Rellena la última palabra antes de copiar encima.
                b.add(12 + (nq - 1) * 2).write_volatile(0);
                b.add(13 + (nq - 1) * 2).write_volatile(0);
                std::ptr::copy_nonoverlapping(src.as_ptr(), dst, bytes);
                self.kick(6 + nq);
            }
            y += rows;
        }
        self.uploads += 1;
        self.push(TEXFLUSH, 0);
        // Una paleta nueva en la misma ranura exige volver a cargar la CLUT.
        self.last_tex0 = u64::MAX;
    }

    /// Reserva sitio para una textura de 8 bits y la sube.
    fn texture(&mut self, sffs: &[Sff], key: (u16, u32)) -> Option<Tex> {
        if let Some(t) = self.tex.get(&key) {
            return Some(*t);
        }
        let spr = sffs.get(key.0 as usize)?.sprites.get(key.1 as usize)?;
        let (w, h) = (spr.w as u32, spr.h as u32);
        if w == 0 || h == 0 || w > 1024 || h > 1024 {
            return None;
        }
        // Las texturas pequeñas ocupan una región de bloques alineada a su
        // tamaño; las demás, páginas enteras.
        let (blocks, tbw) = if w <= 128 && h <= 64 {
            const REGIONS: [(u32, u32, u32); 6] = [(16, 16, 1), (32, 16, 2), (32, 32, 4), (64, 32, 8), (64, 64, 16), (128, 64, 32)];
            let r = REGIONS.iter().find(|r| w <= r.0 && h <= r.1).unwrap();
            (r.2, 2)
        } else {
            let across = w.div_ceil(128);
            (across * h.div_ceil(64) * 32, across * 2)
        };
        let align = blocks.min(32);
        let mut tbp = self.tex_next.div_ceil(align) * align;
        if tbp + blocks > TEX_END {
            // Caché llena: se empieza de cero. Lo ya dibujado no se pierde,
            // porque el GS dibuja en el momento.
            self.flush();
            self.tex.clear();
            tbp = self.tex_base.div_ceil(align) * align;
            if tbp + blocks > TEX_END {
                return None;
            }
        }
        self.tex_next = tbp + blocks;
        // Un SFF grande descomprime el sprite en este momento.
        sffs[key.0 as usize].with_pixels(key.1, |pix| self.upload(tbp, tbw, PSMT8, w, h, 1, &pix[..(w * h) as usize]))?;
        let t = Tex { tbp, tbw, tw: log2_ceil(w), th: log2_ceil(h) };
        self.tex.insert(key, t);
        Some(t)
    }

    fn upload_clut(&mut self, slot: u32, pal: &Pal) {
        let mut data = [0u8; 1024];
        for (i, &c) in pal.iter().enumerate() {
            // El GS guarda la CLUT de 8 bits con las entradas 8-15 y 16-23
            // de cada grupo de 32 intercambiadas. Alfa 0x80 = opaco.
            let j = match i & 0x18 {
                0x08 => i + 8,
                0x10 => i - 8,
                _ => i,
            };
            let a = if c >> 24 != 0 { 0x80u32 } else { 0 };
            data[j * 4..j * 4 + 4].copy_from_slice(&((c & 0x00ff_ffff) | a << 24).to_le_bytes());
        }
        self.upload(self.clut_base + slot * 4, 1, PSMCT32, 16, 16, 4, &data);
    }

    /// Ranura de CLUT de una paleta, subiéndola si hace falta.
    fn clut(&mut self, frame: &Frame, sffs: &[Sff], pal: PalRef) -> Option<u32> {
        match pal {
            PalRef::Dyn(i) => {
                let i = i as usize;
                if let Some(Some(slot)) = self.dyn_slot.get(i) {
                    return Some(*slot);
                }
                let p = frame.pals.get(i)?;
                let slot = self.dyn_next % CLUT_DYN;
                self.dyn_next += 1;
                // La ranura se recicla: quien la tuviera deja de valer.
                for s in self.dyn_slot.iter_mut() {
                    if *s == Some(slot) {
                        *s = None;
                    }
                }
                self.upload_clut(slot, p);
                if self.dyn_slot.len() <= i {
                    self.dyn_slot.resize(i + 1, None);
                }
                self.dyn_slot[i] = Some(slot);
                Some(slot)
            }
            PalRef::Sff(s, p) => {
                if let Some(slot) = self.pals.get(&(s, p)) {
                    return Some(*slot);
                }
                let pal = sffs.get(s as usize)?.pals.get(p as usize)?;
                if self.pal_next >= CLUT_SLOTS {
                    self.flush();
                    self.pals.clear();
                    self.pal_next = CLUT_DYN;
                }
                let slot = self.pal_next;
                self.pal_next += 1;
                self.upload_clut(slot, pal);
                self.pals.insert((s, p), slot);
                Some(slot)
            }
        }
    }

    /// Mezcla: devuelve los valores de ALPHA de cada pasada (la primera, si
    /// la hay, sólo atenúa el destino).
    fn blend(t: Trans) -> (Option<u64>, Option<u64>) {
        // ((A - B) * FIX >> 7) + D, con 0 = origen, 1 = destino, 2 = cero.
        let alpha = |a: u64, b: u64, d: u64, fix: u16| Some(a | b << 2 | 2 << 4 | d << 6 | ((fix as u64 / 2).min(0x80)) << 32);
        match t {
            Trans::None => (None, None),
            Trans::Sub => (None, alpha(1, 0, 2, 256)),
            Trans::Add { src, dst } if dst >= 256 => (None, alpha(0, 2, 1, src)),
            Trans::Add { src, dst } if src >= 256 => (None, alpha(1, 2, 0, dst)),
            Trans::Add { src, dst } if src + dst == 256 => (None, alpha(0, 1, 1, src)),
            Trans::Add { src, dst } => (alpha(2, 1, 1, 256 - dst), alpha(0, 2, 1, src)),
        }
    }

    fn set_alpha(&mut self, value: u64) {
        if self.last_alpha != value {
            self.push(ALPHA_1, value);
            self.last_alpha = value;
        }
    }

    fn xy(&mut self, x: i32, y: i32) {
        let fx = ((x + ORIGIN).clamp(0, 4095) as u64) << 4;
        let fy = ((y + ORIGIN).clamp(0, 4095) as u64) << 4;
        self.push(XYZ2, fx | fy << 16);
    }

    /// Línea del framebuffer donde empieza la línea `y` del juego.
    fn row(&self, y: i32) -> i32 {
        (y as f32 * self.ys + 0.5) as i32
    }

    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgba: u32, trans: Trans) {
        let (x0, y0) = (x.max(0), self.row(y.max(0)));
        let (x1, y1) = ((x + w).min(SCREEN_W), self.row((y + h).min(SCREEN_H)));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let color = (rgba & 0x00ff_ffff) as u64 | 0x80 << 24 | 1u64 << 32;
        for a in [Self::blend(trans).0, Self::blend(trans).1].into_iter().flatten() {
            self.set_alpha(a);
            self.push(PRIM, 6 | 1 << 6);
            self.push(RGBAQ, color);
            self.xy(x0, y0);
            self.xy(x1, y1);
        }
        if trans == Trans::None {
            self.push(PRIM, 6);
            self.push(RGBAQ, color);
            self.xy(x0, y0);
            self.xy(x1, y1);
        }
    }

    fn sprite(&mut self, frame: &Frame, sffs: &[Sff], c: &SpriteCmd) {
        if c.sx == 0.0 || c.sy == 0.0 {
            return;
        }
        let Some(spr) = sffs.get(c.sff as usize).and_then(|s| s.sprites.get(c.spr as usize)) else { return };
        let (w, h) = (spr.w as i32, spr.h as i32);
        let (ax, ay) = (spr.ax as f32, spr.ay as f32);
        let [u0, v0, u1, v1] = c.src.map(|s| s.map(|v| v as i32)).unwrap_or([0, 0, w, h]);
        let (u0, v0, u1, v1) = (u0.max(0), v0.max(0), u1.min(w), v1.min(h));
        let [cx0, cy0, cx1, cy1] = c.clip.unwrap_or([0, 0, SCREEN_W, SCREEN_H]);
        // Mismo recorrido de píxeles que el rasterizador por software.
        let span = |o: f32, a: f32, s: f32, lo: i32, hi: i32| -> (i32, i32) {
            let p = o + (lo as f32 - a) * s;
            let q = o + (hi as f32 - a) * s;
            let (min, max) = if p < q { (p, q) } else { (q, p) };
            ((min - 0.5).ceil() as i32, (max - 0.5).ceil() as i32)
        };
        // En vertical todo va en líneas del framebuffer.
        let (oy, osy) = (c.y * self.ys, c.sy * self.ys);
        let (x0, x1) = span(c.x, ax, c.sx, u0, u1);
        let (y0, y1) = span(oy, ay, osy, v0, v1);
        let (x0, x1) = (x0.max(cx0).max(0), x1.min(cx1).min(SCREEN_W));
        let (y0, y1) = (y0.max(self.row(cy0)).max(0), y1.min(self.row(cy1)).min(self.height));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let Some(tex) = self.texture(sffs, (c.sff, c.spr)) else { return };
        let Some(slot) = self.clut(frame, sffs, c.pal) else { return };

        // TEX0: textura de 8 bits, alfa de la paleta, color tal cual (DECAL).
        let tex0 = tex.tbp as u64
            | (tex.tbw as u64) << 14
            | PSMT8 << 20
            | (tex.tw as u64) << 26
            | (tex.th as u64) << 30
            | 1 << 34
            | 1 << 35
            | ((self.clut_base + slot * 4) as u64) << 37
            | 1 << 61;
        if tex0 != self.last_tex0 {
            self.push(TEX0_1, tex0);
            self.last_tex0 = tex0;
        }
        // Texel que corresponde al centro de cada píxel de los extremos.
        let uv = |px: i32, o: f32, s: f32, a: f32, lo: i32, hi: i32| -> u64 {
            let t = ((px as f32 + 0.5 - o) / s + a).clamp(lo as f32, hi as f32);
            (t * 16.0) as u64
        };
        let (ua, ub) = (uv(x0, c.x, c.sx, ax, u0, u1), uv(x1, c.x, c.sx, ax, u0, u1));
        let (va, vb) = (uv(y0, oy, osy, ay, v0, v1), uv(y1, oy, osy, ay, v0, v1));
        let (first, second) = Self::blend(c.trans);
        let passes = [first, second.or(Some(u64::MAX))];
        for a in passes.into_iter().flatten() {
            let blend = a != u64::MAX;
            if blend {
                self.set_alpha(a);
            }
            self.push(PRIM, 6 | 1 << 4 | (blend as u64) << 6 | 1 << 8);
            self.push(UV, ua | va << 16);
            self.xy(x0, y0);
            self.push(UV, ub | vb << 16);
            self.xy(x1, y1);
        }
        self.sprites += 1;
    }

    /// Olvida las texturas y paletas subidas: los sprites han cambiado.
    pub fn reset_cache(&mut self) {
        self.flush();
        self.tex.clear();
        self.tex_next = self.tex_base;
        self.pals.clear();
        self.pal_next = CLUT_DYN;
        self.last_tex0 = u64::MAX;
    }

    /// Dibuja un frame completo en el búfer oculto.
    pub fn render(&mut self, frame: &Frame, sffs: &[Sff]) {
        self.uploads = 0;
        self.sprites = 0;
        self.dyn_slot.clear();
        self.rect(0, 0, SCREEN_W, SCREEN_H, frame.clear, Trans::None);
        for cmd in &frame.cmds {
            match cmd {
                Cmd::Rect { x, y, w, h, rgba, trans } => self.rect(*x, *y, *w, *h, *rgba, *trans),
                Cmd::Sprite(c) => self.sprite(frame, sffs, c),
            }
        }
    }

    /// Espera al retrazo vertical y muestra lo dibujado.
    pub fn present(&mut self) {
        unsafe { write64(GS_CSR, 2) };
        self.push(FINISH, 0);
        self.flush();
        unsafe {
            let mut spins = 0u32;
            while read64(GS_CSR) & 2 == 0 && spins < 4_000_000 {
                spins += 1;
            }
            write64(GS_CSR, 8);
            while read64(GS_CSR) & 8 == 0 {}
            self.show(self.draw);
        }
        self.draw ^= 1;
        self.select_frame();
        self.frames = self.frames.wrapping_add(1);
    }
}
