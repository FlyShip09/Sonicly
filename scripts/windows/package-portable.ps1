# Stages dist/Sonicly-<version>-windows-x64-portable.zip from an already built release exe:
# the exe, the licence, the notices and a short README. Prints the zip's SHA-256.
#   cargo build --release --locked -p sonicly
#   powershell -File scripts/windows/package-portable.ps1
param(
    [string]$Version = "",
    [string]$Executable = "target/release/sonicly.exe",
    [string]$Output = "dist"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "../..")).Path
$exe = (Resolve-Path (Join-Path $root $Executable)).Path
if (-not $Version) {
    $Version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
}
$Version = $Version.TrimStart("v")

$dist = Join-Path $root $Output
$name = "Sonicly-$Version-windows-x64-portable"
$stage = Join-Path $dist $name
$zip = Join-Path $dist "$name.zip"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
if (Test-Path $zip) { Remove-Item -Force $zip }
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item $exe (Join-Path $stage "sonicly.exe")
foreach ($file in "LICENSE", "NOTICE.md", "THIRD-PARTY.md") {
    Copy-Item (Join-Path $root $file) $stage
}
@(
    "Sonicly $Version, a music player for local files and streaming services. Run sonicly.exe; nothing is installed."
    "Settings are kept in %APPDATA%\sonicly, and the library, sign-ins and cache in %LOCALAPPDATA%\sonicly."
    "Source code and licence: https://github.com/FlyShip09/Sonicly (GPL-3.0-or-later, see LICENSE, NOTICE.md and THIRD-PARTY.md)."
) | Set-Content -Encoding ascii (Join-Path $stage "README.txt")

Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal
Remove-Item -Recurse -Force $stage

$hash = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
$size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
Write-Output "$hash  $name.zip"
Write-Output "portable: $zip, $size MB"
