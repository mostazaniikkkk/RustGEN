# Módulos del IOP

`libsd.irx` y `sdrdrv.irx` son los controladores de sonido de [PS2SDK](https://github.com/ps2dev/ps2sdk)
(versión v2.0.0 de ps2dev, sin modificar), con licencia Academic Free License 2.0
(`LICENSE-ps2sdk.txt`).

El chip de sonido de la PS2 (SPU2) cuelga del procesador de E/S, y la BIOS no trae un
servidor para manejarlo desde el procesador principal. Estos dos módulos lo son:
`libsd` habla con el SPU2 y `sdrdrv` expone sus funciones por RPC. Van dentro del
ejecutable (`include_bytes!` en `ps2/src/audio.rs`) y se cargan en el IOP desde memoria;
si no cargan, el juego funciona sin sonido.
