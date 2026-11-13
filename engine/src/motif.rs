//! Screenpack ("motif"): `system.def` con el menú de título, la selección de
//! personaje, la pantalla de versus y la de victoria, y `select.def` con la
//! lista de personajes y escenarios.

use crate::air::{self, AnimTable};
use crate::bg::BgSet;
use crate::fnt::{font_spec, FontSet, FontSpec};
use crate::ini::{self, Section};
use crate::sff::{Sff, Sprite};
use crate::snd::Snd;
use crate::stage::stage_name;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Arcade,
    Versus,
    Training,
    Watch,
    /// No es un modo de juego: la entrada OPTIONS del menú.
    Options,
}

pub type SprNo = Option<(i32, i32)>;
pub type SndNo = Option<(i32, i32)>;

/// Sprite colocado en una pantalla: número, posición, orientación y escala.
#[derive(Default, Clone)]
pub struct Placed {
    pub spr: SprNo,
    pub offset: (f32, f32),
    pub facing: f32,
    pub scale: (f32, f32),
    pub window: Option<[i32; 4]>,
}

fn pair_opt(s: &Section, key: &str) -> Option<(i32, i32)> {
    s.get(key).filter(|v| !v.trim().is_empty()).map(|_| s.pair(key, 0.0, 0.0)).map(|(a, b)| (a as i32, b as i32))
}

fn window(s: &Section, key: &str) -> Option<[i32; 4]> {
    s.get(key).map(|v| ini::nums(v, 0.0)).filter(|w| w.len() >= 4).map(|w| [w[0] as i32, w[1] as i32, w[2] as i32 + 1, w[3] as i32 + 1])
}

impl Placed {
    fn read(s: &Section, prefix: &str) -> Placed {
        let key = |k: &str| format!("{prefix}.{k}");
        // Algunos motifs escriben `pos` donde otros `offset`.
        let offset = if s.get(&key("offset")).is_some() { s.pair(&key("offset"), 0.0, 0.0) } else { s.pair(&key("pos"), 0.0, 0.0) };
        Placed {
            spr: pair_opt(s, &key("spr")),
            offset,
            facing: if s.i32(&key("facing"), 1) < 0 { -1.0 } else { 1.0 },
            scale: s.pair(&key("scale"), 1.0, 1.0),
            window: window(s, &key("window")),
        }
    }
}

pub struct Title {
    pub fadein: i32,
    pub fadeout: i32,
    pub menu_pos: (f32, f32),
    pub item_font: Option<FontSpec>,
    pub active_font: Option<FontSpec>,
    pub spacing: (f32, f32),
    pub items: Vec<(Mode, String)>,
    pub visible: usize,
    pub box_visible: bool,
    pub box_coords: [f32; 4],
    pub snd_move: SndNo,
    pub snd_done: SndNo,
    pub snd_cancel: SndNo,
}

pub struct Select {
    pub fadein: i32,
    pub fadeout: i32,
    pub rows: i32,
    pub columns: i32,
    pub wrapping: bool,
    pub pos: (f32, f32),
    pub show_empty: bool,
    pub move_over_empty: bool,
    pub cell_size: (f32, f32),
    pub cell_spacing: f32,
    pub cell_bg: SprNo,
    pub cell_random: SprNo,
    pub random_switch: i32,
    pub cursor_start: [(i32, i32); 2],
    pub cursor_active: [SprNo; 2],
    pub cursor_done: [SprNo; 2],
    pub p2_blink: bool,
    pub snd_move: [SndNo; 2],
    pub snd_done: [SndNo; 2],
    pub snd_stage_move: SndNo,
    pub snd_stage_done: SndNo,
    pub snd_cancel: SndNo,
    pub portrait: Placed,
    pub title_offset: (f32, f32),
    pub title_font: Option<FontSpec>,
    pub face: [Placed; 2],
    pub name_offset: [(f32, f32); 2],
    pub name_font: [Option<FontSpec>; 2],
    pub stage_pos: (f32, f32),
    pub stage_active_font: Option<FontSpec>,
    pub stage_active2_font: Option<FontSpec>,
    pub stage_done_font: Option<FontSpec>,
}

pub struct Versus {
    pub time: i32,
    pub fadein: i32,
    pub fadeout: i32,
    pub match_text: String,
    pub match_offset: (f32, f32),
    pub match_font: Option<FontSpec>,
    pub p: [Placed; 2],
    pub name_offset: [(f32, f32); 2],
    pub name_font: [Option<FontSpec>; 2],
}

pub struct Victory {
    pub enabled: bool,
    pub time: i32,
    pub fadein: i32,
    pub fadeout: i32,
    pub p1: Placed,
    pub name_offset: (f32, f32),
    pub name_font: Option<FontSpec>,
    pub quote_text: String,
    pub quote_offset: (f32, f32),
    pub quote_font: Option<FontSpec>,
    pub quote_window: Option<[i32; 4]>,
}

pub struct Demo {
    pub enabled: bool,
    pub waittime: i32,
    pub endtime: i32,
    pub bars: bool,
}

pub struct WinScreen {
    pub text: String,
    pub font: Option<FontSpec>,
    pub offset: (f32, f32),
    pub time: i32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
    Char(usize),
    Random,
    Empty,
}

pub struct RosterChar {
    pub def: String,
    pub name: String,
    /// Escenario propio (modo arcade).
    pub stage: Option<usize>,
    pub order: i32,
    /// Sprites 9000,0 / 9000,1 / 9000,2 dentro del SFF de retratos.
    pub portraits: [Option<u32>; 3],
    pub intro: Option<String>,
    pub ending: Option<String>,
}

pub struct StageEntry {
    pub path: String,
    pub name: String,
}

pub struct Motif {
    pub sff: u16,
    pub snd: u16,
    pub fonts: FontSet,
    pub anims: AnimTable,
    pub fight_def: String,
    pub logo: Option<String>,
    pub intro: Option<String>,
    pub gameover: Option<String>,
    pub credits: Option<String>,
    pub title: Title,
    pub title_bg: BgSet,
    /// Pantalla de opciones: fondo y sonidos de mover, aceptar y cancelar.
    pub option_bg: BgSet,
    pub option_snd: [SndNo; 3],
    pub select: Select,
    pub select_bg: BgSet,
    pub vs: Versus,
    pub vs_bg: BgSet,
    pub victory: Victory,
    pub victory_bg: BgSet,
    pub demo: Demo,
    pub win: WinScreen,
    pub slots: Vec<Slot>,
    pub chars: Vec<RosterChar>,
    pub stages: Vec<StageEntry>,
    /// SFF con los retratos de todos los personajes de la lista.
    pub portraits: u16,
    /// Combates por cada nivel de `order` en el modo arcade.
    pub max_matches: Vec<i32>,
}

/// Ruta del `.def` de un personaje a partir de su entrada en `select.def`.
fn char_def(root: &str, name: &str) -> String {
    let name = name.replace('\\', "/");
    if name.to_ascii_lowercase().ends_with(".def") {
        format!("{root}/chars/{name}")
    } else {
        let base = name.rsplit('/').next().unwrap_or(&name).to_string();
        format!("{root}/chars/{name}/{base}.def")
    }
}

impl Motif {
    /// Carga el screenpack y añade sus sprites, fuentes y sonidos a las listas.
    pub fn load(root: &str, path: &str, sffs: &mut Vec<Sff>, snds: &mut Vec<Snd>) -> Result<Motif, String> {
        let text = ini::read_text(path).map_err(|e| format!("{path}: {e}"))?;
        let secs = ini::parse(&text);
        let empty = Section::default();
        let sec = |n: &str| secs.iter().find(|s| s.name == n).unwrap_or(&empty);
        let files = sec("files");
        let find = |f: &str| ini::find_file(root, path, f);
        let file = |key: &str| files.str(key).filter(|f| !f.is_empty()).map(|f| find(&f));

        sffs.push(Sff::load(&file("spr").ok_or(format!("{path}: falta spr"))?)?);
        let sff = sffs.len() as u16 - 1;
        snds.push(file("snd").and_then(|f| Snd::load(&f).ok()).unwrap_or_default());
        let snd = snds.len() as u16 - 1;
        let fonts = FontSet::load(files, &find, sffs);
        let snd_no = |s: &Section, key: &str| pair_opt(s, key);
        let story = |s: &Section, key: &str, need_enabled: bool| {
            if need_enabled && s.i32("enabled", 0) == 0 {
                return None;
            }
            s.str(key).filter(|f| !f.is_empty()).map(|f| find(&f))
        };

        let ti = sec("title info");
        let mut items = Vec::new();
        for (mode, key, default) in [
            (Mode::Arcade, "arcade", "ARCADE"),
            (Mode::Versus, "versus", "VS MODE"),
            (Mode::Training, "training", "TRAINING"),
            (Mode::Watch, "watch", "WATCH"),
            (Mode::Options, "options", "OPTIONS"),
        ] {
            let name = ti.str(&format!("menu.itemname.{key}")).unwrap_or_else(|| default.to_string());
            // Un nombre vacío quita la opción del menú.
            if !name.is_empty() {
                items.push((mode, name));
            }
        }
        let bc = ti.list("menu.boxcursor.coords", 0.0);
        let title = Title {
            fadein: ti.i32("fadein.time", 10),
            fadeout: ti.i32("fadeout.time", 10),
            menu_pos: ti.pair("menu.pos", 160.0, 160.0),
            item_font: font_spec(ti, "menu.item.font"),
            active_font: font_spec(ti, "menu.item.active.font"),
            spacing: ti.pair("menu.item.spacing", 0.0, 13.0),
            items,
            visible: ti.i32("menu.window.visibleitems", 5).max(1) as usize,
            box_visible: ti.i32("menu.boxcursor.visible", 1) != 0,
            box_coords: if bc.len() >= 4 { [bc[0], bc[1], bc[2], bc[3]] } else { [-50.0, -10.0, 50.0, 2.0] },
            snd_move: snd_no(ti, "cursor.move.snd"),
            snd_done: snd_no(ti, "cursor.done.snd"),
            snd_cancel: snd_no(ti, "cancel.snd"),
        };

        let si = sec("select info");
        let cell = |key: &str, d: (i32, i32)| {
            let (r, c) = si.pair(key, d.0 as f32, d.1 as f32);
            (r as i32, c as i32)
        };
        let select = Select {
            fadein: si.i32("fadein.time", 10),
            fadeout: si.i32("fadeout.time", 10),
            rows: si.i32("rows", 2).max(1),
            columns: si.i32("columns", 5).max(1),
            wrapping: si.i32("wrapping", 0) != 0,
            pos: si.pair("pos", 90.0, 170.0),
            show_empty: si.i32("showemptyboxes", 0) != 0,
            move_over_empty: si.i32("moveoveremptyboxes", 0) != 0,
            cell_size: si.pair("cell.size", 27.0, 27.0),
            cell_spacing: si.f32("cell.spacing", 2.0),
            cell_bg: pair_opt(si, "cell.bg.spr"),
            cell_random: pair_opt(si, "cell.random.spr"),
            random_switch: si.i32("cell.random.switchtime", 4).max(1),
            cursor_start: [cell("p1.cursor.startcell", (0, 0)), cell("p2.cursor.startcell", (0, 0))],
            cursor_active: [pair_opt(si, "p1.cursor.active.spr"), pair_opt(si, "p2.cursor.active.spr")],
            cursor_done: [pair_opt(si, "p1.cursor.done.spr"), pair_opt(si, "p2.cursor.done.spr")],
            p2_blink: si.i32("p2.cursor.blink", 0) != 0,
            snd_move: [snd_no(si, "p1.cursor.move.snd"), snd_no(si, "p2.cursor.move.snd")],
            snd_done: [snd_no(si, "p1.cursor.done.snd"), snd_no(si, "p2.cursor.done.snd")],
            snd_stage_move: snd_no(si, "stage.move.snd"),
            snd_stage_done: snd_no(si, "stage.done.snd"),
            snd_cancel: snd_no(si, "cancel.snd"),
            portrait: Placed::read(si, "portrait"),
            title_offset: si.pair("title.offset", 160.0, 13.0),
            title_font: font_spec(si, "title.font"),
            face: [Placed::read(si, "p1.face"), Placed::read(si, "p2.face")],
            name_offset: [si.pair("p1.name.offset", 0.0, 0.0), si.pair("p2.name.offset", 0.0, 0.0)],
            name_font: [font_spec(si, "p1.name.font"), font_spec(si, "p2.name.font")],
            stage_pos: si.pair("stage.pos", 160.0, 237.0),
            stage_active_font: font_spec(si, "stage.active.font"),
            stage_active2_font: font_spec(si, "stage.active2.font"),
            stage_done_font: font_spec(si, "stage.done.font"),
        };

        let vi = sec("vs screen");
        let vs = Versus {
            time: vi.i32("time", 150),
            fadein: vi.i32("fadein.time", 15),
            fadeout: vi.i32("fadeout.time", 15),
            match_text: vi.str("match.text").unwrap_or_default(),
            match_offset: vi.pair("match.offset", 160.0, 12.0),
            match_font: font_spec(vi, "match.font"),
            p: [Placed::read(vi, "p1"), Placed::read(vi, "p2")],
            name_offset: [vi.pair("p1.name.offset", 0.0, 0.0), vi.pair("p2.name.offset", 0.0, 0.0)],
            name_font: [font_spec(vi, "p1.name.font"), font_spec(vi, "p2.name.font")],
        };

        let wi = sec("victory screen");
        let victory = Victory {
            enabled: wi.i32("enabled", 0) != 0,
            time: wi.i32("time", 300),
            fadein: wi.i32("fadein.time", 8),
            fadeout: wi.i32("fadeout.time", 15),
            p1: Placed::read(wi, "p1"),
            name_offset: wi.pair("p1.name.offset", 20.0, 180.0),
            name_font: font_spec(wi, "p1.name.font"),
            quote_text: wi.str("winquote.text").unwrap_or_default(),
            quote_offset: wi.pair("winquote.offset", 20.0, 192.0),
            quote_font: font_spec(wi, "winquote.font"),
            quote_window: window(wi, "winquote.window"),
        };

        let di = sec("demo mode");
        let demo = Demo {
            enabled: di.i32("enabled", 0) != 0,
            waittime: di.i32("title.waittime", 600),
            endtime: di.i32("fight.endtime", 1500),
            bars: di.i32("fight.bars.display", 0) != 0,
        };
        let ws = sec("win screen");
        let win = WinScreen {
            text: ws.str("wintext.text").unwrap_or_else(|| "Congratulations!".to_string()),
            font: font_spec(ws, "wintext.font"),
            offset: ws.pair("wintext.offset", 160.0, 70.0),
            time: ws.i32("pose.time", 300),
        };

        // Los fondos pueden traer su propio SFF.
        let title_bg = BgSet::load(&secs, "titlebg", path, sff, sffs);
        let option_bg = BgSet::load(&secs, "optionbg", path, sff, sffs);
        let oi = sec("option info");
        let option_snd = [snd_no(oi, "cursor.move.snd"), snd_no(oi, "cursor.done.snd"), snd_no(oi, "cancel.snd")];
        let select_bg = BgSet::load(&secs, "selectbg", path, sff, sffs);
        let vs_bg = BgSet::load(&secs, "versusbg", path, sff, sffs);
        let victory_bg = BgSet::load(&secs, "victorybg", path, sff, sffs);

        let fight_def = file("fight").unwrap_or_else(|| format!("{root}/data/fight.def"));
        let select_def = file("select").unwrap_or_else(|| format!("{root}/data/select.def"));
        let mut motif = Motif {
            sff,
            snd,
            fonts,
            anims: air::parse(&text),
            fight_def,
            logo: story(files, "logo.storyboard", false),
            intro: story(files, "intro.storyboard", false),
            gameover: story(sec("game over screen"), "storyboard", true),
            credits: story(sec("end credits"), "storyboard", true),
            title,
            title_bg,
            option_bg,
            option_snd,
            select,
            select_bg,
            vs,
            vs_bg,
            victory,
            victory_bg,
            demo,
            win,
            slots: Vec::new(),
            chars: Vec::new(),
            stages: Vec::new(),
            portraits: 0,
            max_matches: vec![6, 1, 1],
        };
        motif.load_roster(root, &select_def, sffs)?;
        Ok(motif)
    }

    fn add_stage(&mut self, root: &str, file: &str) -> Option<usize> {
        let path = format!("{root}/{}", file.trim().replace('\\', "/"));
        if let Some(i) = self.stages.iter().position(|s| s.path == path) {
            return Some(i);
        }
        let name = stage_name(&path)?;
        self.stages.push(StageEntry { path, name });
        Some(self.stages.len() - 1)
    }

    /// Lee `select.def`: personajes (con su retrato), escenarios y opciones.
    fn load_roster(&mut self, root: &str, path: &str, sffs: &mut Vec<Sff>) -> Result<(), String> {
        let text = ini::read_text(path).map_err(|e| format!("{path}: {e}"))?;
        let mut portraits = Sff::default();
        let mut section = String::new();
        for raw in text.lines() {
            let line = ini::clean(raw);
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                section = line.trim_matches(|c| c == '[' || c == ']').trim().to_ascii_lowercase();
                continue;
            }
            match section.as_str() {
                "characters" => {
                    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
                    let name = parts[0];
                    if name.eq_ignore_ascii_case("randomselect") {
                        self.slots.push(Slot::Random);
                        continue;
                    }
                    if name.eq_ignore_ascii_case("blank") {
                        self.slots.push(Slot::Empty);
                        continue;
                    }
                    let (mut stage, mut order, mut include) = (None, 1, true);
                    for p in &parts[1..] {
                        match p.split_once('=') {
                            Some((k, v)) => match k.trim().to_ascii_lowercase().as_str() {
                                "order" => order = ini::num(v).unwrap_or(1.0) as i32,
                                "includestage" => include = ini::num(v).unwrap_or(1.0) != 0.0,
                                _ => {}
                            },
                            None if !p.is_empty() && !p.eq_ignore_ascii_case("random") => stage = Some(p.to_string()),
                            None => {}
                        }
                    }
                    let def = char_def(root, name);
                    // Un personaje que no se puede leer deja su casilla vacía.
                    match self.load_char(&def, &mut portraits) {
                        Some(mut ch) => {
                            ch.order = order;
                            if let Some(file) = stage {
                                let known = self.stages.len();
                                ch.stage = self.add_stage(root, &file);
                                if !include && self.stages.len() > known {
                                    // Sólo para su combate de arcade: no se ofrece en la lista.
                                    let entry = self.stages.pop().unwrap();
                                    self.stages.push(StageEntry { name: String::new(), ..entry });
                                }
                            }
                            self.slots.push(Slot::Char(self.chars.len()));
                            self.chars.push(ch);
                        }
                        None => self.slots.push(Slot::Empty),
                    }
                }
                "extrastages" => {
                    self.add_stage(root, line);
                }
                "options" => {
                    if let Some(v) = line.strip_prefix("arcade.maxmatches").and_then(|r| r.split_once('=')).map(|x| x.1) {
                        self.max_matches = ini::nums(v, 0.0).iter().map(|&n| n as i32).collect();
                    }
                }
                _ => {}
            }
        }
        if self.chars.is_empty() {
            return Err(format!("{path}: no hay ningún personaje utilizable"));
        }
        if self.stages.iter().all(|s| s.name.is_empty()) {
            return Err(format!("{path}: no hay ningún escenario utilizable"));
        }
        sffs.push(portraits);
        self.portraits = sffs.len() as u16 - 1;
        Ok(())
    }

    /// Datos de un personaje para la selección: nombre y retratos.
    fn load_char(&self, def: &str, portraits: &mut Sff) -> Option<RosterChar> {
        let secs = ini::load(def).ok()?;
        let info = secs.iter().find(|s| s.name == "info")?;
        let files = secs.iter().find(|s| s.name == "files")?;
        // Sólo hacen falta los retratos: no se descomprime el resto.
        let sff = Sff::load_mode(&ini::sibling(def, &files.str("sprite")?), Some(true)).ok()?;
        let index = self.chars.len() as u16;
        let mut found = [None; 3];
        for (n, slot) in found.iter_mut().enumerate() {
            let Some(si) = sff.find(9000, n as i32) else { continue };
            let src = &sff.sprites[si as usize];
            let Some(pix) = sff.with_pixels(si, |p| p.to_vec()) else { continue };
            portraits.pals.push(sff.pals[src.pal as usize]);
            portraits.sprites.push(Sprite::new(index, n as u16, src.w, src.h, src.ax, src.ay, portraits.pals.len() as u16 - 1, pix));
            *slot = Some(portraits.sprites.len() as u32 - 1);
        }
        let story = |key: &str| {
            let arcade = secs.iter().find(|s| s.name == "arcade")?;
            arcade.str(key).filter(|f| !f.is_empty()).map(|f| ini::sibling(def, &f))
        };
        Some(RosterChar {
            def: def.to_string(),
            name: info.str("displayname").or_else(|| info.str("name")).unwrap_or_default(),
            stage: None,
            order: 1,
            portraits: found,
            intro: story("intro.storyboard"),
            ending: story("ending.storyboard"),
        })
    }

    /// Escenarios que se pueden elegir a mano.
    pub fn listed_stages(&self) -> Vec<usize> {
        (0..self.stages.len()).filter(|&i| !self.stages[i].name.is_empty()).collect()
    }
}
