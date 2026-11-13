//! El procesador de E/S (IOP): llamadas RPC a sus módulos, y carga de
//! módulos desde la memoria del EE.
//!
//! Los controladores de sonido van dentro del ejecutable. El servidor
//! LOADFILE de la BIOS sólo sabe cargar módulos desde un archivo, así que
//! antes se le añade la función que falta ("cargar desde un búfer") con el
//! parche clásico de PS2SDK (`sbv_patch_enable_lmb`): se copia una rutina
//! pequeña a la RAM del IOP y se engancha en su tabla de funciones.

use ps2::kernel::syscall;
use std::os::ps2::{load_iop_module, SifRpcClient};

/// Búfer de intercambio con el IOP: alineado a 64 bytes, como exige el DMA.
#[repr(C, align(64))]
pub struct Aligned<const N: usize>(pub [u8; N]);

/// Puntero al espejo sin caché de la RAM: el DMA no pasa por la caché.
pub fn uncached<T>(p: *mut T) -> *mut T {
    (p as usize | 0x2000_0000) as *mut T
}

pub struct Rpc {
    client: Box<SifRpcClient>,
    buf: *mut u32,
}

impl Rpc {
    /// Liga un cliente al servidor `sid`. El búfer de 512 bytes vive para
    /// siempre: el IOP escribe en él.
    pub fn bind(sid: u32, attempts: u32) -> Option<Rpc> {
        let mut client = Box::new(SifRpcClient::new());
        if !client.bind(sid, attempts) {
            return None;
        }
        let buf = Box::leak(Box::new(Aligned::<512>([0; 512])));
        Some(Rpc { client, buf: uncached(buf.0.as_mut_ptr() as *mut u32) })
    }

    pub fn set(&mut self, i: usize, v: u32) {
        unsafe { self.buf.add(i).write_volatile(v) }
    }

    pub fn get(&self, i: usize) -> u32 {
        unsafe { self.buf.add(i).read_volatile() }
    }

    pub fn clear(&mut self) {
        for i in 0..128 {
            self.set(i, 0);
        }
    }

    /// Dirección física del búfer.
    pub fn addr(&self) -> u32 {
        self.buf as u32 & 0x1fff_ffff
    }

    /// Llama a `fno` enviando `send` bytes del búfer y recibiendo `recv`.
    pub fn call(&mut self, fno: u32, send: u32, recv: u32) {
        let p = self.buf as *mut u8;
        unsafe { self.client.call(fno, p, send, p, recv) }
    }
}

pub fn load_module(path: &str) -> bool {
    match load_iop_module(path) {
        Ok(_) => true,
        Err(e) => {
            println!("iop: no pude cargar {path}: {e}");
            false
        }
    }
}

#[repr(C)]
struct SifDma {
    src: u32,
    dest: u32,
    size: u32,
    attr: u32,
}

/// Copia `size` bytes de la RAM del EE (dirección física, alineada a 16) a
/// la RAM del IOP y espera a que la copia termine.
pub fn copy_to_iop(src: u32, dest: u32, size: u32) {
    let d = SifDma { src, dest, size, attr: 0 };
    // SifSetDma devuelve 0 si la cola está llena.
    let id = loop {
        match unsafe { syscall(0x77, &d as *const SifDma as u32, 1, 0, 0) } {
            0 => std::thread::yield_now(),
            id => break id,
        }
    };
    // SifDmaStat: negativo cuando la transferencia ya terminó.
    while (unsafe { syscall(0x76, id, 0, 0, 0) } as i32) >= 0 {}
}

/// Reserva memoria en el IOP (servidor de heap de la BIOS).
pub fn alloc_iop(size: u32) -> Option<u32> {
    let mut rpc = Rpc::bind(0x8000_0003, 100)?;
    rpc.set(0, size);
    rpc.call(1, 4, 4);
    match rpc.get(0) {
        0 => None,
        addr => Some(addr),
    }
}

const SID_LOADFILE: u32 = 0x8000_0006;
const LF_SET_ADDR: u32 = 2;
const LF_GET_ADDR: u32 = 3;
const LF_MOD_BUF_LOAD: u32 = 6;

/// Tamaño del búfer de paso entre el EE y el IOP.
pub const CHUNK: usize = 0x8000;

/// Acceso a la memoria del IOP y carga de módulos desde memoria.
pub struct Iop {
    lf: Rpc,
    /// Búfer de paso: en el IOP y su origen en el EE (sin caché).
    pub buf: u32,
    pub stage: *mut u8,
    patched: bool,
}

/// Rutina que LOADFILE ejecutará para "cargar módulo desde un búfer": llama
/// a LoadModuleBuffer y StartModule de MODLOAD. Los `jal` y la dirección
/// del resultado se rellenan al instalarla.
const LMB_STUB: [u32; 32] = [
    0x27bdffd8, // addiu sp, -40
    0xafb00018, // sw    s0, 0x18(sp)
    0xafbf0020, // sw    ra, 0x20(sp)
    0x00808021, // move  s0, a0
    0x8c840000, // lw    a0, 0(a0)
    0x0c000000, // jal   LoadModuleBuffer
    0xafb1001c, // sw    s1, 0x1c(sp)
    0x3c110000, // lui   s1, HI(resultado)
    0x04400008, // bltz  v0, 1f
    0x36310000, // ori   s1, LO(resultado)
    0x00402021, // move  a0, v0
    0x26250008, // addiu a1, s1, 8
    0x8e060004, // lw    a2, 4(s0)
    0x26070104, // addiu a3, s0, 0x104
    0x26280004, // addiu t0, s1, 4
    0x0c000000, // jal   StartModule
    0xafa80010, // sw    t0, 0x10(sp)
    0xae220000, // 1: sw v0, 0(s1)
    0x02201021, // move  v0, s1
    0x8fbf0020, // lw    ra, 0x20(sp)
    0x8fb1001c, // lw    s1, 0x1c(sp)
    0x8fb00018, // lw    s0, 0x18(sp)
    0x03e00008, // jr    ra
    0x27bd0028, // addiu sp, 40
    0,
    0,
    0x7962424c, // "LBbyEE"
    0x00004545,
    0,
    0,
    0,
    0,
];

impl Iop {
    pub fn init() -> Option<Iop> {
        let lf = Rpc::bind(SID_LOADFILE, 200)?;
        let mut iop = Iop { lf, buf: 0, stage: std::ptr::null_mut(), patched: false };
        iop.patched = iop.enable_buffer_loading().is_some();
        if !iop.patched {
            println!("iop: no pude habilitar la carga de módulos desde memoria");
        }
        iop.buf = alloc_iop(CHUNK as u32)?;
        iop.stage = uncached(Box::leak(Box::new(Aligned::<CHUNK>([0; CHUNK]))).0.as_mut_ptr());
        Some(iop)
    }

    /// Lee una palabra de la memoria del IOP.
    pub fn peek(&mut self, addr: u32) -> u32 {
        self.lf.set(0, addr);
        self.lf.set(1, 2);
        self.lf.call(LF_GET_ADDR, 8, 4);
        self.lf.get(0)
    }

    /// Escribe una palabra en la memoria del IOP.
    fn poke(&mut self, addr: u32, value: u32) {
        self.lf.set(0, addr);
        self.lf.set(1, 2);
        self.lf.set(2, value);
        self.lf.call(LF_SET_ADDR, 12, 4);
    }

    /// Los primeros `N` bytes de una cadena de la memoria del IOP.
    fn text<const N: usize>(&mut self, addr: u32) -> [u8; N] {
        let mut out = [0u8; N];
        for (i, chunk) in out.chunks_mut(4).enumerate() {
            let w = self.peek(addr + i as u32 * 4).to_le_bytes();
            chunk.copy_from_slice(&w[..chunk.len()]);
        }
        out
    }

    /// Información de un módulo cargado, por nombre: (inicio, tamaño) de su código.
    pub fn module(&mut self, name: &[u8]) -> Option<(u32, u32)> {
        // La lista de módulos empieza en 0x800 (SYSMEM).
        let mut info = 0x800;
        for _ in 0..64 {
            let name_ptr = self.peek(info + 4);
            if self.text::<24>(name_ptr).starts_with(name) {
                return Some((self.peek(info + 24), self.peek(info + 28)));
            }
            info = self.peek(info);
            if info == 0 {
                break;
            }
        }
        None
    }

    /// Dirección de la tabla de exportaciones de una biblioteca del IOP.
    fn export_table(&mut self, name: &[u8]) -> Option<u32> {
        // LOADCORE (el segundo módulo) guarda la lista de bibliotecas. Su
        // propia tabla está al final de su código: se reconoce porque su
        // enlace "anterior" apunta a la de SYSMEM (0x830).
        let core = self.peek(0x800);
        let end = self.peek(core + 24) + self.peek(core + 28);
        let lib = (0..128).map(|i| end - 512 + i * 4).find(|&a| self.peek(a) == 0x830 && &self.text::<8>(a + 12) == b"loadcore")?;
        // Su exportación 3 devuelve la lista: `lui v0, X; addiu v0, v0, Y`.
        let func = self.peek(lib + 20 + 3 * 4);
        let (hi, lo) = (self.peek(func), self.peek(func + 4));
        if hi & 0xffff_0000 != 0x3c02_0000 || lo & 0xffff_0000 != 0x2442_0000 {
            return None;
        }
        let list = ((hi & 0xffff) << 16).wrapping_add(lo as u16 as i16 as i32 as u32);
        let mut cur = self.peek(list);
        for _ in 0..96 {
            if cur == 0 {
                break;
            }
            if self.text::<8>(cur + 12).starts_with(name) {
                return Some(cur + 20);
            }
            cur = self.peek(cur);
        }
        None
    }

    /// Añade a LOADFILE la función 6: cargar un módulo desde un búfer.
    fn enable_buffer_loading(&mut self) -> Option<()> {
        let exports = self.export_table(b"modload")?;
        let start_module = self.peek(exports + 8 * 4);
        let load_buffer = self.peek(exports + 10 * 4);
        if start_module == 0 || load_buffer == 0 {
            return None;
        }
        let (text, size) = self.module(b"LoadModuleByEE")?;
        if size < 0x4c4 + 128 {
            return None;
        }
        // El despachador RPC de LOADFILE empieza comprobando el número de
        // función (`sltiu v0, a0, N`) y después salta por una tabla. En las
        // BIOS antiguas está en +0x4c4 y admite 6 funciones (0 a 5); las
        // recientes ya traen la séptima, que es la que hace falta.
        let is_dispatch = |d: &[u32]| d[0] == 0x27bd_ffe8 && d[1] & 0xffff_fff0 == 0x2c82_0000 && d[2] == 0x1440_0003 && d[3] == 0xafbf_0010;
        let read = |iop: &mut Iop, at: u32| -> Vec<u32> { (0..10).map(|i| iop.peek(at + i * 4)).collect() };
        let mut dispatch = text + 0x4c4;
        let mut d = read(self, dispatch);
        if !is_dispatch(&d) {
            // Otra versión de LOADFILE: se busca el despachador en su código.
            let mut found = None;
            let mut prev = 0;
            for off in (0..size.saturating_sub(40)).step_by(4) {
                let w = self.peek(text + off);
                if prev == 0x27bd_ffe8 && w & 0xffff_fff0 == 0x2c82_0000 {
                    found = Some(text + off - 4);
                    break;
                }
                prev = w;
            }
            dispatch = found?;
            d = read(self, dispatch);
            if !is_dispatch(&d) {
                return None;
            }
        }
        if d[1] & 0xf >= 7 {
            return Some(());
        }
        let table_end = ((d[7] & 0xffff) << 16).wrapping_add(d[9] as u16 as i16 as i32 as u32) + 0x18;

        let stub = alloc_iop(128)?;
        let result = stub + 96;
        let jal = |addr: u32| 0x0c00_0000 | (0x03ff_ffff & (addr >> 2));
        let mut code = LMB_STUB;
        code[5] = jal(load_buffer);
        code[7] |= result >> 16;
        code[9] |= result & 0xffff;
        code[15] = jal(start_module);
        let src = uncached(Box::leak(Box::new(Aligned::<128>([0; 128]))).0.as_mut_ptr() as *mut u32);
        for (i, w) in code.iter().enumerate() {
            unsafe { src.add(i).write_volatile(*w) };
        }
        copy_to_iop(src as u32 & 0x1fff_ffff, stub, 128);
        // Entrada 6 de la tabla de saltos, y el límite del número de función.
        self.poke(table_end, stub);
        self.poke(dispatch + 4, 0x2c82_0007);
        Some(())
    }

    /// Carga y arranca un módulo IRX que está en la memoria del EE.
    /// Devuelve el identificador del módulo (negativo si falló).
    pub fn exec_module(&mut self, name: &str, irx: &[u8]) -> i32 {
        if !self.patched || irx.len() > CHUNK {
            println!("iop: no puedo cargar {name} desde memoria");
            return -1;
        }
        unsafe { std::ptr::copy_nonoverlapping(irx.as_ptr(), self.stage, irx.len()) };
        copy_to_iop(self.stage as u32 & 0x1fff_ffff, self.buf, (irx.len() as u32 + 15) & !15);
        self.lf.clear();
        self.lf.set(0, self.buf);
        self.lf.call(LF_MOD_BUF_LOAD, 512, 8);
        let id = self.lf.get(0) as i32;
        if id < 0 {
            println!("iop: {name} no cargó ({id})");
        }
        id
    }
}
