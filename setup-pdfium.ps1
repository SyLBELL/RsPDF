# Mengunduh pdfium.dll (x64) dan menaruhnya di folder hasil build.
# Pemakaian:  .\setup-pdfium.ps1            (ke target\release)
#             .\setup-pdfium.ps1 -Dest .    (ke folder tertentu)
param([string]$Dest = "target\release")

$ErrorActionPreference = "Stop"
$url = "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-win-x64.tgz"
$tmp = Join-Path $env:TEMP "pdfium-win-x64"
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $tmp | Out-Null
$tgz = Join-Path $tmp "pdfium.tgz"

Write-Host "Mengunduh PDFium..."
Invoke-WebRequest -Uri $url -OutFile $tgz
tar -xzf $tgz -C $tmp

New-Item -ItemType Directory $Dest -Force | Out-Null
Copy-Item (Join-Path $tmp "bin\pdfium.dll") $Dest -Force
Write-Host "Selesai: $(Resolve-Path (Join-Path $Dest 'pdfium.dll'))"
