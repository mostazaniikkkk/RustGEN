//! Fuentes FNT v1: una tira PCX con los glifos y un bloque de texto que
//! dice dónde está cada carácter.

use crate::gfx::{Frame, PalRef, SpriteCmd, Trans};
use crate::ini;
use crate::sff::{decode_pcx, Pal, Sff, Sprite};
use std::collections::HashMap;

pub struct Font {
    /// Cantidad de bancos de color (paletas de la tira).
    pub banks: usize,
    pub glyphs: HashMap<char, (u16, u16)>,
    pub size: (i32, i32),
    pub spacing: (i32, i32),
    pub offset: (i32, i32),
}

/// Fuente a usar en un texto: número de fuente, banco de color, alineación.
pub type FontSpec = (usize, i32, i32);

/// Lee `clave = fuente, banco, alineación`.
pub fn font_spec(s: &ini::Section, key: &str) -> Option<FontSpec> {
    s.get(key).filter(|v| !v.trim().is_empty()).map(|v| {
        let l = ini::nums(v, 0.0);
        let at = |i: usize, d: f32| l.get(i).copied().unwrap_or(d);
        (at(0, 1.0) as usize, at(1, 0.0) as i32, at(2, 0.0) as i32)
    })
}

/// Las fuentes `font1`…`font9` de un archivo de definición.
#[derive(Default)]
pub struct FontSet {
    /// Por número (el 0 no se usa): la fuente y el id de SFF de su tira.
    fonts: Vec<Option<(Font, u16)>>,
}

impl FontSet {
    /// `find` convierte el nombre de archivo en una ruta. Si la fuente es un
    /// `.def` (FNT v2, no soportado) se prueba el `.fnt` del mismo nombre.
    pub fn load(files: &ini::Section, find: &dyn Fn(&str) -> String, sffs: &mut Vec<Sff>) -> FontSet {
        let mut fonts = vec![None];
        for n in 1..=9 {
            let font = files.str(&format!("font{n}")).filter(|f| !f.is_empty()).and_then(|f| {
                let alt = match f.rfind('.') {
                    Some(dot) => format!("{}.fnt", &f[..dot]),
                    None => f.clone(),
                };
                Font::load(&find(&f)).or_else(|_| Font::load(&find(&alt))).ok()
            });
            fonts.push(font.map(|(font, sheet)| {
                sffs.push(sheet);
                (font, sffs.len() as u16 - 1)
            }));
        }
        FontSet { fonts }
    }

    pub fn get(&self, no: usize) -> Option<&Font> {
        self.fonts.get(no)?.as_ref().map(|f| &f.0)
    }

    pub fn draw(&self, frame: &mut Frame, spec: Option<FontSpec>, x: f32, y: f32, text: &str) {
        let Some((no, bank, align)) = spec else { return };
        if let Some(Some((font, sff))) = self.fonts.get(no) {
            font.draw(frame, *sff, bank, align, x, y, text);
        }
    }

    pub fn width(&self, spec: Option<FontSpec>, text: &str) -> i32 {
        spec.and_then(|s| self.get(s.0)).map_or(0, |f| f.text_width(text))
    }

    /// Parte `text` en líneas que quepan en `width` píxeles.
    pub fn wrap(&self, spec: Option<FontSpec>, text: &str, width: i32) -> Vec<String> {
        let mut lines = vec![String::new()];
        for word in text.split_whitespace() {
            let cur = lines.last_mut().unwrap();
            let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
            if self.width(spec, &candidate) <= width || cur.is_empty() {
                *cur = candidate;
            } else {
                lines.push(word.to_string());
            }
        }
        lines
    }
}

fn u32le(d: &[u8], o: usize) -> usize {
    d.get(o..o + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])) as usize
}

impl Font {
    /// Devuelve la fuente y un SFF con un único sprite (la tira de glifos) y
    /// una paleta por banco de color.
    pub fn load(path: &str) -> Result<(Font, Sff), String> {
        let d = crate::vfs::read(path).map_err(|e| format!("{path}: {e}"))?;
        if d.len() < 32 || &d[..12] != b"ElecbyteFnt\0" {
            return Err(format!("{path}: no es una fuente FNT v1"));
        }
        let (pcx_off, pcx_len) = (u32le(&d, 16), u32le(&d, 20));
        let (txt_off, txt_len) = (u32le(&d, 24), u32le(&d, 28));
        let pcx = d.get(pcx_off..(pcx_off + pcx_len).min(d.len())).ok_or("FNT truncada")?;
        let (w, h, pix) = decode_pcx(pcx).ok_or("FNT: PCX inválido")?;
        let mut base: Pal = [0; 256];
        if pcx.len() >= 768 {
            for (i, c) in pcx[pcx.len() - 768..].chunks_exact(3).enumerate() {
                base[i] = c[0] as u32 | (c[1] as u32) << 8 | (c[2] as u32) << 16 | if i == 0 { 0 } else { 0xff00_0000 };
            }
        }
        let text = String::from_utf8_lossy(d.get(txt_off..(txt_off + txt_len).min(d.len())).unwrap_or(&[])).to_string();

        let mut font = Font { banks: 1, glyphs: HashMap::new(), size: (0, 0), spacing: (0, 0), offset: (0, 0) };
        let mut sheet = Sff::default();
        let mut colors = 16usize;
        let secs = ini::parse(&text);
        if let Some(def) = secs.iter().find(|s| s.name == "def") {
            let p = |k: &str| {
                let (a, b) = def.pair(k, 0.0, 0.0);
                (a as i32, b as i32)
            };
            font.size = p("size");
            font.spacing = p("spacing");
            font.offset = p("offset");
            colors = (def.i32("colors", 16).clamp(1, 255)) as usize;
        }
        // El mapa no son pares clave/valor: se recorre el texto a mano.
        let mut in_map = false;
        let mut ofs = 0i32;
        for raw in text.lines() {
            let line = ini::clean(raw);
            if line.starts_with('[') {
                in_map = line.to_ascii_lowercase().starts_with("[map");
                continue;
            }
            if !in_map || line.is_empty() {
                continue;
            }
            let mut it = line.split_whitespace();
            let tok = it.next().unwrap();
            let ch = if tok.len() > 2 && tok.to_ascii_lowercase().starts_with("0x") {
                u32::from_str_radix(&tok[2..], 16).ok().and_then(char::from_u32)
            } else {
                tok.chars().next()
            };
            let Some(ch) = ch else { continue };
            let mut gw = font.size.0;
            if let Some(x) = it.next().and_then(ini::num) {
                ofs = x as i32;
                if let Some(wd) = it.next().and_then(ini::num) {
                    gw = wd as i32;
                }
            }
            font.glyphs.entry(ch).or_insert((ofs.max(0) as u16, gw.max(0) as u16));
            ofs += gw;
        }

        // Bancos de color: cada uno cambia el bloque alto de la paleta.
        let c = (colors.div_ceil(16) * 16).min(255);
        let banks = (255 / c).max(1);
        for i in 0..banks {
            let mut p = base;
            for k in 0..c {
                let src = 256 - c * (i + 1) + k;
                p[256 - c + k] = base[src.min(255)];
            }
            p[0] &= 0x00ff_ffff;
            sheet.pals.push(p);
        }
        font.banks = banks;
        sheet.sprites.push(Sprite::new(0, 0, w, h, 0, 0, 0, pix));
        Ok((font, sheet))
    }

    pub fn char_width(&self, c: char) -> i32 {
        if c == ' ' {
            return self.size.0;
        }
        self.glyphs.get(&c).map_or(self.size.0, |g| g.1 as i32)
    }

    pub fn text_width(&self, text: &str) -> i32 {
        let n = text.chars().count() as i32;
        text.chars().map(|c| self.char_width(c)).sum::<i32>() + self.spacing.0 * (n - 1).max(0)
    }

    /// Dibuja `text`. `align`: 1 = `x` es el borde izquierdo, 0 = el centro,
    /// -1 = el borde derecho. `y` es la base de la línea.
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&self, frame: &mut Frame, sff_id: u16, bank: i32, align: i32, x: f32, y: f32, text: &str) {
        let width = self.text_width(text) as f32;
        let mut cx = match align {
            0 => x - (width / 2.0).floor(),
            a if a < 0 => x - width,
            _ => x,
        } + self.offset.0 as f32;
        let top = y - self.size.1 as f32 + self.offset.1 as f32;
        let bank = (bank.max(0) as usize).min(self.banks.max(1) - 1) as u16;
        for ch in text.chars() {
            if let Some(&(gx, gw)) = self.glyphs.get(&ch) {
                if ch != ' ' {
                    frame.sprite(SpriteCmd {
                        sff: sff_id,
                        spr: 0,
                        x: cx - gx as f32,
                        y: top,
                        sx: 1.0,
                        sy: 1.0,
                        pal: PalRef::Sff(sff_id, bank),
                        trans: Trans::None,
                        clip: None,
                        src: Some([gx, 0, gx + gw, self.size.1.max(0) as u16]),
                    });
                }
            }
            cx += (self.char_width(ch) + self.spacing.0) as f32;
        }
    }
}
