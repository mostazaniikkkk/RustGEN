# Compila el juego para PS2 (con los datos de game/ dentro del ELF) y lo
# ejecuta en PCSX2.
#
#   .\run-ps2.ps1                               jugar
#   .\run-ps2.ps1 test tests\ai300.cfg [...]    prueba automática
#
# Los argumentos restantes se pasan a `cargo ps2 run` (por ejemplo
# --screenshot out\ps2.png). La ruta de PCSX2 se toma de la variable de entorno
# PCSX2_EXE.
param([string]$Mode = '', [string]$Config = '', [Parameter(ValueFromRemainingArguments = $true)]$Rest)

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
# Lo que haya en target\ps2test se empaqueta junto a game\.
$overlay = Join-Path $PSScriptRoot 'target\ps2test'
if (Test-Path $overlay) { Remove-Item -Recurse -Force $overlay -Confirm:$false }
New-Item -ItemType Directory -Force $overlay | Out-Null
if ($Mode -eq 'test') {
    Copy-Item $Config "$overlay\ps2test.cfg"
    Copy-Item tests\*.keys $overlay
    # Los personajes de prueba, junto a los del juego.
    Copy-Item -Recurse tests\chars $overlay
    cargo ps2 run -p mugen-ps2 --until 'prueba terminada' @Rest
} else {
    cargo ps2 run -p mugen-ps2 --keep-open --timeout 100000 @Rest
}
exit $LASTEXITCODE
