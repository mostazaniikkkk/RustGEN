#!/bin/sh
# Verificación completa: pruebas del motor en el PC y comparación de la
# misma pelea ejecutada en el PC y en la PS2 (PCSX2).
#
#   ./verify.sh              pruebas y comparación
#   ./verify.sh --captura    además, captura la ventana de PCSX2
#
# Necesita PCSX2 con BIOS (ruta en la variable de entorno PCSX2_EXE) y el
# compilador de Rust para PS2 (`cargo ps2`, de PS2Rust).
cd "$(dirname "$0")"
mkdir -p out
fail=0
step() { printf '\n=== %s\n' "$1"; }
ok() { if [ "$1" -eq 0 ]; then echo "  -> OK"; else echo "  -> FALLO"; fail=$((fail + 1)); fi; }

step "Pruebas del motor y de la herramienta (PC)"
cargo test --release -p mugen -p mugen-host 2>&1 | grep -E 'test result|FAILED|panicked' | grep -v ' 0 passed'
cargo test --release -p mugen -p mugen-host >/dev/null 2>&1
ok $?

step "Compilar la herramienta de PC y el juego para PS2"
cargo build --release 2>&1 | grep -E '^(warning|error)' -A 6
cargo ps2 build -p mugen-ps2 2>&1 | grep -E 'R5900|ELF|^error' -A 6
[ -f target/ps2/mugen-ps2.elf ] && [ -f target/release/mugen-host.exe ]
ok $?

# Ejecuta la misma prueba en el PC y en la PS2 y compara el estado tick a tick.
compare() {
    name="$1"; cfg="$2"; shift 2
    step "PS2 contra PC: $name"
    sh run-ps2.sh test "$cfg" --timeout 400 --log "out/$name.ps2.log" >"out/$name.ps2.out" 2>&1
    grep -E 'frame:|audio: [0-9]|pad:' "out/$name.ps2.log" | sed 's/^/  /'
    target/release/mugen-host.exe "$@" | grep '^t[0-9]' >"out/$name.pc.txt"
    grep '^t[0-9]' "out/$name.ps2.log" >"out/$name.ps2.txt"
    n=$(wc -l <"out/$name.pc.txt")
    echo "  $n líneas de estado; última: $(tail -1 "out/$name.ps2.txt")"
    [ "$n" -gt 10 ] && python tests/compare.py "out/$name.pc.txt" "out/$name.ps2.txt"
    ok $?
}
compare ai3600 tests/ai3600.cfg run 3600 --ai1 --log 100
compare keys720 tests/keys720.cfg run 720 --keys tests/p1.keys --log 20
compare menu tests/menu.cfg run 1500 --motif data/system.def --keys tests/menu.keys --screens --log 50
compare arcade tests/arcade.cfg run 2400 --motif data/kfm/system.def --keys tests/arcade.keys --ai1 --screens --log 100
compare stream tests/stream.cfg run 3600 --ai1 --log 100
grep -E 'bajo demanda|memoria libre' out/stream.ps2.log | tail -2 | sed 's/^/  /'
echo "  sin carga bajo demanda: $(grep 'memoria libre' out/ai3600.ps2.log | tail -1)"

step "Arranque desde una imagen de disco (ISO), con los datos leídos del disco"
rm -rf target/ps2test && mkdir -p target/ps2test
cp tests/menu.cfg target/ps2test/ps2test.cfg && cp tests/*.keys target/ps2test/
cargo ps2 build -p mugen-ps2 -- --features disc >out/iso.build.out 2>&1
target/release/mugen-host.exe iso target/ps2/mugen-ps2.elf out/test.iso target/game.pak
python tests/run-iso.py out/test.iso out/iso.ps2.log 300
r=$?
grep -E 'Detected PS2 Disc|paquete de datos|resguardo' out/iso.ps2.log | sort -u | sed 's/^/  /'
grep '^t[0-9]' out/iso.ps2.log >out/iso.ps2.txt
[ $r -eq 0 ] && python tests/compare.py out/menu.pc.txt out/iso.ps2.txt
ok $?

step "Sonido y mandos en la PS2"
voices=$(grep -c 'envolvente 0x7fff' out/ai3600.ps2.log)
echo "  voces del SPU2 sonando a todo volumen: $voices"
grep -q 'pad: estado 6' out/ai3600.ps2.log && [ "$voices" -gt 20 ]
ok $?

if [ "$1" = "--captura" ]; then
    # Captura la ventana de PCSX2: no uses el PC mientras tanto, o saldrá
    # lo que tengas delante.
    step "Captura de pantalla de la PS2"
    sh run-ps2.sh test tests/ai300.cfg --timeout 200 --screenshot out/ps2.png >out/shot.out 2>&1
    target/release/mugen-host.exe run 300 --ai1 --shots 299 --out out >/dev/null
    [ -f out/ps2.png ] && [ -f out/f00299.png ]
    ok $?
    echo "  PS2: out/ps2.png   PC (mismo tick): out/f00299.png"
fi

# Deja el ELF y la imagen normales (sin configuración de prueba).
sh dist.sh >out/dist.out 2>&1
step "Ejecutables finales"
ls -l dist/mugen-ps2.elf dist/mugen-ps2.iso 2>&1 | awk '{print "  " $5 " bytes  " $NF}'
[ -f dist/mugen-ps2.elf ] && [ -f dist/mugen-ps2.iso ]
ok $?

printf '\n=== Resumen: '
if [ $fail -eq 0 ]; then echo "TODO OK"; else echo "$fail paso(s) fallaron"; fi
exit $fail
