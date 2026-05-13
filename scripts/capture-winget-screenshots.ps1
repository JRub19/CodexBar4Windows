<#
.SYNOPSIS
  Skeleton for capturing the Winget submission screenshots.

.DESCRIPTION
  Winget reviewers like seeing real screenshots of the app's main
  surfaces. This script lays out the sequence the operator walks
  through manually: it opens each pane via a Tauri command and pauses
  so you can hit Win+Shift+S to capture.

  In a future iteration this will drive `tauri-driver` so the captures
  happen headlessly without manual snipping. For 1.0 it's a manual
  flow with prompts.

.PARAMETER OutputDir
  Where to put the .png screenshots. Defaults to `docs/screenshots/`.

.PARAMETER SkipBuild
  Skip the `npm run tauri dev` boot. Pass when the app is already
  running.

.EXAMPLE
  pwsh scripts/capture-winget-screenshots.ps1

.NOTES
  Requires:
    - Node + npm in PATH
    - The app installed or buildable from source
    - The Snipping Tool (Win+Shift+S) available
#>

[CmdletBinding()]
param(
  [string]$OutputDir = (Join-Path (Resolve-Path "$PSScriptRoot\..") "docs\screenshots"),
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $OutputDir)) {
  New-Item -ItemType Directory -Path $OutputDir | Out-Null
}

Write-Host "Output dir: $OutputDir"

if (-not $SkipBuild) {
  Write-Host ""
  Write-Host "Launching the dev build. Open a second terminal and run:" -ForegroundColor Yellow
  Write-Host "  cd apps\desktop-tauri && npm run tauri dev" -ForegroundColor Cyan
  Write-Host ""
  Write-Host "Wait until the tray icon appears, then press Enter here."
  Read-Host
}

$captures = @(
  @{ name = "01-popup-tray.png"; description = "Popup open from tray icon, showing cards for at least Claude + Codex with live usage" }
  @{ name = "02-popup-onboarding.png"; description = "Onboarding Step 2 (provider picker) with two providers checked" }
  @{ name = "03-settings-general.png"; description = "Preferences window, General pane, refresh-frequency picker open" }
  @{ name = "04-settings-providers.png"; description = "Preferences window, Providers pane, Claude row selected, sign-in widgets visible" }
  @{ name = "05-settings-cost.png"; description = "Preferences window, Cost & Storage pane, at least one provider's footprint rendered" }
  @{ name = "06-settings-shortcuts.png"; description = "Preferences window, Shortcuts pane, KeyShortcutRecorder in recording state" }
  @{ name = "07-toast-notification.png"; description = "Native Windows toast notification firing on a 50% threshold cross" }
)

foreach ($cap in $captures) {
  Write-Host ""
  Write-Host "===== $($cap.name) =====" -ForegroundColor Cyan
  Write-Host $cap.description
  Write-Host ""
  Write-Host "Set up the UI to match, then press Win+Shift+S to capture."
  Write-Host "Save the .png as: $OutputDir\$($cap.name)"
  Write-Host "Press Enter when done (or 'q' Enter to skip the rest)."
  $key = Read-Host
  if ($key -eq 'q') { break }
}

Write-Host ""
Write-Host "Done. Commit the screenshots and update the Winget manifest" -ForegroundColor Green
Write-Host "(packaging/winget/CodexBar4Windows.CodexBar4Windows.locale.en-US.yaml)"
Write-Host "with raw.githubusercontent.com URLs pointing at the new files."
