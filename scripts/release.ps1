$ErrorActionPreference = "Stop"

# Windows-only release script. For cross-platform builds use scripts/release.sh in WSL/Linux.
$target = "x86_64-pc-windows-msvc"
$outName = "onvm-windows.exe"

# Remap absolute paths out of debug info to avoid leaking local paths in the binary.
$repoPath = (Resolve-Path ".").Path
$remapFlag = "--remap-path-prefix=$repoPath=."
if ($env:RUSTFLAGS) {
    $env:RUSTFLAGS = "$($env:RUSTFLAGS) $remapFlag"
} else {
    $env:RUSTFLAGS = $remapFlag
}

Write-Host "Building Windows target ($target) with path remapping..."
cargo build --release --target $target

if (Test-Path "dist") {
    Remove-Item "dist" -Recurse -Force
}

New-Item -ItemType Directory "dist" | Out-Null
New-Item -ItemType Directory "dist\zipped" | Out-Null

$search = "target\$target\release\onvm*"
$bins = Get-ChildItem $search -File

if (-not $bins) {
    Write-Error "Binary not found for target $target"
    exit 1
}

foreach ($bin in $bins) {
    $isMain = ($bin.BaseName -ieq "onvm") -and (($bin.Extension -eq "") -or ($bin.Extension -ieq ".exe"))
    $destName = if ($isMain) { $outName } else { $bin.Name }
    Copy-Item $bin.FullName "dist\$destName" -Force
    Compress-Archive -Path "dist\$destName" -DestinationPath "dist\zipped\$destName.zip" -Force
}

Write-Host ""
Write-Host "Build finished. Windows executable is in dist\ and zipped version in dist\zipped\."
