//! Acceso a los archivos del juego. Hay tres orígenes:
//!
//! * el sistema de archivos (en el PC);
//! * un paquete en memoria, que va dentro del ejecutable (PS2, ELF suelto);
//! * un paquete en un archivo (PS2, en el disco): sólo se tiene en memoria
//!   su índice, y cada archivo se lee del disco cuando hace falta.
//!
//! Formato del paquete: `MPAK2\0\0\0`, número de archivos (u32), tamaño de
//! la cabecera (u32) y, por cada archivo, longitud del nombre (u16), nombre,
//! posición y tamaño (u32, respecto al principio del paquete). Después, los
//! datos.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::Deref;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

const MAGIC: &[u8; 8] = b"MPAK2\0\0\0";
const FIXED: usize = 16;

/// Contenido de un archivo: prestado del paquete en memoria (sin copiar) o
/// leído a un búfer propio.
pub enum Bytes {
    Static(&'static [u8]),
    Owned(Vec<u8>),
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Bytes::Static(s) => s,
            Bytes::Owned(v) => v,
        }
    }
}

enum Source {
    Memory(&'static [u8]),
    File(Mutex<fs::File>),
}

struct Archive {
    source: Source,
    index: HashMap<String, (usize, usize)>,
}

static ARCHIVE: OnceLock<Archive> = OnceLock::new();

/// Nombre canónico: minúsculas, barras normales y sin `.` ni `..`.
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split(['/', '\\']) {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    parts.join("/").to_ascii_lowercase()
}

fn u32at(data: &[u8], o: usize) -> Option<usize> {
    data.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
}

/// Tamaño de la cabecera (índice incluido) a partir de sus primeros 16 bytes.
fn header_size(fixed: &[u8]) -> Result<usize, String> {
    if fixed.len() < FIXED || &fixed[..8] != MAGIC {
        return Err("el paquete de datos no es válido".into());
    }
    Ok(u32at(fixed, 12).unwrap())
}

/// Índice de un paquete: nombre -> (posición, tamaño). `total` es el tamaño
/// del paquete entero, para comprobar que nada se sale.
fn parse_index(header: &[u8], total: usize) -> Result<HashMap<String, (usize, usize)>, String> {
    let count = u32at(header, 8).ok_or("paquete truncado")?;
    let mut index = HashMap::new();
    let mut o = FIXED;
    for _ in 0..count {
        let len = header.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).ok_or("paquete truncado")?;
        let name = header.get(o + 2..o + 2 + len).ok_or("paquete truncado")?;
        let (pos, size) = (u32at(header, o + 2 + len).ok_or("paquete truncado")?, u32at(header, o + 6 + len).ok_or("paquete truncado")?);
        if pos + size > total {
            return Err("paquete truncado".into());
        }
        index.insert(String::from_utf8_lossy(name).into_owned(), (pos, size));
        o += len + 10;
    }
    Ok(index)
}

/// Índice de un paquete que está entero en memoria.
pub fn index(data: &[u8]) -> Result<HashMap<String, (usize, usize)>, String> {
    let size = header_size(data)?;
    parse_index(data.get(..size).ok_or("paquete truncado")?, data.len())
}

/// Monta un paquete en memoria: a partir de aquí todas las lecturas salen de
/// él. Devuelve cuántos archivos contiene.
pub fn mount(data: &'static [u8]) -> Result<usize, String> {
    let index = index(data)?;
    let count = index.len();
    ARCHIVE.set(Archive { source: Source::Memory(data), index }).map_err(|_| "ya hay un paquete montado".to_string())?;
    Ok(count)
}

/// Monta un paquete que está en un archivo (por ejemplo, en el disco): se
/// lee su índice y el resto se irá leyendo a medida que se pida.
pub fn mount_file(path: &str) -> Result<usize, String> {
    let err = |e: io::Error| format!("{path}: {e}");
    let mut file = fs::File::open(path).map_err(err)?;
    let mut fixed = [0u8; FIXED];
    file.read_exact(&mut fixed).map_err(err)?;
    let size = header_size(&fixed)?;
    let mut header = vec![0u8; size];
    header[..FIXED].copy_from_slice(&fixed);
    file.read_exact(&mut header[FIXED..]).map_err(err)?;
    let index = parse_index(&header, usize::MAX)?;
    let count = index.len();
    ARCHIVE.set(Archive { source: Source::File(Mutex::new(file)), index }).map_err(|_| "ya hay un paquete montado".to_string())?;
    Ok(count)
}

pub fn mounted() -> bool {
    ARCHIVE.get().is_some()
}

/// Contenido de un archivo.
pub fn read(path: &str) -> io::Result<Bytes> {
    let Some(a) = ARCHIVE.get() else { return fs::read(path).map(Bytes::Owned) };
    let Some(&(pos, size)) = a.index.get(&normalize(path)) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no está en el paquete"));
    };
    match &a.source {
        Source::Memory(data) => Ok(Bytes::Static(&data[pos..pos + size])),
        Source::File(file) => {
            let mut f = file.lock().unwrap();
            f.seek(SeekFrom::Start(pos as u64))?;
            let mut buf = vec![0u8; size];
            f.read_exact(&mut buf)?;
            Ok(Bytes::Owned(buf))
        }
    }
}

/// Tamaño de un archivo, sin leerlo.
pub fn size(path: &str) -> io::Result<usize> {
    match ARCHIVE.get() {
        Some(a) => a.index.get(&normalize(path)).map(|e| e.1).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no está en el paquete")),
        None => fs::metadata(path).map(|m| m.len() as usize),
    }
}

/// Un trozo de un archivo: `len` bytes a partir de `off`. Es lo que permite
/// usar archivos más grandes que la memoria.
pub fn read_range(path: &str, off: usize, len: usize) -> io::Result<Bytes> {
    let out_of_range = || io::Error::new(io::ErrorKind::UnexpectedEof, "trozo fuera del archivo");
    let from_file = |f: &mut fs::File, at: usize| -> io::Result<Bytes> {
        f.seek(SeekFrom::Start(at as u64))?;
        let mut buf = vec![0u8; len];
        f.read_exact(&mut buf)?;
        Ok(Bytes::Owned(buf))
    };
    let Some(a) = ARCHIVE.get() else { return from_file(&mut fs::File::open(path)?, off) };
    let Some(&(pos, size)) = a.index.get(&normalize(path)) else {
        return Err(io::Error::new(io::ErrorKind::NotFound, "no está en el paquete"));
    };
    if off + len > size {
        return Err(out_of_range());
    }
    match &a.source {
        Source::Memory(data) => Ok(Bytes::Static(&data[pos + off..pos + off + len])),
        Source::File(file) => from_file(&mut file.lock().unwrap(), pos + off),
    }
}

pub fn exists(path: &str) -> bool {
    match ARCHIVE.get() {
        Some(a) => a.index.contains_key(&normalize(path)),
        None => fs::metadata(path).is_ok(),
    }
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = format!("{prefix}/{}", e.file_name().to_string_lossy());
        if e.file_type()?.is_dir() {
            walk(&e.path(), &name, out)?;
        } else {
            out.push((normalize(&name), fs::read(e.path())?));
        }
    }
    Ok(())
}

/// Empaqueta carpetas enteras: cada `(carpeta, prefijo)` aporta sus archivos
/// con ese prefijo delante. Si un nombre se repite, gana el último.
pub fn pack(dirs: &[(&Path, &str)]) -> io::Result<Vec<u8>> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (dir, prefix) in dirs {
        let mut found = Vec::new();
        walk(dir, prefix, &mut found)?;
        for f in found {
            files.retain(|(n, _)| *n != f.0);
            files.push(f);
        }
    }
    // Los datos empiezan en un múltiplo de 2048 (un sector de disco).
    let header: usize = (FIXED + files.iter().map(|(n, _)| n.len() + 10).sum::<usize>()).div_ceil(2048) * 2048;
    let mut out = Vec::with_capacity(header + files.iter().map(|f| f.1.len()).sum::<usize>());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    out.extend_from_slice(&(header as u32).to_le_bytes());
    let mut pos = header;
    for (name, data) in &files {
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(pos as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        pos += data.len();
    }
    out.resize(header, 0);
    for (_, data) in &files {
        out.extend_from_slice(data);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_normalized() {
        assert_eq!(normalize("game/chars/KFM/../kfm/./Kfm.DEF"), "game/chars/kfm/kfm.def");
        assert_eq!(normalize("game\\data\\fight.def"), "game/data/fight.def");
        assert_eq!(normalize("./a//b/"), "a/b");
    }

    #[test]
    fn pack_and_index_round_trip() {
        let dir = std::env::temp_dir().join(format!("mugen-vfs-{}", std::process::id()));
        let (a, b) = (dir.join("a"), dir.join("b"));
        fs::create_dir_all(a.join("Sub")).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("uno.txt"), b"uno").unwrap();
        fs::write(a.join("Sub").join("DOS.bin"), [1u8, 2, 3, 4]).unwrap();
        // El mismo nombre en la segunda carpeta sustituye al de la primera.
        fs::write(b.join("UNO.TXT"), b"otro").unwrap();
        let pak = pack(&[(a.as_path(), "game"), (b.as_path(), "game")]).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        let idx = index(&pak).unwrap();
        assert_eq!(idx.len(), 2);
        let get = |name: &str| {
            let (pos, size) = idx[name];
            &pak[pos..pos + size]
        };
        assert_eq!(get("game/uno.txt"), b"otro");
        assert_eq!(get("game/sub/dos.bin"), [1, 2, 3, 4]);
        assert!(index(&pak[..pak.len() - 1]).is_err());
        assert!(index(b"no es un paquete").is_err());
    }
}
