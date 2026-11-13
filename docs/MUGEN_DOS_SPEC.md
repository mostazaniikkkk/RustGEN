# Especificación de referencia: M.U.G.E.N para DOS (1999–2001)

Documento de trabajo para el clon de MUGEN en PS2. Objetivo: cargar y hacer jugable a
Kung Fu Man (KFM) tal como venía con las versiones DOS.

> **Estado (12 de noviembre de 2026).** El motor está hecho y corre en PS2: ver el
> [README](../README.md). Este documento queda como notas de formato. Lo que se confirmó
> al implementarlo, y lo que no, está en la sección 8.

Leyenda de confianza:

- **[V]** verificado en esta investigación contra una fuente (ver "Fuentes").
- **[M]** de memoria / conocimiento general de la documentación de Elecbyte; hay que
  contrastarlo contra los archivos reales de KFM y los docs del zip de DOS antes de darlo
  por bueno.

---

## 1. Contexto histórico y de plataforma

| Dato | Valor | Conf. |
|---|---|---|
| Primera beta pública | 27 de julio de 1999 (build "1999.07.27"). Algunas fuentes dicen 17/07 o 27/06 | [V] (con conflicto entre fuentes) |
| Builds DOS siguientes | 16/08/1999, 03/09/1999, 19/10/1999, … | [V] |
| Última build DOS | 2001.04.14 (publicada en mayo de 2001) | [V] una fuente |
| Fin del desarrollo DOS | Noviembre de 2001 (Elecbyte pasa a Linux) | [V] |
| Lenguaje / toolchain | C, DJGPP (gcc 2.95.2), librería Allegro (4.0.0 en las últimas) | [V] |
| CPU mínima / recomendada | Pentium 90 / Pentium 200 | [V] |
| RAM | ~32 MB "debería alcanzar"; depende de chars y stages | [V] |
| Vídeo | 320x240 en color de 16 bits, VESA2 lineal por defecto; opción `stretch` a 640x480 | [V] |
| Sonido | Sound Blaster 16 y compatibles | [V] |
| Música de stage | MIDI, MOD/S3M, MP3 | [V] |
| Entrada | Teclado y gamepad digital (sin analógico) | [V] |
| Uso para probar un char | `MUGEN player player` (después `mugen kfm kfm`) | [V] |
| KFM original | Vino con el MUGEN de DOS hasta la 2000.06.27; después lo reemplazó un KFM rehecho | [V] una fuente |

Formatos de archivo de la era DOS (el FAQ de la época los lista): `.def`, `.cns`, `.cmd`,
`.air` (texto) y `.sff`, `.snd`, `.act`, `.fnt` (binarios). **[V]**

### Qué significa esto para PS2

- 320x240 a 60 ticks/s calza natural con la PS2 (NTSC ~59.94 Hz, modo no entrelazado).
- 32 MB de RAM en la PC de la época = 32 MB de RAM principal de la PS2. Presupuesto parecido.
- Los sprites son de 8 bits con paleta: se mapean directo a texturas CLUT de 8 bits del GS
  (PSMT8), sin convertir a color real. El cambio de paleta (`.act`) es cambiar la CLUT.
- Los sonidos son WAV PCM: se pueden mandar al SPU2 como PCM o convertir a ADPCM al cargar.

---

## 2. Archivos de un personaje

Un char es una carpeta `chars/kfm/` con:

| Archivo | Tipo | Contenido |
|---|---|---|
| `kfm.def` | texto | Índice: nombre, autor y rutas del resto de los archivos |
| `kfm.sff` | binario | Sprites (PCX de 8 bits empaquetados) |
| `kfm.air` | texto | Animaciones y cajas de colisión |
| `kfm.cns` | texto | Constantes del char y estados (la "lógica") |
| `kfm.cmd` | texto | Comandos (secuencias de botones) y estado -1 |
| `kfm.snd` | binario | Sonidos (WAV empaquetados) |
| `kfm.act`, `kfm2.act`… | binario | Paletas alternativas |
| `common1.cns` (en `data/`) | texto | Estados comunes a todos los chars (parado, caminar, salto, recibir golpe…) |

Sintaxis común de los archivos de texto **[M]**: estilo INI. Secciones `[Nombre]`,
pares `clave = valor`, comentarios con `;` hasta fin de línea, no distingue mayúsculas
en claves y nombres de sección.

### 2.1 `.def`

```ini
[Info]
name = "Kung Fu Man"
displayname = "Kung Fu Man"
versiondate = 09,30,1999
mugenversion = 04,14,2001
author = "Elecbyte"
pal.defaults = 6,3,4,2

[Files]
cmd     = kfm.cmd
cns     = kfm.cns
st      = kfm.cns       ; archivo(s) de estados
stcommon = common1.cns
sprite  = kfm.sff
anim    = kfm.air
sound   = kfm.snd
pal1    = kfm6.act
...
[Arcade]
intro.storyboard  =
ending.storyboard =
```

(Valores de ejemplo. **[M]**)

---

## 3. Formatos binarios

Todo en little-endian (igual que la PS2, no hay que invertir bytes).

### 3.1 SFF v1 (sprites) **[V]**

Las versiones 2002.04.12 y anteriores usan SFF v1; es lo único que hay que soportar.

**Cabecera del archivo (512 bytes)**

| Offset | Tam. | Campo |
|---|---|---|
| 0 | 12 | Firma `"ElecbyteSpr\0"` |
| 12 | 4 | Versión: verlo3, verlo2, verlo1, verhi (verhi = 1) |
| 16 | 4 | Cantidad de grupos **[M]** |
| 20 | 4 | Cantidad de imágenes |
| 24 | 4 | Offset del primer subarchivo (normalmente 512) |
| 28 | 4 | Tamaño de la subcabecera (32) **[M]** |
| 32 | 1 | Tipo de paleta: 1 = compartida, 0 = individual **[M]** |
| 33 | 3 | Sin uso |
| 36 | 476 | Comentarios |

**Subcabecera de cada sprite (32 bytes)** — forman una lista enlazada por el primer campo.

| Offset | Tam. | Campo |
|---|---|---|
| 0 | 4 | Offset absoluto del siguiente subarchivo |
| 4 | 4 | Largo de los datos (sin la cabecera). **0 = sprite enlazado** |
| 8 | 2 | Eje X (i16) |
| 10 | 2 | Eje Y (i16) |
| 12 | 2 | Número de grupo |
| 14 | 2 | Número de imagen |
| 16 | 2 | Índice del sprite del que se copia (solo si largo = 0) |
| 18 | 1 | 1 = usa la misma paleta que el sprite anterior |
| 19 | 13 | Sin uso |
| 32 | — | Archivo PCX completo |

**PCX embebido**

- Cabecera PCX de 128 bytes. Campos que importan: byte 2 = codificación (1 = RLE),
  byte 3 = bits por píxel (tiene que ser 8), bytes 4–11 = xmin, ymin, xmax, ymax (u16),
  bytes 66–67 = bytes por línea.
- Ancho = xmax − xmin + 1; alto = ymax − ymin + 1.
- Píxeles desde el byte 128. RLE: un byte `>= 0xC0` es una repetición, cantidad =
  `byte & 0x3F`, el valor es el byte siguiente; cualquier otro byte es un píxel literal.
  Las repeticiones no cruzan el fin de línea; "bytes por línea" puede ser mayor que el
  ancho y ese sobrante se descarta.
- Paleta: últimos 768 bytes del bloque (256 × RGB), precedidos por un byte marcador
  `0x0C`. Si el flag "misma paleta" está en 1, esos 768 bytes no están.
- El índice de color 0 es transparente.

**Casos borde**

- Sprite enlazado (largo 0): reutiliza los píxeles del sprite de índice indicado. Si el
  índice es inválido se ignora.
- Si un par (grupo, imagen) se repite, vale el primero.
- En un char con paleta compartida, la paleta "del personaje" es la del primer sprite
  (y es la que reemplaza el `.act`). Sprites con paleta propia (efectos, por ejemplo)
  no cambian con el `.act`.

Grupos de sprite reservados por convención **[M]**: 9000,0 = retrato chico (selección),
9000,1 = retrato grande; 5000–5079 = sprites obligatorios de "recibir golpe".

### 3.2 SND (sonidos) **[V]**

**Cabecera (512 bytes)**

| Offset | Tam. | Campo |
|---|---|---|
| 0 | 12 | Firma `"ElecbyteSnd\0"` |
| 12 | 4 | Versión |
| 16 | 4 | Cantidad de sonidos |
| 20 | 4 | Offset del primer subarchivo |
| 24 | 488 | Comentarios |

**Subcabecera (16 bytes)**, también lista enlazada:

| Offset | Tam. | Campo |
|---|---|---|
| 0 | 4 | Offset del siguiente subarchivo (0 o fuera de rango = fin) |
| 4 | 4 | Largo de los datos |
| 8 | 4 | Número de grupo |
| 12 | 4 | Número de sonido |
| 16 | — | Archivo WAV completo (RIFF, PCM de 8 o 16 bits) **[M]** |

### 3.3 ACT (paleta) **[M]**

768 bytes: 256 colores RGB. Es el formato de paleta de Photoshop, y MUGEN lo lee
**en orden invertido**: el color del índice 0 está al final del archivo.
Hay que confirmarlo comparando un `.act` de KFM con la paleta del primer sprite del SFF.

### 3.4 FNT v1 (fuentes) **[V]**

Un solo archivo binario (la FNT v2 con `.def` es de MUGEN 1.0, no aplica).

| Offset | Tam. | Campo |
|---|---|---|
| 0 | 12 | Firma `"ElecbyteFnt\0"` |
| 12 | 4 | Versión (2 × u16) |
| 16 | 4 | Offset del PCX |
| 20 | 4 | Largo del PCX |
| 24 | 4 | Offset del texto |
| 28 | 4 | Largo del texto |
| 32 | 40 | Comentarios **[M]** |

- El PCX es una tira horizontal con todos los glifos, mismo decodificado que en SFF
  (píxeles desde +128, paleta en los últimos 768 bytes).
- El texto es un INI con dos secciones:
  - `[Def]`: `Size = ancho,alto`, `Spacing = x,y`, `Colors = n`, `Offset = x,y`,
    `Type = Fixed | Variable`.
  - `[Map]`: una línea por carácter: `carácter [x_inicial [ancho]]`. El carácter puede
    ser literal o `0x41`. Sin x/ancho se toma la posición corrida y el ancho de `Size`.
- Bancos de color: `Colors` se redondea a múltiplo de 16 (`c`); hay `255 / c` bancos.
  Cada banco usa un bloque distinto de `c` colores tomado desde el final de la paleta.

---

## 4. Formatos de texto del personaje

Esta sección es **[M]** casi entera: viene de la documentación de Elecbyte (air, cns, cmd).
Los archivos reales de KFM son la prueba definitiva de qué hay que soportar.

### 4.1 AIR (animaciones)

```ini
[Begin Action 200]
Clsn2Default: 2
 Clsn2[0] = -10,  0, 10,-79
 Clsn2[1] =  -4,-92,  6,-79
200,0, 0,0, 3
Clsn1: 1
 Clsn1[0] = 16,-80, 48,-67
200,1, 0,0, 5, H, A
Loopstart
200,2, 0,0, -1
```

- Cada línea de elemento: `grupo, imagen, offsetX, offsetY, tiempo [, flip [, trans]]`.
- `tiempo` en ticks (1/60 s); `-1` = se queda para siempre en ese cuadro.
- `flip`: `H`, `V` o `VH`.
- `trans`: `A` (suma), `A1` (suma con fuente al 50 %), `S` (resta). El formato
  `AS###D###` es posterior.
- `Clsn1` = cajas de ataque, `Clsn2` = cajas donde te pueden pegar. Cada caja es
  `x1,y1,x2,y2` relativo al eje del char. `Clsn?Default` aplica a todos los cuadros
  siguientes hasta que otro default lo reemplace; sin `Default` aplica solo al cuadro
  siguiente.
- `Loopstart` marca desde dónde se repite la animación.
- Sprite `-1,0` = cuadro en blanco.

Números de animación reservados: 0 parado, 5 girar, 10–12 agacharse, 20/21 caminar,
40–47 salto, 100/105 correr, 120–152 guardia, 5000+ recibir golpe, 5300 mareado, etc.

### 4.2 CMD (comandos)

```ini
[Command]
name = "QCF_x"
command = ~D, DF, F, x
time = 15

[Statedef -1]
[State -1, Hadoken]
type = ChangeState
value = 1000
triggerall = command = "QCF_x"
trigger1 = statetype = S
trigger1 = ctrl
```

- Direcciones: `B, DB, D, DF, F, UF, U, UB` (relativas a hacia dónde mira el char).
- Botones: `a, b, c, x, y, z, s`.
- Modificadores: `/` mantener, `~` soltar (`~30a` = soltar tras cargar 30 ticks),
  `$` dirección en 4 sentidos (`$D` acepta DB, D, DF), `+` simultáneo,
  `>` sin otra tecla en el medio.
- `time` = ventana total en ticks (por defecto 15). `[Defaults]` con `command.time`.
- `[Statedef -1]` se evalúa todos los ticks y es donde los comandos se transforman
  en cambios de estado.

### 4.3 CNS (constantes y estados)

**Constantes**

- `[Data]`: `life`, `attack`, `defence`, `fall.defence_up`, `liedown.time`, `airjuggle`,
  `sparkno`, `guard.sparkno`, `KO.echo`, `volume`.
- `[Size]`: `xscale`, `yscale`, `ground.back`, `ground.front`, `air.back`, `air.front`,
  `height`, `attack.dist`, `proj.attack.dist`, `proj.doscale`, `head.pos`, `mid.pos`,
  `shadowoffset`.
- `[Velocity]`: `walk.fwd`, `walk.back`, `run.fwd`, `run.back`, `jump.neu`, `jump.back`,
  `jump.fwd`, `runjump.back`, `runjump.fwd`, `airjump.neu`, `airjump.back`, `airjump.fwd`.
- `[Movement]`: `airjump.num`, `airjump.height`, `yaccel`, `stand.friction`,
  `crouch.friction`.

**Estados**

```ini
[Statedef 200]
type      = S        ; S parado, C agachado, A en el aire, L tirado, U sin cambio
movetype  = A        ; A atacando, I inactivo, H recibiendo golpe
physics   = S        ; S, C, A, N (ninguna)
anim      = 200
velset    = 0,0
ctrl      = 0
poweradd  = 10
juggle    = 1

[State 200, 1]
type = HitDef
trigger1 = AnimElem = 3
attr = S, NA
damage = 23, 0
animtype = Light
guardflag = MA
hitflag = MAF
pausetime = 8, 8
sparkno = 0
hitsound = 5, 0
guardsound = 6, 0
ground.type = High
ground.slidetime = 5
ground.hittime = 10
ground.velocity = -4
air.velocity = -1.4,-3

[State 200, 2]
type = ChangeState
trigger1 = AnimTime = 0
value = 0
ctrl = 1
```

- Un controlador se ejecuta si se cumplen todas las `triggerall` y todas las líneas de
  algún grupo `triggerN` (AND dentro del grupo, OR entre grupos).
- `persistent`: 1 = cada tick (por defecto), 0 = una vez por estado, n = cada n ticks.
- Estados especiales: `-1` (comandos), `-2` y `-3` (se ejecutan siempre).
- Estados comunes (`common1.cns`): 0 parado, 10–12 agacharse, 20 caminar, 40–52 salto,
  100/105 correr y salto atrás, 120–155 guardia, 5000–5210 recibir golpe y caer,
  5900 inicialización, 170/175/180 perder por tiempo / empate / ganar, 190 intro.

**Controladores de estado esperables en un KFM de DOS** (lista a confirmar con un
`grep` sobre `kfm.cns` y `common1.cns`):
`ChangeState`, `ChangeAnim`, `HitDef`, `VelSet`, `VelAdd`, `VelMul`, `PosSet`, `PosAdd`,
`CtrlSet`, `StateTypeSet`, `PlaySnd`, `Projectile`, `Explod`, `VarSet`, `VarAdd`,
`PowerAdd`, `Turn`, `Width`, `NotHitBy`, `HitBy`, `AssertSpecial`, `SuperPause`, `Pause`,
`EnvShake`, `AfterImage`, `MakeDust`, `SelfState`, `TargetState`, `TargetBind`,
`HitFallDamage`, `HitFallVel`, `HitVelSet`, `PlayerPush`, `SprPriority`, `Gravity`.

**Triggers esperables:** `Time`, `AnimTime`, `AnimElem`, `Anim`, `command`, `ctrl`,
`StateType`, `MoveType`, `StateNo`, `Vel X/Y`, `Pos X/Y`, `P2BodyDist X`, `P2Dist X`,
`MoveContact`, `MoveHit`, `MoveGuarded`, `HitShakeOver`, `HitOver`, `HitFall`, `Life`,
`Power`, `Var(n)`, `Random`, `RoundState`, `Alive`, `GameTime`, `FrontEdgeBodyDist`,
`BackEdgeBodyDist`.

**Diferencia importante entre builds DOS (a confirmar):** en las primeras builds los
triggers eran comparaciones simples (`trigger1 = Time = 0`). El evaluador de expresiones
completo (aritmética, floats, `&&`, `||`, funciones como `ifelse`, expresiones en casi
cualquier parámetro), las variables float `fvar` y los `Helper` llegaron en builds
posteriores de 2000–2001. Qué tan complejo tiene que ser nuestro parser depende de
qué KFM se cargue.

### 4.4 Reglas del motor

- 60 ticks por segundo; toda duración se mide en ticks.
- Coordenadas de juego 320x240. El eje del char es el punto entre los pies; Y negativo
  es hacia arriba; el suelo es Y = 0 (en pantalla lo ubica el `zoffset` del stage).
- X de velocidades y cajas es relativa a hacia dónde mira el char.
- Orden por tick (aprox.): leer entrada → actualizar buffer de comandos → estados
  -3, -2, -1 → estado actual → física (velocidad, gravedad, fricción) → avanzar
  animación → detección de golpes (Clsn1 contra Clsn2) → empuje entre jugadores → cámara.
- Vida por defecto 1000, barra de power hasta 3000 (3 niveles).
- Al recibir un golpe el char entra en estados 5000+ controlados por `common1.cns`
  con los valores del `HitDef` del atacante.

---

## 5. Resto del motor (no es del char pero hace falta para una pelea)

**[M]** — archivos en `data/`:

- `mugen.cfg`: opciones (velocidad, vídeo, sonido, teclas, `PlayerCache`).
- `system.def`: pantalla de título, menú, selección de personaje, versus.
- `select.def`: lista de chars y stages.
- `fight.def` + `fight.sff` + `fight.snd`: barras de vida y power, retratos, timer,
  textos de "Round 1 / Fight / KO", chispas de golpe comunes.
- `common1.cns`, `common.snd`.
- `stages/*.def`: `[Info]`, `[Camera]` (límites, seguimiento), `[PlayerInfo]`
  (posiciones iniciales), `[Bound]`, `[StageInfo]` (`zoffset`), `[Shadow]`, `[Music]`,
  `[BGdef]` (SFF del fondo) y capas `[BG nombre]` de tipo `normal`, `anim` o `parallax`
  con `spriteno`, `start`, `delta`, `tile`, `velocity`, `mask`, `trans`.

Alcance mínimo para "KFM contra KFM": char completo + un stage + `fight.def` reducido
(barras y timer). Menús, select y storyboards se pueden dejar para el final.

---

## 6. Plan de módulos (pensado para Rust `no_std`)

Todo lo que es parseo y lógica no depende de la plataforma: se puede escribir ya, solo
con `core` (sin `alloc` si hace falta), y probarlo en la PC con `cargo test` contra los
archivos reales de KFM. Cuando el compilador para PS2 esté listo, se enchufa el backend.

| Módulo | Depende de la PS2 | Se puede empezar ya |
|---|---|---|
| Decodificador PCX + lector SFF v1 | No | Sí |
| Lector SND + cabecera WAV | No | Sí |
| Lector ACT y FNT v1 | No | Sí |
| Tokenizador INI común (def/air/cmd/cns) | No | Sí |
| Parser AIR + reproductor de animación | No | Sí |
| Parser CMD + buffer de entrada y matcher de comandos | No | Sí |
| Parser CNS + evaluador de triggers + máquina de estados | No | Sí |
| Física, colisiones Clsn, HitDef / gethit | No | Sí |
| Matemática de punto fijo o float (la FPU de la PS2 es de 32 bits, sin double) | Parcial | Sí, eligiendo `f32` |
| Render (GS, texturas PSMT8 + CLUT) | Sí | No |
| Audio (SPU2) | Sí | No |
| Entrada (pad) y carga de archivos (CD/USB/host) | Sí | No |

Detalles de PS2 a tener en cuenta al diseñar los datos:

- Texturas del GS: lados potencia de 2 (hasta 1024). Los sprites de KFM hay que
  empaquetarlos en atlas o rellenar a potencia de 2.
- 4 MB de VRAM: con 8 bits por píxel entra un char entero, pero no dos chars + stage
  sin administrar qué se sube. Conviene subir por demanda los sprites de la animación
  en curso.
- La CLUT de 8 bits del GS se guarda con las entradas reordenadas (bloques de 8
  intercambiados); hay que permutar la paleta al subirla.
- Sin `double` por hardware: usar `f32` en todo el evaluador de expresiones.
- Sin `std` no hay `HashMap` ni `String` de regalo: los parsers conviene que trabajen
  sobre `&[u8]` / `&str` y llenen tablas de tamaño conocido.

---

## 7. Preguntas abiertas

1. **Qué KFM es el objetivo.** El de las primeras builds (hasta 2000.06.27) y el
   rehecho (2001.04.14 en adelante) no son el mismo personaje ni usan el mismo nivel de
   CNS. Los formatos binarios son idénticos; cambia cuánta lógica hay que implementar.
2. Confirmar contra archivos reales: orden invertido del `.act`, campos [M] de la
   cabecera SFF, formato de los WAV dentro del SND (bits, frecuencia, mono/estéreo).
3. Lista exacta de controladores y triggers que usa el KFM elegido (sale de un grep).

---

## 8. Lo que confirmó la implementación

Con los datos de MUGEN 1.1 beta 1 (los únicos que había en el disco):

- **SFF v1**: `fight.sff` y `fightfx.sff` se leen con la cabecera y la subcabecera
  descritas en 3.1 (lista enlazada, PCX con RLE, paleta en los últimos 768 bytes, bandera
  de "misma paleta"). Los campos marcados [M] de la cabecera (cantidad de grupos, tamaño
  de subcabecera, tipo de paleta) no hicieron falta.
- **FNT v1** y **SND**: tal como en 3.4 y 3.2. Los WAV son PCM de 8 bits, mono, a 8000,
  11025, 16000 y 22050 Hz.
- **El Kung Fu Man de MUGEN 1.1 usa SFF v2**, no v1: sprites de 5 bits (32 colores) con
  compresión LZ5, seis paletas en el propio archivo (grupo 1, números 1 a 6) y ningún
  `.act`. Sus CNS, CMD y AIR son los del Kung Fu Man clásico. El motor lee los dos
  formatos de SFF.
- **`.act`**: el lector existe (orden invertido, como en 3.3) pero no se pudo probar:
  no hay ningún personaje con paletas `.act` en el disco.
- **AIR, CMD y CNS**: la sintaxis de la sección 4 es correcta. Detalles que faltaban:
  - `AnimElem = n, k` (sin operador) equivale a `AnimElem = n, = k`.
  - `Time = command = "holdfwd"` es válido: compara `Time` con el resultado (0 o 1).
  - Los parámetros pueden ir vacíos (`reversal.attr =` desactiva el contraataque).
  - Los archivos pueden empezar con un BOM de UTF-8.
- **Lo que hace el motor y no los archivos.** `common1.cns` no contiene las transiciones
  básicas; las hace el motor cuando el jugador tiene el control: caminar (estado 20) y
  volver a 0, agacharse (10, 11, 12), saltar (40) y el salto en el aire (45), ponerse en
  guardia (120) si el rival ataca cerca y se mantiene atrás, girarse hacia el rival,
  aterrizar (52) y levantarse del suelo (de 5110 a 5120 pasado `liedown.time`).
- **Pausas.** Un `Pause` o `SuperPause` empieza a contar en el tick siguiente; el tick
  que lo lanza termina normalmente (si no, `AnimElem = 2` volvería a cumplirse al
  acabar la pausa y el especial no avanzaría nunca).
- **Estela por defecto.** `AfterImage` sin parámetros de paleta da la estela azulada
  opaca clásica (contraste 120, 120, 220).
- **FPU de la PS2.** Redondea hacia cero: `30 * 0.7` da 20 al pasar a entero. El poder
  que da un golpe se calcula en enteros (`daño * 7 / 10`) para que la pelea sea idéntica
  en el PC y en la consola.

Sigue sin confirmar la historia de versiones de la sección 1 (qué añadió cada versión de
DOS) y la pregunta 1 de la sección 7 quedó resuelta por los hechos: se usa el Kung Fu Man
de MUGEN 1.1 porque es el que hay.

---

## Fuentes

- Wikipedia — Mugen (game engine): https://en.wikipedia.org/wiki/Mugen_(game_engine)
- MUGEN FAQ de la era DOS (1999/2000): https://discmaster.textfiles.com/view/13007/HentaiMania%20CD%2003.iso/games/STREET%20GIRL/MUGENFAQ.TXT
- Ikemen GO, lector de SFF v1 (`src/image.go`): https://github.com/ikemen-engine/Ikemen-GO/blob/master/src/image.go
- Ikemen GO, lector de FNT v1 (`src/font.go`): https://github.com/ikemen-engine/Ikemen-GO/blob/master/src/font.go
- Crate `mugen-snd`, lector de SND: https://docs.rs/mugen-snd/0.0.1/src/mugen_snd/decoder.rs.html
- Elecbyte — SpriteMaker (SFF v1 vs v2): https://www.elecbyte.com/mugendocs/sprmake2.html
- Elecbyte — Fonts (FNT v1 vs v2): https://www.elecbyte.com/mugendocs/fonts.html
- Elecbyte — formato AIR: https://elecbyte.com/mugendocs-11b1/air.html
- Elecbyte — historial de versiones: https://www.elecbyte.com/mugendocs-11b1/history.html (no se pudo abrir: certificado inválido)
- The MUGEN Archive — build DOS 27/07/1999: https://mugenarchive.com/forums/downloads.php?do=file&id=58604-mugen-dos-version-27-07-1999-elecbyte
- Retronauts — MUGEN turns 20: https://retronauts.com/article/1240/infinite-fighting-love-mugen-turns-20
- Fandom MUGEN Database — Kung Fu Man: https://mugen.fandom.com/wiki/Kung_Fu_Man (no se pudo abrir; dato tomado del resumen del buscador)
