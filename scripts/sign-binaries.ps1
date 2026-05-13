<#
.SYNOPSIS
  Authenticode-sign the release-built inner EXEs before Inno Setup
  wraps them.

.DESCRIPTION
  Stage one of Phase 9's two-stage signing pipeline. The installer
  itself is signed by Inno's `SignTool=` directive at the end of
  compile; this script handles the binaries that go *inside* the
  installer plus the portable ZIP.

  Authentication: the script expects a code-signing certificate
  already in the current user's certificate store (typically loaded
  by the release workflow from a PFX via `certutil -importpfx`). The
  signtool `/a` switch selects the best-match cert; pass
  `-Thumbprint` to lock to a specific SHA-1.

.PARAMETER Thumbprint
  Optional. SHA-1 thumbprint of the signing certificate; when set,
  signtool uses `/sha1` instead of `/a`. Read from the
  CODESIGN_THUMBPRINT environment variable when not passed.

.PARAMETER TargetDir
  Directory containing the unsigned binaries. Defaults to
  `target/release` relative to the repo root.

.PARAMETER TimestampUrl
  Authenticode timestamp server. Defaults to DigiCert's RFC 3161
  service.

.EXAMPLE
  pwsh scripts/sign-binaries.ps1
  pwsh scripts/sign-binaries.ps1 -Thumbprint "ABCDEF1234..."
#>
[CmdletBinding()]
param(
  [string] $Thumbprint   = $env:CODESIGN_THUMBPRINT,
  [string] $TargetDir    = (Join-Path (Split-Path -Parent $PSScriptRoot) "target\release"),
  [string] $TimestampUrl = "http://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

function Resolve-SignTool {
  # Newer Windows SDKs install signtool.exe under bin/<ver>/<arch>/.
  # The release runner has signtool on PATH, but local invocations
  # sometimes need the fallback.
  $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($onPath) { return $onPath.Source }

  $sdkRoots = @(
    "C:\Program Files (x86)\Windows Kits\10\bin",
    "C:\Program Files\Windows Kits\10\bin"
  )
  foreach ($root in $sdkRoots) {
    if (-not (Test-Path $root)) { continue }
    $candidate = Get-ChildItem -Path $root -Recurse -Filter "signtool.exe" -ErrorAction SilentlyContinue |
      Where-Object { $_.FullName -like "*\x64\signtool.exe" } |
      Sort-Object -Property FullName -Descending |
      Select-Object -First 1
    if ($candidate) { return $candidate.FullName }
  }
  throw "signtool.exe not found. Install the Windows SDK or add signtool to PATH."
}

$signTool = Resolve-SignTool
Write-Host "[sign-binaries] using $signTool"

$binaries = @(
  "CodexBar4Windows.exe",
  "codexbar4windows-claude-watchdog.exe"
)

$signed = @()
$skipped = @()
foreach ($name in $binaries) {
  $path = Join-Path $TargetDir $name
  if (-not (Test-Path $path)) {
    Write-Host "[sign-binaries] skip $name (not built)"
    $skipped += $name
    continue
  }
  Write-Host "[sign-binaries] signing $path"
  $args = @(
    "sign",
    "/tr", $TimestampUrl,
    "/td", "SHA256",
    "/fd", "SHA256",
    "/v"
  )
  if ($Thumbprint) {
    $args += @("/sha1", $Thumbprint)
  }
  else {
    $args += "/a"
  }
  $args += $path

  & $signTool @args
  if ($LASTEXITCODE -ne 0) {
    throw "signtool sign failed for $path (exit $LASTEXITCODE)"
  }
  $signed += $name
}

# Verify everything we just signed. /pa walks the PE certificate
# trust chain the same way SmartScreen does.
foreach ($name in $signed) {
  $path = Join-Path $TargetDir $name
  Write-Host "[sign-binaries] verifying $path"
  & $signTool verify /pa /v $path
  if ($LASTEXITCODE -ne 0) {
    throw "signtool verify failed for $path (exit $LASTEXITCODE)"
  }
}

Write-Host "[sign-binaries] signed: $($signed -join ', ')"
if ($skipped.Count -gt 0) {
  Write-Host "[sign-binaries] skipped (missing): $($skipped -join ', ')"
}
