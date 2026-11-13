//! Codificación a PS-ADPCM, el formato de muestras del chip de sonido (SPU2)
//! de la PlayStation 2: bloques de 16 bytes con 28 muestras de 4 bits.

const FILTERS: [[i32; 2]; 5] = [[0, 0], [60, 0], [115, -52], [98, -55], [122, -60]];

/// Codifica PCM mono de 16 bits. El resultado empieza con un bloque en
/// silencio y su último bloque lleva la marca de fin (la voz se calla sola).
pub fn encode(pcm: &[i16]) -> Vec<u8> {
    let mut out = vec![0u8; 16];
    // Historial real (para elegir filtro) y historial del decodificador.
    let (mut h1, mut h2) = (0i32, 0i32);
    let (mut q1, mut q2) = (0i32, 0i32);
    let blocks = pcm.len().div_ceil(28).max(1);
    for b in 0..blocks {
        let mut s = [0i32; 28];
        for (i, v) in s.iter_mut().enumerate() {
            *v = pcm.get(b * 28 + i).copied().unwrap_or(0) as i32;
        }
        // El filtro que deja el residuo más pequeño.
        let (mut best_max, mut filter) = (i32::MAX, 0usize);
        for (f, k) in FILTERS.iter().enumerate() {
            let (mut a, mut c) = (h1, h2);
            let mut max = 0;
            for &v in &s {
                let pred = (a * k[0] + c * k[1]) >> 6;
                max = max.max((v - pred).abs());
                c = a;
                a = v;
            }
            if max < best_max {
                best_max = max;
                filter = f;
            }
        }
        h1 = s[27];
        h2 = s[26];
        // El desplazamiento más fino con el que el residuo aún cabe en 4 bits
        // (con margen: la realimentación del error lo agranda un poco).
        let need = best_max + best_max / 4 + 1;
        let mut shift = 12u32;
        while shift > 0 && need > 7 << (12 - shift) {
            shift -= 1;
        }
        let k = FILTERS[filter];
        let step = 12 - shift;
        let mut block = [0u8; 16];
        block[0] = (filter as u8) << 4 | shift as u8;
        block[1] = if b + 1 == blocks { 1 } else { 0 };
        for (i, &v) in s.iter().enumerate() {
            let pred = (q1 * k[0] + q2 * k[1] + 32) >> 6;
            let e = v - pred;
            let nib = ((e + (1 << step >> 1)) >> step).clamp(-8, 7);
            let dec = (pred + (nib << step)).clamp(-32768, 32767);
            q2 = q1;
            q1 = dec;
            block[2 + i / 2] |= ((nib & 0xf) as u8) << (4 * (i & 1));
        }
        out.extend_from_slice(&block);
    }
    out
}

/// Decodifica PS-ADPCM igual que lo hace el SPU2 (para pruebas).
pub fn decode(data: &[u8]) -> Vec<i16> {
    let mut out = Vec::new();
    let (mut q1, mut q2) = (0i32, 0i32);
    for block in data.chunks_exact(16) {
        let (shift, filter) = ((block[0] & 0xf) as u32, (block[0] >> 4) as usize);
        let k = FILTERS[filter.min(4)];
        for i in 0..28 {
            let nib = (block[2 + i / 2] >> (4 * (i & 1))) & 0xf;
            let nib = ((nib as i32) << 28) >> 28;
            let pred = (q1 * k[0] + q2 * k[1] + 32) >> 6;
            let dec = (pred + ((nib << 12) >> shift.min(12))).clamp(-32768, 32767);
            q2 = q1;
            q1 = dec;
            out.push(dec as i16);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_the_signal() {
        // Una mezcla de senos, como una voz: el error debe ser pequeño.
        let pcm: Vec<i16> = (0..5000)
            .map(|i| {
                let t = i as f32;
                ((t * 0.05).sin() * 9000.0 + (t * 0.31).sin() * 4000.0 + (t * 0.9).sin() * 1500.0) as i16
            })
            .collect();
        let enc = encode(&pcm);
        assert_eq!(enc.len() % 16, 0);
        assert_eq!(enc[enc.len() - 15], 1, "el último bloque marca el fin");
        let dec = decode(&enc[16..]);
        let (mut err, mut sig) = (0f64, 0f64);
        for (a, b) in pcm.iter().zip(&dec) {
            err += ((*a as f64) - (*b as f64)).powi(2);
            sig += (*a as f64).powi(2);
        }
        let snr = 10.0 * (sig / err).log10();
        assert!(snr > 20.0, "relación señal/ruido {snr:.1} dB");
    }

    #[test]
    fn silence_and_extremes() {
        assert_eq!(encode(&[]).len(), 32);
        let loud: Vec<i16> = (0..200).map(|i| if i % 2 == 0 { 32767 } else { -32768 }).collect();
        let dec = decode(&encode(&loud)[16..]);
        assert!(dec.len() >= 200);
    }
}
