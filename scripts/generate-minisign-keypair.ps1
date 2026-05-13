<#
.SYNOPSIS
  Generate a minisign keypair for the Tauri updater. Phase 9 polish.

.DESCRIPTION
  The Tauri updater plugin requires a minisign keypair: the private
  key signs `latest.json` manifests during release; the public key is
  baked into the app at build time (tauri.conf.json → plugins.updater
  .pubkey) so the running app can verify signatures.

  This script:
    1. Runs `minisign -G` to generate the keypair.
    2. Writes the private key to `<OutputDir>\codexbar-updater.key`.
    3. Reads the public key, base64-encodes the full line that
       tauri-plugin-updater expects, prints it to stdout.
    4. With `-Apply`: edits `apps/desktop-tauri/src-tauri/tauri.conf
       .json` in place, replacing the `REPLACE_WITH_BASE64_MINISIGN
       _PUBLIC_KEY` placeholder with the generated pubkey.

  The private key never lands in the repo — `.minisign/` is in
  .gitignore. Copy `codexbar-updater.key` into the
  `TAURI_MINISIGN_PRIVATE_KEY_PATH` secret on GitHub Actions and
  the matching password into `TAURI_MINISIGN_PASSWORD`.

.PARAMETER OutputDir
  Where to write the keypair. Defaults to `.minisign/` at the repo
  root (which is gitignored).

.PARAMETER Apply
  When set, also writes the generated pubkey into tauri.conf.json
  in place. Useful for first-time setup so the placeholder is
  cleared and the app boots with a real key.

.PARAMETER Force
  Overwrite an existing keypair on disk. Without this the script
  refuses to clobber.

.EXAMPLE
  pwsh scripts/generate-minisign-keypair.ps1 -Apply

  Prompts for a password, writes .minisign/codexbar-updater.key
  and .minisign/codexbar-updater.pub, patches tauri.conf.json with
  the new pubkey, and prints the pubkey to stdout.

.NOTES
  Requires `minisign` on PATH. Install via:
    winget install jedisct1.minisign
  Or download from https://github.com/jedisct1/minisign/releases.
#>

[CmdletBinding()]
param(
  [string]$OutputDir = (Join-Path (Resolve-Path "$PSScriptRoot\..") ".minisign"),
  [switch]$Apply,
  [switch]$Force
)

$ErrorActionPreference = "Stop"

function Find-Minisign {
  $cmd = Get-Command minisign -ErrorAction SilentlyContinue
  if ($null -eq $cmd) {
    throw "minisign not found on PATH. Install via 'winget install jedisct1.minisign' or download from https://github.com/jedisct1/minisign/releases."
  }
  return $cmd.Source
}

$minisign = Find-Minisign
Write-Host "Using minisign at: $minisign"

if (-not (Test-Path $OutputDir)) {
  New-Item -ItemType Directory -Path $OutputDir | Out-Null
}

$secretKey = Join-Path $OutputDir "codexbar-updater.key"
$publicKey = Join-Path $OutputDir "codexbar-updater.pub"

if ((Test-Path $secretKey) -and -not $Force) {
  throw "Refusing to overwrite existing key at $secretKey. Pass -Force to proceed."
}

# minisign -G prompts for a password interactively. Use -W if the user
# wants an unencrypted key (NOT recommended).
& $minisign -G -p $publicKey -s $secretKey
if ($LASTEXITCODE -ne 0) {
  throw "minisign -G exited with code $LASTEXITCODE"
}

if (-not (Test-Path $publicKey)) {
  throw "minisign did not write the public key to $publicKey"
}

# The public-key file is two lines:
#   untrusted comment: minisign public key XXXXXXXXXXXXXXXX
#   <base64 key blob>
# Tauri expects the base64 line, optionally prefixed with the
# `untrusted comment:` line; the v2 updater accepts either.
$lines = Get-Content $publicKey
$pubkeyLine = ($lines | Where-Object { $_ -notmatch '^untrusted comment' } | Select-Object -First 1).Trim()
if (-not $pubkeyLine) {
  throw "could not extract public-key blob from $publicKey"
}

Write-Host ""
Write-Host "==== minisign keypair generated ====" -ForegroundColor Green
Write-Host "Private key: $secretKey"
Write-Host "Public key:  $publicKey"
Write-Host ""
Write-Host "Base64 public key (paste into tauri.conf.json → plugins.updater.pubkey):"
Write-Host $pubkeyLine -ForegroundColor Cyan
Write-Host ""

if ($Apply) {
  $repoRoot = Resolve-Path "$PSScriptRoot\.."
  $configPath = Join-Path $repoRoot "apps\desktop-tauri\src-tauri\tauri.conf.json"
  if (-not (Test-Path $configPath)) {
    throw "tauri.conf.json not found at $configPath"
  }
  $raw = Get-Content $configPath -Raw
  if ($raw -notmatch '"REPLACE_WITH_BASE64_MINISIGN_PUBLIC_KEY"') {
    Write-Warning "Placeholder REPLACE_WITH_BASE64_MINISIGN_PUBLIC_KEY not present in $configPath; pubkey may already be set. Skipping in-place edit."
  }
  else {
    $patched = $raw -replace '"REPLACE_WITH_BASE64_MINISIGN_PUBLIC_KEY"', ('"{0}"' -f $pubkeyLine)
    Set-Content -Path $configPath -Value $patched -Encoding utf8 -NoNewline
    Write-Host "Patched $configPath in place." -ForegroundColor Green
  }
}

Write-Host ""
Write-Host "Next steps:" -ForegroundColor Yellow
Write-Host "  1. Commit tauri.conf.json (with the real pubkey)."
Write-Host "  2. Store $secretKey contents in the TAURI_MINISIGN_PRIVATE_KEY_PATH secret."
Write-Host "  3. Store the keypair password in TAURI_MINISIGN_PASSWORD."
Write-Host "  4. NEVER commit $OutputDir — confirm it's in .gitignore."
