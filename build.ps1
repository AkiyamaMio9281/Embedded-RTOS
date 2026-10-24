# Build (and optionally flash) the RTOS.
#   .\build.ps1          cargo build --release -> target\rtos.bin
#   .\build.ps1 -Flash   build, then copy rtos.bin to the Nucleo's USB drive (NOD_F446RE)
param([switch]$Flash)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "Build failed." }

# objcopy/size come from the Arm GNU Toolchain
$objcopy = Get-Command arm-none-eabi-objcopy -ErrorAction SilentlyContinue
if (-not $objcopy) {
    $bin = Get-ChildItem 'C:\Program Files (x86)\Arm GNU Toolchain arm-none-eabi\*\bin' -Directory -ErrorAction SilentlyContinue |
           Select-Object -Last 1
    if (-not $bin) { throw "arm-none-eabi-objcopy not found. Install the Arm GNU Toolchain." }
    $env:Path = "$($bin.FullName);$env:Path"
}

$elf = 'target/thumbv7em-none-eabihf/release/rtos'
& arm-none-eabi-objcopy -O binary $elf target/rtos.bin
& arm-none-eabi-size $elf

if ($Flash) {
    $vol = Get-Volume | Where-Object { $_.FileSystemLabel -like 'NOD_F446RE*' } | Select-Object -First 1
    if (-not $vol) { throw "NOD_F446RE drive not found. Is the Nucleo plugged in via USB?" }
    Copy-Item target/rtos.bin "$($vol.DriveLetter):\"
    Write-Host "Copied to $($vol.DriveLetter):\ - the ST-LINK will program and reset the board."
}
