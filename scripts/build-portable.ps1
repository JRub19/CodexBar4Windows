<#
.SYNOPSIS
  Assemble the portable ZIP artifact from a release build.

.DESCRIPTION
  Phase 9 §E. The portable build is the same compiled binaries as the
  installer, but distributed as a ZIP with a `portable.marker` file
  that tells the runtime to store config alongside the EXE in a
  `portable-config\` directory instead of `%APPDATA%`.

  Inputs:
   * CODEXBAR_VERSION env var (or -Version param) — the marketing
     version string, used to name the ZIP.
   * `target/release/` populated with `cargo tauri build` output.

  Outputs:
   * `dist/CodexBar4Windows-<version>-portable-x64.zip`

.PARAMETER Version
  Marketing version, e.g. `1.0.1`. Defaults to the
  `CODEXBAR_VERSION` env var, then to `1.0.1`.

.PARAMETER TargetDir
  Path to `target/release`. Defaults relative to the repo root.

.PARAMETER DistDir
  Where the ZIP is written. Defaults to `dist/` at repo root.
#>
[CmdletBinding()]
param(
  [string] $Version   = $env:CODEXBAR_VERSION,
  [string] $TargetDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "target\release"),
  [string] $DistDir   = (Join-Path (Split-Path -Parent $PSScriptRoot) "dist")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

if (-not $Version) { $Version = "1.0.1" }

if (-not (Test-Path $TargetDir)) {
  throw "Release build not found at $TargetDir. Run ``cargo tauri build`` first."
}

$arch = "x64"
$stem = "CodexBar4Windows-$Version-portable-$arch"
$stage = Join-Path $DistDir $stem
$zipPath = Join-Path $DistDir "$stem.zip"

if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

$binaries = @(
  "CodexBar4Windows.exe",
  "codexbar4windows-claude-watchdog.exe"
)
foreach ($name in $binaries) {
  $src = Join-Path $TargetDir $name
  if (Test-Path $src) {
    Copy-Item $src -Destination $stage
  } else {
    Write-Host "[build-portable] skipping missing $name"
  }
}

# The WebView2 loader lives next to the shell binary; if Tauri emitted
# one, ship it so the portable build is usable on hosts that have only
# the WebView2 runtime installed system-wide.
$loader = Join-Path $TargetDir "WebView2Loader.dll"
if (Test-Path $loader) {
  Copy-Item $loader -Destination $stage
}

# Icon for the README + Windows shell.
$iconSrc = Join-Path (Split-Path -Parent $PSScriptRoot) "apps\desktop-tauri\src-tauri\icons\icon.ico"
if (Test-Path $iconSrc) {
  Copy-Item $iconSrc -Destination (Join-Path $stage "CodexBar4Windows.ico")
}

# Discriminator file: presence triggers portable mode at runtime so
# config writes go to the sibling `portable-config\` directory.
Set-Content -Path (Join-Path $stage "portable.marker") -Encoding ASCII -Value "1"

# Minimal README. Keeps the portable bundle self-explanatory without a
# round-trip to the website.
$readme = @"
CodexBar4Windows $Version (portable)
====================================

This is the portable distribution. Run CodexBar4Windows.exe in
place; configuration files live in a sibling ``portable-config\``
directory rather than %APPDATA%.

Requirements
------------
- Microsoft Edge WebView2 Runtime. Most Windows 10/11 hosts already
  ship it; install separately if launch fails with a WebView2 error.

Notes
-----
- Authenticode signed; verify with ``signtool verify /pa /v``.
- Source: https://github.com/JRub19/CodexBar4Windows
"@
Set-Content -Path (Join-Path $stage "README-PORTABLE.txt") -Encoding utf8 -Value $readme

Write-Host "[build-portable] zipping $stage -> $zipPath"
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zipPath -CompressionLevel Optimal

# Clean up the staging directory after a successful zip; the ZIP is
# the deliverable.
Remove-Item -Recurse -Force $stage
Write-Host "[build-portable] wrote $zipPath"
