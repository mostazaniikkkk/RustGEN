"""Compara dos registros de estado (PC y PS2).

Todo debe coincidir, salvo las posiciones, que pueden diferir en 1 píxel: la
FPU de la PS2 redondea hacia cero y un valor como 21.999998 se imprime 21.
"""
import re
import sys

a = open(sys.argv[1]).read().split("\n")
b = open(sys.argv[2]).read().split("\n")
if len(a) != len(b):
    print(f"  distinto numero de lineas: {len(a)} y {len(b)}")
    sys.exit(1)
pos = re.compile(r"([xy])(-?\d+)")
near = 0
for n, (la, lb) in enumerate(zip(a, b), 1):
    if la == lb:
        continue
    pa, pb = pos.findall(la), pos.findall(lb)
    same_rest = pos.sub(r"\1", la) == pos.sub(r"\1", lb)
    close = len(pa) == len(pb) and all(abs(int(x[1]) - int(y[1])) <= 1 for x, y in zip(pa, pb))
    if not (same_rest and close):
        print(f"  linea {n}:\n    PC : {la}\n    PS2: {lb}")
        sys.exit(1)
    near += 1
print(f"  {len(a) - near} lineas identicas; {near} con posiciones a 1 pixel")
