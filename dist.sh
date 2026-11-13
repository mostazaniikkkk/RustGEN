#!/bin/sh
# Genera los ejecutables finales en dist/:
#   mugen-ps2.elf   para lanzar con Free McBoot / uLaunchELF (USB o tarjeta).
#                   Lleva dentro los datos de game/.
#   mugen-ps2.iso   imagen de disco. Los datos van aparte, en GAME.PAK, y se
#                   leen del disco al cargar cada pantalla o pelea.
set -e
cd "$(dirname "$0")"
# Sin configuración de prueba dentro del paquete.
rm -rf target/ps2test
mkdir -p target/ps2test dist
cargo build --release -p mugen-host
cargo ps2 build -p mugen-ps2 -- --features disc
target/release/mugen-host iso target/ps2/mugen-ps2.elf dist/mugen-ps2.iso target/game.pak
cargo ps2 build -p mugen-ps2
cp target/ps2/mugen-ps2.elf dist/mugen-ps2.elf
ls -l dist
