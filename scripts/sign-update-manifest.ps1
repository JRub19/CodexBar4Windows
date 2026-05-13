<#
.SYNOPSIS
  Produce + sign the Tauri updater manifest. Phase 9 §F.

.DESCRIPTION
  After the Inno Setup installer is built and Authenticode-signed,
  this script wraps it in the JSON manifest format the
  tauri-plugin-updater expects, then signs the installer bytes with
  minisign and embeds the base64 signature in the manifest.

  Two output files:
   * dist/latest.json — Stable channel manifest.
   * dist/beta.json   — Beta channel manifest, pointed at the same
                        artifact when the version tag carries a
                        -beta or -rc suffix.

  The Tauri runtime fetches `latest.json` (or `beta.json` when the
  user chose Beta) on launch + on demand, verifies the embedded
  base64 minisign signature against the public key baked into
  tauri.conf.json, and refuses to install a tampered installer.

.PARAMETER Version
  Marketing version. Required (env CODEXBAR_VERSION when unset).

.PARAMETER InstallerPath
  Path to the signed installer .exe. Required.

.PARAMETER Channel
  Either "stable" or "beta". Derived from -Version suffix when
  omitted: -beta / -rc → beta, else stable.

.PARAMETER MinisignKey
  Path to the minisign private key. Defaults to
  $env:TAURI_MINISIGN_PRIVATE_KEY_PATH.

.PARAMETER MinisignPassword
  Optional password for the minisign key. Defaults to
  $env:TAURI_MINISIGN_PASSWORD.

.PARAMETER NotesPath
  Optional Markdown release notes for the `notes` field.

.PARAMETER DistDir
  Where to write the manifests. Defaults to repo `dist/`.
#>
[CmdletBinding()]
param(
  [string] $Version           = $env:CODEXBAR_VERSION,
  [Parameter(Mandatory = $false)]
  [string] $InstallerPath,
  [ValidateSet("stable", "beta")]
  [string] $Channel,
  [string] $MinisignKey       = $env:TAURI_MINISIGN_PRIVATE_KEY_PATH,
  [string] $MinisignPassword  = $env:TAURI_MINISIGN_PASSWORD,
  [string] $NotesPath,
  [string] $DistDir           = (Join-Path (Split-Path -Parent $PSScriptRoot) "dist")
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 3.0

if (-not $Version) { throw "Version is required (set CODEXBAR_VERSION env var)." }
if (-not $InstallerPath) {
  $InstallerPath = Join-Path $DistDir "CodexBar4Windows-$Version-x64.exe"
}
if (-not (Test-Path $InstallerPath)) {
  throw "Installer not found at $InstallerPath."
}
if (-not (Test-Path $DistDir)) {
  New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
}
if (-not $Channel) {
  $Channel = if ($Version -match "-(beta|rc)") { "beta" } else { "stable" }
}
if (-not $MinisignKey) {
  throw "Missing minisign private key. Set TAURI_MINISIGN_PRIVATE_KEY_PATH or pass -MinisignKey."
}
if (-not (Test-Path $MinisignKey)) {
  throw "Minisign key not found at $MinisignKey."
}

$minisign = (Get-Command minisign.exe -ErrorAction SilentlyContinue) ?? (Get-Command minisign -ErrorAction SilentlyContinue)
if (-not $minisign) {
  throw "minisign not found on PATH. Install with ``choco install minisign``."
}

$sigPath = "$InstallerPath.sig"
if (Test-Path $sigPath) { Remove-Item -Force $sigPath }

# minisign supports password-on-stdin via `-W` ("non-interactive password from environment");
# fall back to interactive prompt when the env var is unset.
$argv = @(
  "-S",
  "-s", $MinisignKey,
  "-m", $InstallerPath,
  "-x", $sigPath,
  "-t", "CodexBar4Windows $Version"
)
if ($MinisignPassword) {
  # When a password is configured, minisign reads it from stdin.
  $MinisignPassword | & $minisign.Source @argv
} else {
  & $minisign.Source @argv
}
if ($LASTEXITCODE -ne 0) {
  throw "minisign signing failed (exit $LASTEXITCODE)"
}

# The signature file contains an untrusted-comment header line plus
# the base64 signature on its own line. Tauri's manifest format
# wants the entire signature file content base64-encoded.
$sigBytes = [System.IO.File]::ReadAllBytes($sigPath)
$signatureB64 = [Convert]::ToBase64String($sigBytes)

$pubDate = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")

$notes = if ($NotesPath -and (Test-Path $NotesPath)) {
  Get-Content -Raw -Path $NotesPath
} else {
  "CodexBar4Windows $Version"
}

$baseUrl = "https://github.com/JRub19/CodexBar4Windows/releases/download/v$Version"
$installerUrl = "$baseUrl/CodexBar4Windows-$Version-x64.exe"

$manifest = [ordered]@{
  version   = $Version
  notes     = $notes
  pub_date  = $pubDate
  platforms = [ordered]@{
    "windows-x86_64" = [ordered]@{
      signature = $signatureB64
      url       = $installerUrl
    }
  }
}

$json = $manifest | ConvertTo-Json -Depth 6 -Compress:$false

$primary = if ($Channel -eq "beta") { "beta.json" } else { "latest.json" }
$primaryPath = Join-Path $DistDir $primary
Set-Content -Path $primaryPath -Encoding utf8 -Value $json
Write-Host "[sign-update-manifest] wrote $primaryPath"

# Beta channel always sees stable releases too — write a beta.json
# pointing at the same artifact when the channel is stable so
# beta-channel users do not regress to older builds.
if ($Channel -eq "stable") {
  $betaPath = Join-Path $DistDir "beta.json"
  Set-Content -Path $betaPath -Encoding utf8 -Value $json
  Write-Host "[sign-update-manifest] wrote $betaPath (mirror of stable)"
}
