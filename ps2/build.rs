//! Empaqueta la carpeta `game/`. El paquete va dentro del ejecutable (así
//! el ELF funciona solo, se lance desde donde se lance) o, en la versión
//! para disco, en `target/game.pak`, que se graba junto al ELF.
//!
//! Si existe `target/ps2test/`, sus archivos se añaden al paquete (lo usan
//! las pruebas automáticas para meter su configuración).

use std::path::Path;
use std::{env, fs};

fn main() {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("..");
    let game = root.join("game");
    let overlay = root.join("target").join("ps2test");
    // La carpeta de pruebas existe siempre (vacía si no hay prueba), para
    // que cargo pueda vigilarla.
    fs::create_dir_all(&overlay).unwrap();
    println!("cargo:rerun-if-changed={}", game.display());
    println!("cargo:rerun-if-changed={}", overlay.display());
    let pak = mugen::vfs::pack(&[(game.as_path(), "game"), (overlay.as_path(), "game")]).expect("no pude empaquetar game/");
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("game.pak"), &pak).unwrap();
    // Copia a la vista, para meterla en la imagen de disco.
    fs::write(root.join("target").join("game.pak"), &pak).unwrap();
}
