//! Opciones guardadas en la tarjeta de memoria (ranura 1).
//!
//! `std::fs` de PS2Rust carga los módulos de la tarjeta de la BIOS la primera
//! vez que se nombra `mc0:` y sirve para leer y listar; para escribir hay
//! aquí un cliente propio (ver [`Fio`]). Dónde va cada cosa:
//!
//! ```text
//! mc0:/RUSTGEN/options.cfg   las opciones, en texto
//! mc0:/RUSTGEN/icon.sys      nombre y aspecto de la partida en el navegador
//! mc0:/RUSTGEN/rustgen.icn   el icono
//! ```

use crate::iop::{uncached, Aligned, Rpc};
use std::fs;

const DIR: &str = "mc0:/RUSTGEN";
const FILE: &str = "mc0:/RUSTGEN/options.cfg";
const ICON_SYS: &str = "mc0:/RUSTGEN/icon.sys";
const ICON: &str = "mc0:/RUSTGEN/rustgen.icn";
const ICON_NAME: &str = "rustgen.icn";

/// Carga los módulos de la tarjeta de memoria: lo hace `std` la primera vez
/// que se nombra `mc0:`, haya tarjeta o no. Con ellos queda cargado SIO2MAN,
/// que también usan los mandos.
pub fn init() {
    match fs::read_dir("mc0:/") {
        Ok(_) => println!("tarjeta: ranura 1 lista"),
        Err(e) => println!("tarjeta: ranura 1 sin tarjeta ({e})"),
    }
}

/// Cliente mínimo del servidor de archivos de la BIOS (FILEIO), sólo para
/// escribir en la tarjeta.
///
/// `std::fs` no sirve para eso: antes de crear nada pregunta si la ruta
/// existe, y con el controlador de tarjetas de la BIOS esa pregunta
/// (`getstat`) responde siempre "sí, y es un directorio", exista o no. Leer
/// y listar sí funcionan con `std::fs`.
struct Fio {
    rpc: Rpc,
    bounce: *mut u8,
}

const SID_FILEIO: u32 = 0x8000_0001;
const F_OPEN: u32 = 0;
const F_CLOSE: u32 = 1;
const F_WRITE: u32 = 3;
const F_REMOVE: u32 = 6;
const F_MKDIR: u32 = 7;
const F_RMDIR: u32 = 8;
const O_RDONLY: u32 = 0x0001;
const O_WRONLY: u32 = 0x0002;
const O_CREAT: u32 = 0x0200;
const O_TRUNC: u32 = 0x0400;
const CHUNK: usize = 8192;

impl Fio {
    fn new() -> Result<Fio, String> {
        let rpc = Rpc::bind(SID_FILEIO, 200).ok_or("el servidor de archivos no responde")?;
        let bounce = uncached(Box::leak(Box::new(Aligned::<CHUNK>([0; CHUNK]))).0.as_mut_ptr());
        Ok(Fio { rpc, bounce })
    }

    /// Pone una ruta, terminada en cero, a partir de la palabra `word`.
    fn put_path(&mut self, word: usize, path: &str) {
        let mut bytes = [0u8; 256];
        bytes[..path.len()].copy_from_slice(path.as_bytes());
        for (i, chunk) in bytes.chunks(4).enumerate() {
            self.rpc.set(word + i, u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
    }

    /// Llamada cuyo único argumento es una ruta.
    fn path_call(&mut self, fno: u32, path: &str) -> i32 {
        self.put_path(0, path);
        self.rpc.call(fno, 256, 16);
        self.rpc.get(0) as i32
    }

    fn open(&mut self, path: &str, flags: u32) -> i32 {
        self.rpc.set(0, flags);
        self.put_path(1, path);
        self.rpc.call(F_OPEN, 260, 16);
        self.rpc.get(0) as i32
    }

    fn close(&mut self, fd: i32) {
        self.rpc.set(0, fd as u32);
        self.rpc.call(F_CLOSE, 4, 16);
    }

    fn exists(&mut self, path: &str) -> bool {
        let fd = self.open(path, O_RDONLY);
        if fd >= 0 {
            self.close(fd);
        }
        fd >= 0
    }

    fn mkdir(&mut self, path: &str) -> Result<(), String> {
        match self.path_call(F_MKDIR, path) {
            r if r < 0 => Err(format!("crear {path}: error {r}")),
            _ => Ok(()),
        }
    }

    /// Borra un archivo. El FILEIO de la BIOS, tras borrar, sigue por error
    /// hasta `mkdir` y deja un directorio vacío con ese nombre: se quita.
    fn remove(&mut self, path: &str) {
        self.path_call(F_REMOVE, path);
        self.path_call(F_RMDIR, path);
    }

    /// Escribe un archivo entero, sustituyendo el que hubiera.
    fn write_file(&mut self, path: &str, data: &[u8]) -> Result<(), String> {
        if self.exists(path) {
            self.remove(path);
        }
        let fd = self.open(path, O_WRONLY | O_CREAT | O_TRUNC);
        if fd < 0 {
            return Err(format!("crear {path}: error {fd}"));
        }
        let mut result = Ok(());
        for chunk in data.chunks(CHUNK) {
            unsafe { std::ptr::copy_nonoverlapping(chunk.as_ptr(), self.bounce, chunk.len()) };
            for i in 0..8 {
                self.rpc.set(i, 0);
            }
            self.rpc.set(0, fd as u32);
            self.rpc.set(1, self.bounce as u32);
            self.rpc.set(2, chunk.len() as u32);
            self.rpc.call(F_WRITE, 32, 16);
            let n = self.rpc.get(0) as i32;
            if n != chunk.len() as i32 {
                result = Err(format!("escribir {path}: {n} de {} bytes", chunk.len()));
                break;
            }
        }
        self.close(fd);
        result
    }
}

/// Texto de las opciones guardadas, si hay tarjeta y hay partida.
pub fn load() -> Option<String> {
    fs::read_to_string(FILE).ok()
}

/// Guarda las opciones. La primera vez crea la carpeta y el icono.
pub fn save(text: &str) -> Result<(), String> {
    let mut fio = Fio::new()?;
    if fs::read_dir(DIR).is_err() {
        fio.mkdir(DIR)?;
    }
    if !fio.exists(ICON_SYS) {
        fio.write_file(ICON, &icon())?;
        fio.write_file(ICON_SYS, &icon_sys())?;
    }
    fio.write_file(FILE, text.as_bytes())
}

/// Borra la partida entera (lo usan las pruebas para no dejar rastro).
/// Devuelve si ya no queda nada.
pub fn remove() -> bool {
    let Ok(mut fio) = Fio::new() else { return false };
    for f in [FILE, ICON_SYS, ICON] {
        if fio.exists(f) {
            fio.remove(f);
        }
    }
    fio.path_call(F_RMDIR, DIR);
    fs::read_dir(DIR).is_err()
}

fn put_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn put_f32(v: &mut Vec<u8>, x: f32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// `icon.sys`: 964 bytes con el título (en Shift-JIS de ancho completo), los
/// colores de fondo, las luces y el nombre del archivo del icono.
fn icon_sys() -> Vec<u8> {
    let mut v = Vec::with_capacity(964);
    v.extend_from_slice(b"PS2D");
    // Byte donde se parte el título en dos líneas: tras "RUSTGEN".
    v.extend_from_slice(&[0, 0, 14, 0]);
    put_u32(&mut v, 0);
    // Transparencia del fondo y sus cuatro esquinas (r, g, b, a).
    put_u32(&mut v, 0x40);
    for color in [[0x30, 0x10, 0x50], [0x10, 0x10, 0x40], [0x10, 0x20, 0x60], [0x08, 0x08, 0x20]] {
        for c in color {
            put_u32(&mut v, c);
        }
        put_u32(&mut v, 0);
    }
    // Tres luces: direcciones y colores; después la luz ambiente.
    for dir in [[0.5f32, 0.5, 0.5], [0.0, -0.4, -0.1], [-0.5, -0.5, 0.5]] {
        for d in dir {
            put_f32(&mut v, d);
        }
        put_f32(&mut v, 0.0);
    }
    for color in [[0.6f32, 0.6, 0.6], [0.4, 0.4, 0.4], [0.3, 0.3, 0.3]] {
        for c in color {
            put_f32(&mut v, c);
        }
        put_f32(&mut v, 0.0);
    }
    for c in [0.4f32, 0.4, 0.4, 0.0] {
        put_f32(&mut v, c);
    }
    // Título: 68 bytes. Las letras ASCII pasan a su forma de ancho completo.
    let mut title = Vec::new();
    for ch in "RUSTGENOptions".bytes() {
        let wide: u16 = match ch {
            b'A'..=b'Z' => 0x8260 + (ch - b'A') as u16,
            b'a'..=b'z' => 0x8281 + (ch - b'a') as u16,
            b'0'..=b'9' => 0x824f + (ch - b'0') as u16,
            _ => 0x8140,
        };
        title.extend_from_slice(&wide.to_be_bytes());
    }
    title.resize(68, 0);
    v.extend_from_slice(&title);
    // El mismo icono para la lista, para copiar y para borrar.
    for _ in 0..3 {
        let mut name = ICON_NAME.as_bytes().to_vec();
        name.resize(64, 0);
        v.extend_from_slice(&name);
    }
    v.resize(964, 0);
    v
}

/// El icono: un cuadrado con una textura de 128×128 generada aquí (un
/// degradado con marco), visible por las dos caras.
fn icon() -> Vec<u8> {
    let mut v = Vec::new();
    // Cabecera: versión, formas de animación, textura sin comprimir, 1.0 y
    // número de vértices.
    // Coordenadas en las unidades de los iconos de otros juegos: unos
    // 11000 de alto, con el suelo en y = 0 y la altura hacia los negativos.
    let quad = [(-5500.0f32, -11500.0f32, 0.0f32, 0.0f32), (5500.0, -11500.0, 1.0, 0.0), (5500.0, -500.0, 1.0, 1.0), (-5500.0, -500.0, 0.0, 1.0)];
    let order = [0usize, 1, 2, 0, 2, 3, 2, 1, 0, 3, 2, 0];
    put_u32(&mut v, 0x0001_0000);
    put_u32(&mut v, 1);
    put_u32(&mut v, 0x07);
    put_f32(&mut v, 1.0);
    put_u32(&mut v, order.len() as u32);
    let put_i16 = |v: &mut Vec<u8>, x: f32| v.extend_from_slice(&(x as i16).to_le_bytes());
    for (n, &i) in order.iter().enumerate() {
        let (x, y, u, t) = quad[i];
        // Posición, normal (×4096), coordenadas de textura (×4096) y color.
        for c in [x, y, 0.0, 1024.0] {
            put_i16(&mut v, c);
        }
        let nz = if n < 6 { -4096.0 } else { 4096.0 };
        for c in [0.0, 0.0, nz, 0.0] {
            put_i16(&mut v, c);
        }
        put_i16(&mut v, u * 4096.0);
        put_i16(&mut v, t * 4096.0);
        v.extend_from_slice(&[0x80, 0x80, 0x80, 0x80]);
    }
    // Animación: un solo cuadro con la forma 0.
    put_u32(&mut v, 1);
    put_u32(&mut v, 1);
    put_f32(&mut v, 1.0);
    put_u32(&mut v, 0);
    put_u32(&mut v, 1);
    put_u32(&mut v, 0);
    put_u32(&mut v, 1);
    put_f32(&mut v, 1.0);
    put_f32(&mut v, 1.0);
    // Textura: 128×128 en 16 bits (A1 B5 G5 R5).
    for y in 0..128u32 {
        for x in 0..128u32 {
            let edge = x < 6 || y < 6 || x >= 122 || y >= 122;
            // Dos franjas diagonales, como un golpe.
            let stripe = (x + y) / 16 % 2 == 0;
            let (r, g, b) = match (edge, stripe) {
                (true, _) => (31, 31, 31),
                (false, true) => (28, 6 + y / 8, 4),
                (false, false) => (6, 4, 14 + x / 10),
            };
            let px = 0x8000 | (b.min(31) << 10) | (g.min(31) << 5) | r.min(31);
            v.extend_from_slice(&(px as u16).to_le_bytes());
        }
    }
    v
}

/// Lista lo que hay en la carpeta de la partida (para las pruebas).
pub fn list() -> Vec<String> {
    let dir = match fs::read_dir(DIR) {
        Ok(d) => d,
        Err(e) => return vec![format!("(no se puede listar: {e})")],
    };
    let mut out: Vec<String> = dir
        .flatten()
        .map(|e| format!("{} ({} bytes)", e.file_name().to_string_lossy(), e.metadata().map_or(0, |m| m.len())))
        .filter(|name| !name.starts_with('.'))
        .collect();
    out.sort();
    out
}
