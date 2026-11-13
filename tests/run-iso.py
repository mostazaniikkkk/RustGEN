"""Arranca una imagen de disco en PCSX2 y guarda la consola del juego.

    python tests/run-iso.py <imagen.iso> <salida.log> [segundos]

Termina cuando el juego imprime "prueba terminada" (código 0) o se agota el
tiempo (código 1). La ruta de PCSX2 se lee de .cargo/config.toml.
"""
import os
import re
import subprocess
import sys
import time

iso, out = sys.argv[1], sys.argv[2]
timeout = float(sys.argv[3]) if len(sys.argv) > 3 else 120
here = os.path.dirname(os.path.abspath(__file__))
cfg = open(os.path.join(here, "..", ".cargo", "config.toml"), encoding="utf-8").read()
exe = os.environ.get("PCSX2_EXE") or re.search(r'PCSX2_EXE\s*=\s*"([^"]+)"', cfg).group(1)
log = os.path.abspath(out) + ".emu"
if os.path.exists(log):
    os.remove(log)
args = [exe, "-batch", "-fastboot", "-nofullscreen", "-logfile", log, "--", os.path.abspath(iso)]
p = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
start, done = time.time(), False
stamp = re.compile(r"^\[\s*[\d.,]+\]\s?")
lines = []
while time.time() - start < timeout and p.poll() is None and not done:
    time.sleep(0.2)
    if os.path.exists(log):
        text = open(log, encoding="utf-8", errors="replace").read()
        lines = [stamp.sub("", line) for line in text.splitlines()]
        done = any("prueba terminada" in line for line in lines)
p.kill()
p.wait()
open(out, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
if os.path.exists(log):
    os.remove(log)
sys.exit(0 if done else 1)
