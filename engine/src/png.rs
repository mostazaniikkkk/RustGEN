//! Imágenes PNG con paleta (los sprites "PNG8" de SFF v2): descompresión
//! zlib/deflate y filtros de fila. Sólo se obtienen los índices de color;
//! la paleta la pone el SFF.

/// Bits de la tabla rápida de Huffman.
const FAST: u32 = 9;

struct Bits<'a> {
    src: &'a [u8],
    pos: usize,
    acc: u32,
    n: u32,
}

impl Bits<'_> {
    /// Garantiza al menos 16 bits en el acumulador (con ceros al final).
    fn fill(&mut self) {
        while self.n <= 16 {
            let b = self.src.get(self.pos).copied().unwrap_or(0);
            self.pos += 1;
            self.acc |= (b as u32) << self.n;
            self.n += 8;
        }
    }

    fn take(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }
        self.fill();
        let v = self.acc & ((1 << count) - 1);
        self.acc >>= count;
        self.n -= count;
        v
    }

    fn done(&self) -> bool {
        self.pos > self.src.len() + 4
    }
}

struct Huffman {
    /// Índice: los próximos `FAST` bits. Valor: símbolo | longitud << 9
    /// (0 = el código es más largo y hay que ir bit a bit).
    fast: Vec<u16>,
    count: [u16; 16],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Huffman {
        let mut count = [0u16; 16];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        let mut offs = [0u16; 16];
        for i in 1..16 {
            offs[i] = offs[i - 1] + count[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        let mut next = offs;
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[next[l as usize] as usize] = sym as u16;
                next[l as usize] += 1;
            }
        }
        // Códigos canónicos; en el flujo van con los bits al revés.
        let mut fast = vec![0u16; 1 << FAST];
        let mut code = 0u32;
        let mut idx = 0usize;
        for len in 1..=FAST {
            for _ in 0..count[len as usize] {
                let sym = symbols[idx];
                idx += 1;
                let rev = code.reverse_bits() >> (32 - len);
                let mut k = rev;
                while k < 1 << FAST {
                    fast[k as usize] = sym | (len as u16) << 9;
                    k += 1 << len;
                }
                code += 1;
            }
            code <<= 1;
        }
        Huffman { fast, count, symbols }
    }

    fn decode(&self, b: &mut Bits) -> Option<u16> {
        b.fill();
        let e = self.fast[(b.acc & ((1 << FAST) - 1)) as usize];
        if e != 0 {
            let len = (e >> 9) as u32;
            b.acc >>= len;
            b.n -= len;
            return Some(e & 0x1ff);
        }
        // Código largo: bit a bit.
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= b.take(1) as i32;
            let count = self.count[len] as i32;
            if code - count < first {
                return self.symbols.get((index + code - first) as usize).copied();
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        None
    }
}

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] =
    [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// Descomprime un flujo zlib. `size` es el tamaño esperado (no se pasa de él).
pub fn inflate(src: &[u8], size: usize) -> Option<Vec<u8>> {
    let mut b = Bits { src: src.get(2..)?, pos: 0, acc: 0, n: 0 };
    let mut out: Vec<u8> = Vec::with_capacity(size);
    loop {
        let last = b.take(1) != 0;
        match b.take(2) {
            0 => {
                // Bloque sin comprimir: alineado a byte, longitud y su complemento.
                let skip = b.n % 8;
                b.take(skip);
                let len = b.take(16) as usize;
                b.take(16);
                for _ in 0..len {
                    out.push(b.take(8) as u8);
                }
            }
            kind @ (1 | 2) => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [8u8; 288];
                    l[144..256].fill(9);
                    l[256..280].fill(7);
                    (Huffman::new(&l), Huffman::new(&[5u8; 30]))
                } else {
                    let (hlit, hdist, hclen) = (b.take(5) as usize + 257, b.take(5) as usize + 1, b.take(4) as usize + 4);
                    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                    let mut cl = [0u8; 19];
                    for &o in ORDER.iter().take(hclen) {
                        cl[o] = b.take(3) as u8;
                    }
                    let clh = Huffman::new(&cl);
                    let mut lens = vec![0u8; hlit + hdist];
                    let mut i = 0;
                    while i < lens.len() {
                        let (value, repeat) = match clh.decode(&mut b)? {
                            s @ 0..=15 => (s as u8, 1),
                            16 => (*lens.get(i.checked_sub(1)?)?, 3 + b.take(2) as usize),
                            17 => (0, 3 + b.take(3) as usize),
                            _ => (0, 11 + b.take(7) as usize),
                        };
                        for _ in 0..repeat {
                            *lens.get_mut(i)? = value;
                            i += 1;
                        }
                    }
                    (Huffman::new(&lens[..hlit]), Huffman::new(&lens[hlit..]))
                };
                loop {
                    let sym = lit.decode(&mut b)? as usize;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let k = sym - 257;
                        let len = *LEN_BASE.get(k)? as usize + b.take(*LEN_EXTRA.get(k)? as u32) as usize;
                        let d = dist.decode(&mut b)? as usize;
                        let back = *DIST_BASE.get(d)? as usize + b.take(*DIST_EXTRA.get(d)? as u32) as usize;
                        let start = out.len().checked_sub(back)?;
                        for i in 0..len {
                            let v = out[start + i];
                            out.push(v);
                        }
                    }
                    if b.done() || out.len() > size {
                        return None;
                    }
                }
            }
            _ => return None,
        }
        if last {
            return Some(out);
        }
        if b.done() {
            return None;
        }
    }
}

fn be32(d: &[u8], o: usize) -> usize {
    d.get(o..o + 4).map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]])) as usize
}

/// Decodifica un PNG con paleta (o en grises) a un índice por píxel.
/// Devuelve (ancho, alto, píxeles).
pub fn decode_indexed(d: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let start = d.windows(8).take(16).position(|w| w == b"\x89PNG\r\n\x1a\n")?;
    let mut o = start + 8;
    let (mut w, mut h, mut depth) = (0, 0, 8usize);
    let mut idat: Vec<u8> = Vec::new();
    while o + 8 <= d.len() {
        let len = be32(d, o);
        let kind = &d[o + 4..o + 8];
        let body = d.get(o + 8..o + 8 + len)?;
        match kind {
            b"IHDR" => {
                (w, h, depth) = (be32(body, 0), be32(body, 4), *body.get(8)? as usize);
                // Con paleta (3) o grises (0), sin entrelazar.
                if !matches!(body.get(9)?, 0 | 3) || *body.get(12)? != 0 || !matches!(depth, 1 | 2 | 4 | 8) {
                    return None;
                }
            }
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        o += 12 + len;
    }
    if w == 0 || h == 0 {
        return None;
    }
    let row = (w * depth).div_ceil(8);
    let raw = inflate(&idat, (row + 1) * h)?;
    if raw.len() < (row + 1) * h {
        return None;
    }
    // Filtros de fila: cada byte se predice con el de la izquierda, el de
    // arriba o los dos.
    let mut lines = vec![0u8; row * h];
    for y in 0..h {
        let filter = raw[y * (row + 1)];
        let src = &raw[y * (row + 1) + 1..(y + 1) * (row + 1)];
        let (prev, cur) = lines.split_at_mut(y * row);
        let up = if y > 0 { &prev[(y - 1) * row..] } else { &[][..] };
        let cur = &mut cur[..row];
        for x in 0..row {
            let a = if x > 0 { cur[x - 1] } else { 0 };
            let b = up.get(x).copied().unwrap_or(0);
            let c = if x > 0 { up.get(x - 1).copied().unwrap_or(0) } else { 0 };
            let pred = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                4 => {
                    let p = a as i32 + b as i32 - c as i32;
                    let (pa, pb, pc) = ((p - a as i32).abs(), (p - b as i32).abs(), (p - c as i32).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => return None,
            };
            cur[x] = src[x].wrapping_add(pred);
        }
    }
    if depth == 8 {
        return Some((w, h, lines));
    }
    // Menos de 8 bits por píxel: varios píxeles por byte, el primero arriba.
    let mut pix = vec![0u8; w * h];
    let mask = (1u8 << depth) - 1;
    for y in 0..h {
        for x in 0..w {
            let bit = x * depth;
            let byte = lines[y * row + bit / 8];
            pix[y * w + x] = (byte >> (8 - depth - bit % 8)) & mask;
        }
    }
    Some((w, h, pix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inflate_matches_zlib() {
        // Texto con repeticiones comprimido por zlib (Huffman dinámico).
        let packed: [u8; 918] = [120, 156, 237, 211, 239, 43, 36, 113, 0, 199, 241, 49, 218, 206, 55, 181, 71, 166, 142, 168, 27, 153, 237, 72, 237, 145, 7, 124, 163, 110, 164, 237, 124, 83, 123, 140, 7, 71, 212, 205, 222, 182, 29, 169, 189, 145, 7, 71, 212, 141, 209, 118, 164, 246, 24, 83, 71, 212, 141, 180, 29, 169, 61, 242, 224, 124, 163, 238, 43, 109, 71, 106, 143, 60, 56, 162, 110, 100, 182, 35, 181, 70, 30, 28, 81, 247, 227, 63, 184, 231, 55, 159, 207, 251, 95, 120, 81, 20, 149, 66, 167, 58, 238, 128, 116, 103, 38, 147, 157, 199, 114, 69, 238, 50, 248, 200, 83, 39, 52, 139, 109, 82, 183, 60, 164, 78, 70, 62, 226, 245, 248, 190, 105, 221, 166, 49, 249, 238, 74, 244, 52, 208, 165, 140, 234, 11, 100, 219, 184, 160, 179, 92, 229, 168, 37, 216, 175, 205, 174, 110, 155, 215, 206, 2, 248, 36, 208, 171, 206, 145, 61, 43, 141, 133, 66, 71, 232, 61, 222, 77, 2, 142, 111, 237, 214, 22, 227, 103, 224, 129, 199, 175, 76, 19, 131, 98, 121, 159, 50, 19, 251, 1, 138, 189, 157, 218, 242, 33, 197, 161, 224, 24, 254, 238, 40, 22, 122, 244, 13, 43, 155, 111, 87, 87, 18, 119, 161, 63, 140, 205, 140, 170, 54, 109, 45, 153, 139, 94, 69, 118, 233, 82, 113, 132, 88, 108, 131, 178, 148, 184, 135, 122, 163, 71, 76, 109, 223, 130, 153, 227, 29, 192, 231, 174, 22, 117, 147, 170, 144, 102, 141, 28, 97, 56, 246, 11, 118, 69, 79, 57, 113, 106, 143, 105, 8, 111, 129, 199, 161, 117, 186, 90, 38, 20, 47, 175, 209, 53, 161, 47, 160, 46, 188, 195, 52, 77, 25, 92, 123, 244, 18, 190, 142, 165, 11, 19, 70, 161, 180, 76, 33, 245, 192, 245, 18, 167, 122, 39, 204, 146, 190, 13, 198, 23, 189, 65, 239, 18, 101, 202, 14, 43, 145, 12, 113, 145, 110, 140, 252, 68, 147, 201, 106, 237, 164, 106, 196, 132, 225, 99, 248, 54, 81, 169, 158, 242, 227, 86, 173, 126, 45, 204, 59, 158, 225, 172, 224, 38, 39, 31, 86, 104, 23, 222, 121, 240, 34, 86, 160, 28, 241, 58, 37, 146, 124, 229, 216, 243, 1, 116, 196, 75, 181, 171, 86, 194, 13, 37, 27, 241, 253, 208, 153, 128, 217, 65, 171, 137, 20, 169, 55, 129, 175, 112, 198, 217, 99, 214, 175, 22, 106, 41, 193, 3, 244, 201, 53, 70, 119, 26, 245, 164, 68, 207, 84, 46, 3, 223, 208, 138, 123, 154, 25, 188, 149, 204, 230, 120, 13, 126, 24, 201, 85, 129, 124, 37, 157, 136, 251, 194, 150, 231, 51, 92, 114, 207, 113, 122, 222, 56, 51, 234, 28, 6, 111, 28, 3, 116, 63, 37, 255, 121, 63, 61, 224, 120, 3, 134, 157, 163, 204, 120, 158, 206, 205, 185, 151, 224, 103, 207, 150, 176, 47, 158, 72, 87, 50, 80, 115, 35, 15, 113, 77, 188, 217, 148, 110, 7, 153, 105, 247, 10, 250, 22, 184, 84, 50, 245, 18, 82, 111, 116, 210, 99, 174, 79, 232, 32, 152, 162, 21, 174, 214, 155, 61, 206, 25, 248, 53, 112, 163, 22, 145, 38, 107, 144, 197, 194, 89, 232, 62, 110, 76, 14, 113, 164, 245, 74, 43, 141, 119, 128, 15, 158, 99, 37, 159, 136, 148, 206, 31, 41, 5, 177, 23, 96, 222, 123, 161, 85, 28, 202, 220, 102, 48, 11, 63, 115, 204, 11, 215, 122, 173, 53, 206, 159, 170, 149, 137, 183, 240, 56, 12, 205, 145, 170, 19, 173, 58, 57, 137, 126, 70, 26, 233, 69, 49, 131, 72, 236, 142, 82, 150, 120, 135, 110, 162, 62, 102, 163, 175, 196, 156, 240, 166, 226, 151, 174, 3, 21, 81, 203, 82, 161, 49, 33, 164, 199, 94, 195, 203, 104, 59, 103, 76, 53, 49, 59, 225, 58, 240, 37, 84, 67, 175, 201, 60, 69, 228, 106, 122, 61, 244, 24, 108, 133, 27, 152, 189, 41, 145, 59, 141, 118, 193, 95, 177, 97, 33, 199, 152, 149, 42, 168, 77, 181, 197, 117, 142, 7, 188, 57, 230, 66, 95, 45, 115, 20, 237, 69, 247, 18, 75, 74, 3, 107, 145, 17, 177, 148, 222, 141, 188, 66, 185, 201, 53, 173, 173, 42, 195, 196, 97, 63, 188, 155, 88, 81, 219, 249, 108, 107, 67, 239, 17, 138, 29, 223, 241, 88, 16, 113, 212, 225, 178, 214, 233, 45, 6, 63, 98, 51, 138, 143, 103, 41, 131, 76, 43, 126, 207, 3, 112, 22, 95, 212, 186, 91, 121, 14, 36, 119, 241, 251, 80, 135, 0, 217, 52, 107, 143, 204, 169, 189, 129, 39, 176, 192, 121, 109, 110, 175, 206, 106, 253, 193, 22, 84, 238, 202, 162, 47, 140, 109, 178, 160, 143, 42, 93, 129, 167, 168, 210, 157, 207, 164, 221, 90, 230, 126, 124, 29, 127, 140, 76, 170, 67, 114, 183, 212, 38, 54, 11, 117, 158, 71, 176, 204, 93, 196, 177, 121, 217, 76, 166, 51, 29, 220, 113, 164, 210, 41, 212, 223, 217, 254, 108, 127, 182, 63, 219, 159, 237, 207, 246, 103, 251, 179, 253, 253, 147, 63, 223, 115, 191, 157, 157, 157, 157, 157, 221, 127, 219, 111, 41, 213, 25, 188];
        let expected: Vec<u8> = (0..3000u32).map(|i| (i * i / 7 % 256) as u8).chain(b"abcabcabc".iter().copied().cycle().take(1800)).collect();
        assert_eq!(inflate(&packed, expected.len()).unwrap(), expected);
        // Bloque sin comprimir y Huffman fijo.
        let stored = [0x78, 0x01, 0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(inflate(&stored, 3).unwrap(), b"abc");
        let fixed: [u8; 14] = [120, 218, 75, 76, 132, 129, 36, 56, 0, 0, 79, 221, 7, 159];
        assert_eq!(inflate(&fixed, 64).unwrap(), b"aaaaaaaaaabbbbbbbbbb");
        // Un flujo cortado no cuelga ni inventa datos.
        assert!(inflate(&packed[..packed.len() / 2], expected.len()).is_none());
    }

    #[test]
    fn png_with_every_row_filter() {
        let png: [u8; 95] = [137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 6, 0, 0, 0, 5, 8, 3, 0, 0, 0, 81, 134, 109, 212, 0, 0, 0, 38, 73, 68, 65, 84, 120, 218, 99, 96, 96, 231, 19, 149, 81, 102, 228, 229, 0, 1, 38, 94, 62, 126, 1, 65, 33, 102, 41, 30, 94, 94, 62, 62, 22, 94, 110, 16, 0, 0, 29, 93, 1, 166, 159, 218, 114, 75, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
        let (w, h, pix) = decode_indexed(&png).unwrap();
        assert_eq!((w, h), (6, 5));
        let expected: Vec<u8> = (0..5).flat_map(|y| (0..6).map(move |x| ((x * 7 + y * 13 + x * y) % 251) as u8)).collect();
        assert_eq!(pix, expected);
        // 4 bits por píxel: dos píxeles por byte.
        let png4: [u8; 73] = [137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 5, 0, 0, 0, 2, 4, 3, 0, 0, 0, 98, 68, 11, 110, 0, 0, 0, 16, 73, 68, 65, 84, 120, 218, 99, 16, 50, 9, 96, 248, 119, 103, 3, 0, 8, 176, 3, 33, 51, 198, 157, 65, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130];
        let (w, h, pix) = decode_indexed(&png4).unwrap();
        assert_eq!((w, h), (5, 2));
        assert_eq!(pix, [1, 2, 3, 4, 5, 15, 14, 13, 12, 11]);
        assert!(decode_indexed(b"esto no es un PNG").is_none());
    }
}
