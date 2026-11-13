//! Motor de peleas compatible con M.U.G.E.N, independiente de la plataforma.
//!
//! Lee los formatos originales (SFF, SND, FNT, AIR, CMD, CNS, DEF), simula la
//! pelea a 60 ticks por segundo y entrega por frame una lista de dibujo y de
//! sonidos que cada plataforma (PC, PlayStation 2) presenta a su manera.

pub mod adpcm;
pub mod ai;
pub mod app;
pub mod bg;
pub mod air;
pub mod cmd;
pub mod cns;
pub mod ctrl;
pub mod eval;
pub mod expr;
pub mod fnt;
pub mod fx;
pub mod game;
pub mod gfx;
pub mod hud;
pub mod ini;
pub mod motif;
pub mod png;
pub mod sff;
pub mod script;
pub mod snd;
pub mod stage;
pub mod vfs;
pub mod story;
