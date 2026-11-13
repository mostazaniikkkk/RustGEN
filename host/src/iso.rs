//! Imagen de disco de PlayStation 2: un ISO 9660 mínimo con `SYSTEM.CNF`,
//! el ejecutable y, opcionalmente, el paquete de datos (`GAME.PAK`).

const SECTOR: usize = 2048;

fn both16(v: u16) -> [u8; 4] {
    let (l, b) = (v.to_le_bytes(), v.to_be_bytes());
    [l[0], l[1], b[0], b[1]]
}

fn both32(v: u32) -> [u8; 8] {
    let (l, b) = (v.to_le_bytes(), v.to_be_bytes());
    [l[0], l[1], l[2], l[3], b[0], b[1], b[2], b[3]]
}

/// Entrada de directorio: nombre, sector, tamaño y si es carpeta.
fn record(name: &[u8], sector: u32, size: u32, dir: bool) -> Vec<u8> {
    let len = 33 + name.len() + (name.len() + 1) % 2;
    let mut r = vec![0u8; len];
    r[0] = len as u8;
    r[2..10].copy_from_slice(&both32(sector));
    r[10..18].copy_from_slice(&both32(size));
    // Fecha: 1 de enero de 2000.
    r[18..25].copy_from_slice(&[100, 1, 1, 0, 0, 0, 0]);
    r[25] = if dir { 2 } else { 0 };
    r[28..32].copy_from_slice(&both16(1));
    r[32] = name.len() as u8;
    r[33..33 + name.len()].copy_from_slice(name);
    r
}

fn padded(text: &str, len: usize) -> Vec<u8> {
    let mut v = text.as_bytes().to_vec();
    v.resize(len, b' ');
    v
}

/// Construye la imagen. `boot_name` es el nombre del ejecutable en el disco,
/// con forma de código de juego (`XXXX_000.00`). `pak` es el paquete de
/// datos, si no va dentro del ELF.
pub fn build(elf: &[u8], boot_name: &str, title: &str, pak: Option<&[u8]>) -> Vec<u8> {
    let cnf = format!("BOOT2 = cdrom0:{}{boot_name};1\r\nVER = 1.00\r\nVMODE = NTSC\r\n", '\\').into_bytes();
    let sectors = |n: usize| n.div_ceil(SECTOR) as u32;
    // Sectores: 16 descriptor, 17 fin, 18 y 19 tablas de rutas, 20 raíz.
    let (root, cnf_at) = (20u32, 21u32);
    let elf_at = cnf_at + sectors(cnf.len());
    let pak_at = elf_at + sectors(elf.len());
    let total = (pak_at + sectors(pak.map_or(0, <[u8]>::len))).max(2048);
    let mut img = vec![0u8; total as usize * SECTOR];

    // Directorio raíz: ".", ".." y los archivos en orden alfabético.
    let mut files = vec![(format!("{boot_name};1"), elf_at, elf.len() as u32), ("SYSTEM.CNF;1".to_string(), cnf_at, cnf.len() as u32)];
    if let Some(p) = pak {
        files.push(("GAME.PAK;1".to_string(), pak_at, p.len() as u32));
    }
    files.sort();
    let mut dir = record(&[0], root, SECTOR as u32, true);
    dir.extend(record(&[1], root, SECTOR as u32, true));
    for (name, at, size) in &files {
        dir.extend(record(name.as_bytes(), *at, *size, false));
    }
    let at = root as usize * SECTOR;
    img[at..at + dir.len()].copy_from_slice(&dir);

    // Tablas de rutas (sólo la raíz), en los dos órdenes de bytes.
    let mut table = vec![1u8, 0];
    table.extend_from_slice(&root.to_le_bytes());
    table.extend_from_slice(&[1, 0, 0, 0]);
    img[18 * SECTOR..18 * SECTOR + 10].copy_from_slice(&table);
    let mut table = vec![1u8, 0];
    table.extend_from_slice(&root.to_be_bytes());
    table.extend_from_slice(&[0, 1, 0, 0]);
    img[19 * SECTOR..19 * SECTOR + 10].copy_from_slice(&table);

    // Descriptor de volumen primario.
    let mut pvd = vec![0u8; SECTOR];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    pvd[8..40].copy_from_slice(&padded("PLAYSTATION", 32));
    pvd[40..72].copy_from_slice(&padded(title, 32));
    pvd[80..88].copy_from_slice(&both32(total));
    pvd[120..124].copy_from_slice(&both16(1));
    pvd[124..128].copy_from_slice(&both16(1));
    pvd[128..132].copy_from_slice(&both16(SECTOR as u16));
    pvd[132..140].copy_from_slice(&both32(10));
    pvd[140..144].copy_from_slice(&18u32.to_le_bytes());
    pvd[148..152].copy_from_slice(&19u32.to_be_bytes());
    pvd[156..190].copy_from_slice(&record(&[0], root, SECTOR as u32, true));
    pvd[190..318].copy_from_slice(&padded(title, 128));
    pvd[318..446].copy_from_slice(&padded("", 128));
    pvd[446..574].copy_from_slice(&padded("", 128));
    pvd[574..702].copy_from_slice(&padded("PLAYSTATION", 128));
    pvd[702..813].copy_from_slice(&padded("", 111));
    // Fechas: "sin especificar".
    for i in 0..4 {
        let d = 813 + i * 17;
        pvd[d..d + 16].copy_from_slice(b"0000000000000000");
    }
    pvd[881] = 1;
    img[16 * SECTOR..17 * SECTOR].copy_from_slice(&pvd);
    // Fin de los descriptores.
    let end = 17 * SECTOR;
    img[end] = 255;
    img[end + 1..end + 6].copy_from_slice(b"CD001");
    img[end + 6] = 1;

    let at = cnf_at as usize * SECTOR;
    img[at..at + cnf.len()].copy_from_slice(&cnf);
    let at = elf_at as usize * SECTOR;
    img[at..at + elf.len()].copy_from_slice(elf);
    if let Some(p) = pak {
        let at = pak_at as usize * SECTOR;
        img[at..at + p.len()].copy_from_slice(p);
    }
    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_has_boot_files_where_the_directory_says() {
        let elf: Vec<u8> = (0..5000u32).map(|i| (i * 7) as u8).collect();
        let pak = vec![9u8; 3000];
        let img = build(&elf, "MUGN_000.01", "MUGEN", Some(&pak));
        assert_eq!(img.len() % SECTOR, 0);
        // Sin paquete, sólo el ejecutable y SYSTEM.CNF.
        assert!(build(&elf, "MUGN_000.01", "MUGEN", None).len() <= img.len());
        assert_eq!(&img[16 * SECTOR + 1..16 * SECTOR + 6], b"CD001");
        assert_eq!(img[17 * SECTOR], 255);
        // Recorre el directorio raíz y comprueba el contenido de cada archivo.
        let root = &img[20 * SECTOR..21 * SECTOR];
        let mut found = Vec::new();
        let mut o = 0;
        while root[o] != 0 {
            let len = root[o] as usize;
            let name = String::from_utf8_lossy(&root[o + 33..o + 33 + root[o + 32] as usize]).into_owned();
            let at = u32::from_le_bytes(root[o + 2..o + 6].try_into().unwrap()) as usize * SECTOR;
            let size = u32::from_le_bytes(root[o + 10..o + 14].try_into().unwrap()) as usize;
            found.push((name, img[at..at + size].to_vec()));
            o += len;
        }
        assert_eq!(found.len(), 5);
        assert_eq!(found[2].0, "GAME.PAK;1");
        assert_eq!(found[2].1, pak);
        assert_eq!(found[3].0, "MUGN_000.01;1");
        assert_eq!(found[3].1, elf);
        assert_eq!(found[4].0, "SYSTEM.CNF;1");
        assert!(String::from_utf8_lossy(&found[4].1).starts_with("BOOT2 = cdrom0:\\MUGN_000.01;1"));
    }
}
