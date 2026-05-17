<#
.SYNOPSIS
  Produce a sha256sum-format checksums file for the release
  artifacts. Phase 9 §E.

.DESCRIPTION
  After the Inno Setup installer and portable ZIP land in `dist/`,
  this script writes
  `dist/CodexBar4Windows-<version>-checksums.txt` with one line per
  artifact in `<sha256>  <basename>` form (two spaces, matching
  `sha256sum` convention so users can verify with the same tool).

.PARAMETER Version
  Marketing version. Defaults to CODEXBAR_VERSION env var.

.PARAMETER DistDir
  Directory holding the artifacts. Defaults to repo `dist/`.
#>
[CmdletBinding()]
param(
  [string] $Version = $env:CODEXBAR_VERSION,
  [string] $DistDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "dist")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

if (-not $Version) { $Version = "1.0.1" }
if (-not (Test-Path $DistDir)) {
  throw "Dist directory not found at $DistDir."
}

$arch = "x64"
$artifacts = @(
  "CodexBar4Windows-$Version-$arch.exe",
  "CodexBar4Windows-$Version-updater-$arch.nsis.zip",
  "CodexBar4Windows-$Version-portable-$arch.zip"
)

$lines = @()
$missing = @()
foreach ($name in $artifacts) {
  $path = Join-Path $DistDir $name
  if (-not (Test-Path $path)) {
    $missing += $name
    continue
  }
  $hash = (Get-FileHash -Algorithm SHA256 -Path $path).Hash.ToLower()
  $lines += "${hash}  ${name}"
}

if ($missing.Count -gt 0 -and $lines.Count -eq 0) {
  throw "No release artifacts found in $DistDir. Looked for: $($artifacts -join ', ')"
}
if ($missing.Count -gt 0) {
  foreach ($name in $missing) {
    Write-Host "[generate-checksums] skipping missing artifact: $name"
  }
}

$outPath = Join-Path $DistDir "CodexBar4Windows-$Version-checksums.txt"
Set-Content -Path $outPath -Encoding ascii -Value $lines
Write-Host "[generate-checksums] wrote $outPath ($($lines.Count) entries)"
foreach ($line in $lines) {
  Write-Host "    $line"
}
