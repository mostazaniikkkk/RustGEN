# Genera los ejecutables finales en dist\:
#   mugen-ps2.elf   para lanzar con Free McBoot / uLaunchELF (USB o tarjeta).
#                   Lleva dentro los datos de game\.
#   mugen-ps2.iso   imagen de disco. Los datos van aparte, en GAME.PAK, y se
#                   leen del disco al cargar cada pantalla o pelea.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
# Sin configuración de prueba dentro del paquete.
$overlay = Join-Path $PSScriptRoot 'target\ps2test'
if (Test-Path $overlay) { Remove-Item -Recurse -Force $overlay -Confirm:$false }
New-Item -ItemType Directory -Force $overlay | Out-Null
New-Item -ItemType Directory -Force (Join-Path $PSScriptRoot 'dist') | Out-Null
cargo build --release -p mugen-host
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo ps2 build -p mugen-ps2 -- --features disc
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
& target\release\mugen-host.exe iso target\ps2\mugen-ps2.elf dist\mugen-ps2.iso target\game.pak
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo ps2 build -p mugen-ps2
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Copy-Item target\ps2\mugen-ps2.elf dist\mugen-ps2.elf -Force
Get-ChildItem dist | Format-Table Name, Length
