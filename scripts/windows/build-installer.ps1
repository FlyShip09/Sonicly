# Builds dist/Sonicly-<version>-windows-x64-setup.exe from an already built release exe.
# Needs Inno Setup 6, which this script never installs. The version defaults to the workspace's.
param(
    [string]$Version = "",
    [string]$Executable = "target/release/sonicly.exe",
    [string]$Output = "dist"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "../..")
$source = Resolve-Path (Join-Path $root $Executable)
$outputPath = Join-Path $root $Output
New-Item -ItemType Directory -Force $outputPath | Out-Null

$iscc = @(
    (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6/ISCC.exe")
    (Join-Path $env:LOCALAPPDATA "Programs/Inno Setup 6/ISCC.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    throw "Inno Setup 6 is not installed"
}

if (-not $Version) {
    $Version = (Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
}
$env:SONICLY_VERSION = $Version.TrimStart("v")
$env:SONICLY_EXE = $source.Path
$env:SONICLY_DIST = (Resolve-Path $outputPath).Path
# CI sets these two for each architecture it builds. A local run packages the x64 build.
if (-not $env:SONICLY_ARCH) { $env:SONICLY_ARCH = "x64compatible" }
if (-not $env:SONICLY_SETUP) { $env:SONICLY_SETUP = "Sonicly-$($env:SONICLY_VERSION)-windows-x64-setup" }

& $iscc (Join-Path $PSScriptRoot "sonicly.iss")
if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup failed with exit code $LASTEXITCODE"
}
