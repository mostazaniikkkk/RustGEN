#!/bin/sh
# Compila el juego para PS2 (con los datos de game/ dentro del ELF) y lo
# ejecuta en PCSX2.
#   ./run-ps2.sh                      jugar
#   ./run-ps2.sh test <opciones.cfg>  prueba automática (ver tests/*.cfg)
# Los argumentos restantes se pasan a `cargo ps2 run`.
set -e
cd "$(dirname "$0")"
mode="$1"
[ $# -gt 0 ] && shift
# Lo que haya en target/ps2test se empaqueta junto a game/.
rm -rf target/ps2test
mkdir -p target/ps2test
if [ "$mode" = test ]; then
    cfg="$1"
    shift
    cp "$cfg" target/ps2test/ps2test.cfg
    cp tests/*.keys target/ps2test/
    # Los personajes de prueba, junto a los del juego.
    cp -r tests/chars target/ps2test/
    exec cargo ps2 run -p mugen-ps2 --until "prueba terminada" "$@"
fi
exec cargo ps2 run -p mugen-ps2 --keep-open --timeout 100000 "$@"
