---
summary: "Windows-specific release risks and mitigations for CodexBar4Windows."
read_when:
  - Preparing a release
  - Writing release notes
  - Triage of install, updater, cookie, or tray-position bugs
---

# Windows Release Risks

This document tracks known Windows-specific risks that remain after v1.0.1.
They are not release blockers when the listed mitigation is present and tested.

## Chrome v20 App-Bound Encryption

New Chromium builds can store cookies with App-Bound Encryption (`v20`) that
plain DPAPI decryption cannot read from a normal desktop process.

Mitigation:
- Keep manual cookie paste available for cookie-based providers.
- Prefer OAuth/API/CLI sources where available.
- Document provider-specific cookie limits in provider docs and issue templates.

Release note wording:
> Some Chrome/Edge/Brave cookie imports can require manual cookie paste on newer
> browser versions because of Chromium App-Bound Encryption.

## SmartScreen and Authenticode Reputation

Unsigned or newly signed Windows installers can trigger Microsoft SmartScreen,
especially because the app reads browser cookie stores and launches CLI helpers.

Mitigation:
- Prefer Authenticode signing secrets in GitHub Actions when available.
- Unsigned releases must be labeled as unsigned in release notes and may show
  SmartScreen warnings.
- When signing is available, verify the desktop EXE, helper EXEs, and installer
  with `signtool verify /pa /v`.
- Keep one signing certificate stable across releases to build reputation.
- Submit released installers to Microsoft Security Intelligence if Defender or
  SmartScreen reputation lags.

## WebView2 Availability

The Tauri desktop shell requires Microsoft Edge WebView2. Windows 11 usually
has it installed; some Windows 10 or locked-down corporate hosts do not.

Mitigation:
- The installer bootstraps the Evergreen WebView2 runtime when the registry
  probe does not find it.
- Portable ZIP users must install WebView2 separately if launch fails with a
  WebView2 runtime error.
- Consider a fixed-runtime installer variant only if beta/stable users report
  blocked Evergreen installation.

## Mixed-DPI Tray Positioning

The tray popup is positioned from tray-click coordinates and monitor bounds.
Mixed-DPI and multi-monitor taskbar layouts remain hard to fully cover in CI.

Mitigation:
- The app manifest opts into PerMonitorV2 DPI awareness.
- Popup positioning clamps to current monitor bounds.
- Manual smoke testing should include at least one mixed-DPI setup before a
  major UI release.

## Updater Key Configuration

The updater is intentionally disabled when `tauri.conf.json` still contains the
placeholder minisign public key.

Mitigation:
- Stable release workflow fails if the placeholder pubkey is present.
- Run `scripts/generate-minisign-keypair.ps1 -Apply`, commit the public key,
  and store the private key/password as GitHub Actions secrets before tagging.
