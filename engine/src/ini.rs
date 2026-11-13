//! Lectura de los archivos de texto de MUGEN (def, cns, cmd, air): secciones
//! `[Nombre]`, pares `clave = valor` y comentarios con `;`.

use std::io;

/// Lee un archivo de texto tolerando bytes que no son UTF-8 y el BOM inicial.
pub fn read_text(path: &str) -> io::Result<String> {
    let bytes = crate::vfs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.trim_start_matches('\u{feff}').to_string())
}

/// Quita el comentario (`;` fuera de comillas) y los espacios de los extremos.
pub fn clean(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => return line[..i].trim(),
            _ => {}
        }
    }
    line.trim()
}

#[derive(Debug, Clone, Default)]
pub struct Section {
    /// Nombre en minúsculas, sin corchetes.
    pub name: String,
    /// Nombre tal como estaba escrito.
    pub raw_name: String,
    /// Pares clave (minúsculas) / valor.
    pub items: Vec<(String, String)>,
    /// Líneas sin `=` (elementos de animación, por ejemplo).
    pub lines: Vec<String>,
}

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.items.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn str(&self, key: &str) -> Option<String> {
        self.get(key).map(unquote)
    }

    pub fn f32(&self, key: &str, default: f32) -> f32 {
        self.get(key).and_then(|v| num(first(v))).unwrap_or(default)
    }

    pub fn i32(&self, key: &str, default: i32) -> i32 {
        self.f32(key, default as f32) as i32
    }

    /// Lista de números separados por comas; los huecos valen `default`.
    pub fn list(&self, key: &str, default: f32) -> Vec<f32> {
        self.get(key).map(|v| nums(v, default)).unwrap_or_default()
    }

    pub fn pair(&self, key: &str, dx: f32, dy: f32) -> (f32, f32) {
        let l = self.list(key, 0.0);
        let raw: Vec<&str> = self.get(key).map(|v| v.split(',').collect()).unwrap_or_default();
        let at = |i: usize, d: f32| if raw.get(i).map_or(true, |s| s.trim().is_empty()) { d } else { l[i] };
        (at(0, dx), at(1, dy))
    }
}

fn first(v: &str) -> &str {
    v.split(',').next().unwrap_or("").trim()
}

/// Número decimal con signo; acepta `.5`, `5.` y basura al final.
pub fn num(s: &str) -> Option<f32> {
    let s = s.trim();
    let end = s
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || (i == 0 && (c == '-' || c == '+'))))
        .map_or(s.len(), |(i, _)| i);
    let t = &s[..end];
    if t.is_empty() || t == "-" || t == "+" || t == "." {
        return None;
    }
    parse_f32(t)
}

/// Conversión propia de decimal a `f32`: la de `core` usa aritmética de 64
/// bits pesada y aquí sobra con unos pocos dígitos.
pub fn parse_f32(t: &str) -> Option<f32> {
    let (neg, t) = match t.as_bytes().first()? {
        b'-' => (true, &t[1..]),
        b'+' => (false, &t[1..]),
        _ => (false, t),
    };
    let mut int: u64 = 0;
    let mut frac: u64 = 0;
    let mut scale: u64 = 1;
    let mut seen_dot = false;
    let mut any = false;
    for b in t.bytes() {
        match b {
            b'0'..=b'9' => {
                any = true;
                let d = (b - b'0') as u64;
                if !seen_dot {
                    int = (int * 10 + d).min(4_000_000_000);
                } else if scale < 1_000_000_000 {
                    frac = frac * 10 + d;
                    scale *= 10;
                }
            }
            b'.' if !seen_dot => seen_dot = true,
            _ => return None,
        }
    }
    if !any {
        return None;
    }
    let v = int as f32 + frac as f32 / scale as f32;
    Some(if neg { -v } else { v })
}

pub fn nums(v: &str, default: f32) -> Vec<f32> {
    v.split(',').map(|s| num(s).unwrap_or(default)).collect()
}

pub fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && v.starts_with('"') {
        if let Some(end) = v[1..].find('"') {
            return v[1..1 + end].to_string();
        }
    }
    v.to_string()
}

/// Divide el texto en secciones. Lo anterior a la primera va en una sección
/// de nombre vacío.
pub fn parse(text: &str) -> Vec<Section> {
    let mut out = vec![Section::default()];
    for raw in text.lines() {
        let line = clean(raw);
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if let Some(end) = line.find(']') {
                let name = line[1..end].trim();
                out.push(Section { name: name.to_ascii_lowercase(), raw_name: name.to_string(), ..Default::default() });
                continue;
            }
        }
        let cur = out.last_mut().unwrap();
        match line.find('=') {
            // `Clsn2[0] = ...` y similares también son pares.
            Some(eq) if !line[..eq].trim().is_empty() => {
                cur.items.push((line[..eq].trim().to_ascii_lowercase(), line[eq + 1..].trim().to_string()));
            }
            _ => cur.lines.push(line.to_string()),
        }
    }
    out
}

pub fn load(path: &str) -> io::Result<Vec<Section>> {
    Ok(parse(&read_text(path)?))
}

/// Busca `file` junto a `def_path`, en la raíz del juego, en `data/` y en
/// `font/`. Si no está en ningún sitio devuelve la primera opción.
pub fn find_file(root: &str, def_path: &str, file: &str) -> String {
    let file = file.replace('\\', "/");
    let options = [sibling(def_path, &file), format!("{root}/{file}"), format!("{root}/data/{file}"), format!("{root}/font/{file}")];
    options.iter().find(|p| crate::vfs::exists(p.as_str())).unwrap_or(&options[0]).clone()
}

/// Une la carpeta de `base` con `file` (las rutas de un .def son relativas a él).
pub fn sibling(base: &str, file: &str) -> String {
    let file = file.replace('\\', "/");
    match base.rfind('/') {
        Some(i) => format!("{}/{}", &base[..i], file),
        None => file,
    }
}
