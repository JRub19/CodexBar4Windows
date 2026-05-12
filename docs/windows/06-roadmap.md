---
summary: "Phased delivery plan for the Windows refactor."
read_when:
  - Planning sprints / milestones
  - Tracking parity against upstream
---

# 06 — Roadmap

Assumes [Path 1 from doc 04](04-recommended-architecture.md): import Win-CodexBar’s tree as a baseline, then iterate. Time estimates are calendar weeks for one engineer working ~50% on this, doubled if unfamiliar with Rust/Tauri.

## Phase 0 — Repo reset (week 0–1)

- [ ] Decide on Path 1 (rebase) vs Path 2 (clean rebuild). See [04](04-recommended-architecture.md).
- [ ] Sign the CLA/attribution decision: keep Win-CodexBar’s MIT LICENSE + add upstream `steipete/CodexBar` attribution to README.
- [ ] If Path 1: import `Finesssee/Win-CodexBar` files; remove Swift sources; tag a `v0-import` baseline.
- [ ] Stand up CI on GitHub Actions for Windows: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm run tauri build`.
- [ ] Set up branch protection on `main`.
- [ ] First green build → tag `v0.1.0-pre`.

**Exit criteria:** `./dev.ps1` on a clean Windows 11 machine produces a runnable tray app.

## Phase 1 — Re-grounding (week 1–3)

Goal: own the code we’ve imported.

- [ ] Read every file in `rust/src/` once; comment any code we don’t understand.
- [ ] Rename app identifier, AUMID, registry keys, install paths to a stable form for this fork.
- [ ] Strip anything we won’t ship at v1 (debug-only egui windows if present, dead provider stubs).
- [ ] Replace the icon assets with ours (export from `Icon.icon` to 16/20/24/32/40/48/64/256 PNG → bundle into ICO).
- [ ] Verify Mica + Acrylic styling matches the spec in [05](05-windows-ux-spec.md).
- [ ] Add a `docs/windows/` link from the main README; rewrite the README for Windows users (drop Homebrew, add Winget + Inno installer + portable).

**Exit criteria:** the app builds, looks like ours, and the team can navigate the code without re-reading docs each time.

## Phase 2 — Parity audit (week 3–5)

Goal: know exactly where we are vs `steipete/CodexBar` `main`.

- [ ] For each provider in upstream `Sources/CodexBarCore/Providers/`, find the corresponding `rust/src/providers/<name>/` module.
  - Build the matrix in [02-existing-forks-analysis.md](02-existing-forks-analysis.md) at the provider × feature level (cookies/OAuth/CLI/API key).
  - File issues for every gap.
- [ ] Cross-check recent upstream commits — last 90 days — against the Win-CodexBar fork. Cherry-pick or re-implement.
- [ ] Verify each provider’s reset windows match upstream definitions (session/weekly/monthly).
- [ ] Verify the cost-scan results for Claude and Codex match upstream on the same input JSONL.
- [ ] Verify icon rendering matches upstream pixel-for-pixel on equivalent state.

**Exit criteria:** a tracked parity board with one row per provider × feature, all green or labeled with a target milestone.

## Phase 3 — Windows polish (week 5–7)

Goal: this should feel like a Windows app, not a Mac app in a costume.

- [ ] First-run onboarding flow including the "pin to taskbar" toast.
- [ ] Toast notifications (`Windows.UI.Notifications`) wired through `tauri-plugin-notification`.
- [ ] Global hotkey (default `Win+Shift+U`); rebinder in Preferences.
- [ ] Launch-at-sign-in via `HKCU\...\Run` or MSIX manifest.
- [ ] Popup positioning relative to the tray rect — handle bottom/left/right/top taskbars.
- [ ] Light/dark taskbar theming with live `WM_SETTINGCHANGE` redraw.
- [ ] Accent-color pickup (`UISettings`).
- [ ] Per-monitor DPI awareness (`SetProcessDpiAwarenessContext(PerMonitorAwareV2)`).
- [ ] Right-click `muda` menu finalized.

**Exit criteria:** Windows-feel checklist in [05](05-windows-ux-spec.md) §1–6 fully ticked.

## Phase 4 — Packaging & distribution (week 7–8)

- [ ] Inno Setup or NSIS installer; bootstrap WebView2 runtime if missing.
- [ ] Portable EXE build.
- [ ] **Code signing**: source an EV (or at least OV) code-signing cert. Without it, SmartScreen warns on first run. Budget ~$200/yr for OV, ~$300+/yr for EV.
- [ ] Sign installer and the EXE inside.
- [ ] Tauri updater manifest published to GitHub Releases.
- [ ] Auto-update endpoint signed with the Tauri key.
- [ ] Optional: Winget manifest PR to `microsoft/winget-pkgs`.
- [ ] Optional: MSIX package + Microsoft Store listing.

**Exit criteria:** `v0.1.0` shipping installer + portable, with working auto-update.

## Phase 5 — Beta (week 8–10)

- [ ] Invite-only release on the project README / a discord / a small testers list.
- [ ] Telemetry: opt-in error reporting only. No usage analytics.
- [ ] Public bug-report template; a `windows-only` issue label.
- [ ] Triage incoming reports; fix at least every issue marked `crash` or `data-loss`.
- [ ] Document the AV / SmartScreen story.

**Exit criteria:** at least one full release cycle without a crash report from beta users.

## Phase 6 — GA (week 10–12)

- [ ] Public README touched up with screenshots and a 60-second demo GIF.
- [ ] Winget release.
- [ ] Submit the Microsoft Store listing if MSIX is on the table.
- [ ] PR upstream `steipete/CodexBar` README to mention this fork alongside `Finesssee/Win-CodexBar`.

## Ongoing — Parity loop

The work doesn’t end at GA. Plan to:

- Re-run the **parity audit** monthly: diff `steipete/CodexBar:main` against the last point we synced.
- Cherry-pick / re-implement new providers and bug fixes from upstream.
- Keep WebView2, Tauri, and the Rust toolchain current — Windows runtime regressions are the single biggest risk.

## Out-of-scope (explicit)

- Linux build — Tauri makes it possible, but the cookie-decryption story (libsecret variants per distro) and tray (XEmbed/StatusNotifier flux) are their own swamp. Not at v1.
- Windows widgets board integration.
- ARM64 Windows build — possible (Tauri supports it) but defer until there’s demand.
- Mobile.
