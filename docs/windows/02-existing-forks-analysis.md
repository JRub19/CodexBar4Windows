---
summary: "Side-by-side comparison of every known Windows fork/port of CodexBar."
read_when:
  - Deciding whether to fork an existing port or start clean
---

# 02 — Existing Windows forks analysis

Four projects came up. One is healthy. The other three are dead, stubbed, or scoped down to one provider.

## Summary table

| Project | Stack | Stars | Last release | Providers | Tray icon dynamic? | License | Status |
|---|---|---|---|---|---|---|---|
| [`Finesssee/Win-CodexBar`](https://github.com/Finesssee/Win-CodexBar) | **Tauri + React + Rust** (shared `rust/src` core, `apps/desktop-tauri` shell) | **392** | **v0.25.1 — May 11, 2026** | **40** | **Yes** (two-bar session + weekly meter) | MIT | **Active, near-parity** |
| [`babakarto/CodexBar-Win`](https://github.com/babakarto/CodexBar-Win) | Python 3.10 + customtkinter + PyInstaller | 67 | v1.1.2 — May 10, 2026 | **2** (Claude, Codex) | Static / per-tab brand swap | MIT | Active but tiny scope |
| [`nek0der/CodexBarWin`](https://github.com/nek0der/CodexBarWin) | C# / .NET 10 / WinUI 3 / Mica — **wraps an external `CodexBar` CLI**, **requires WSL** | 12 | v0.1.0 — Jan 24, 2026 | 3 (Claude, Codex, Gemini) | Not specified | MIT | Stalled; wrapper, not a port |
| [`rjdoesntcode/CodexBar-Win`](https://github.com/rjdoesntcode/CodexBar-Win) | C# / .NET 8 / WPF | 0 | none | 4 advertised | Not specified | MIT | Stub — 3 commits, no releases |
| [`ai-dev-2024/UsageBar`](https://github.com/ai-dev-2024/UsageBar) | Electron + TypeScript | 4 | v1.4.1 — Dec 2025 | 3 tested, others untested | Yes | MIT | "Inspired by" — not a fork, narrow scope |

## Detailed read

### `Finesssee/Win-CodexBar` — the only realistic base

Architecture from their `AGENTS.md` and Cargo files:

- `rust/src/` — shared core
  - `providers/` — 40 provider modules (mirrors the Swift `CodexBarCore/Providers/` layout)
  - `tray/` — `icon.rs` (pixel-level icon render, ~6KB) + `render.rs` (~4.6KB) + `mod.rs`
  - `browser/` — Chromium/Firefox cookie extraction with DPAPI decryption
  - `cli/` — `codexbar` CLI binary
  - `host/`, `core/`, `status/`, `sound/`, `shortcuts/`, `notifications/`
  - `secure_file.rs` — DPAPI-wrapped at-rest secret blobs
  - `settings.rs`, `login.rs`, `updater.rs`, `wsl.rs`, `cost_scanner.rs`
- `apps/desktop-tauri/src-tauri/` — Tauri shell hosting React popup
- Crates: `tray-icon` 0.19, `muda` 0.15, `winit` 0.30, `tiny-skia` 0.11, `resvg` 0.44, `usvg` 0.44, `image` 0.25, `eframe`/`egui` 0.30 (likely debug surface), `rusqlite` 0.32, `aes-gcm` 0.10, `keyring` 3, `windows` 0.58, `winreg` 0.55, `reqwest` 0.12, `tokio` 1, `clap` 4, `chrono` 0.4
- Build: `npm run tauri:build`, output `target/release/codexbar-desktop-tauri.exe`; CLI: `cargo build -p codexbar --release`. Installer via Inno Setup; portable EXE also published. WebView2 + VC++ runtime bootstrapper bundled.
- Windows-specific: DPAPI for at-rest secrets, `keyring` (Credential Manager) for named creds, Chromium `Local State` decryption with `aes-gcm`, WSLg path for the desktop shell when running under WSL.
- Notable: explicitly notes "Chromium DPAPI cookies cannot be decrypted from WSL." Manual cookie paste is the workaround. Same constraint applies to us.
- License: **MIT** — compatible with this fork.
- Activity: 2,034 commits; v0.25.1 in May 2026; 0 open issues at time of check.

**Why this matters:** Re-implementing 40 providers, Chromium DPAPI cookie decryption, dynamic tray rendering, Tauri popup, ConPTY runner, DPAPI secure-file layer, Inno installer pipeline, and WSL fallback **from scratch** is the bulk of the work. Win-CodexBar has done it. Reusing it (license-compatible) collapses the timeline from months to weeks.

**Risk:** It’s a downstream project. If it goes unmaintained, we own it. The Rust code is well-structured and not large — a one-person team can keep it moving. Upstream `steipete/CodexBar` is also a single-maintainer project, so this is not a worse risk profile than the status quo.

### `babakarto/CodexBar-Win` — Python, narrow

- 2 providers only. Auto-refresh 5 min. PyInstaller .exe, ~30 MB. Antivirus false-positive risk is real with PyInstaller. CLI PTY uses `pywinpty`. Known bug: shows 0% when another Claude session is running. Tray icon swaps brand asset per tab; not a true dynamic meter.
- **Not a viable base.** Python + customtkinter has no path to the rich popup UX, and the scope gap (2 vs 40 providers) is enormous. Reuse value: maybe the `pywinpty` integration pattern as a sanity check. Otherwise skip.

### `nek0der/CodexBarWin` — WinUI 3 wrapper, requires WSL

- It’s a **frontend wrapper** that shells out to an external `CodexBar CLI` running under WSL 2. So it inherits the Mac CLI’s behavior but only on machines that have set up WSL 2 + a Linux distro + the CLI.
- .NET 10 (still preview as of early 2026 for many users), Windows 10 1809+ minimum, WinUI 3 + Mica chrome.
- **Not a viable base.** WSL dependency is a non-starter for "works like Volume / Ethernet / Keyboard icons." The user requirement is a native tray experience, not "open WSL first." Reuse value: the WinUI 3 chrome work, if we ever went WinUI, would be a reference — but we are unlikely to choose WinUI (see [03](03-tech-stack-options.md)).

### `rjdoesntcode/CodexBar-Win` — empty stub

- 3 commits, 0 stars, 0 releases, README describes features but no shipped binary. .NET 8 + WPF.
- **Skip.**

### `ai-dev-2024/UsageBar` — Electron, separate project

- Not a CodexBar fork; "inspired by." 3 providers tested, 3 untested, Claude support marked Limited. Glassmorphism UI, hotkey support, electron-store, electron-builder, GitHub Actions release.
- **Skip as a base.** Useful as a sanity check that Electron *can* do this. We don’t choose Electron for reasons in [03](03-tech-stack-options.md).

## What we learn from the field

1. **Everyone who actually shipped settled on web-tech for the popup.** Win-CodexBar = Tauri + React. UsageBar = Electron. babakarto = Python widget toolkit but kept it small. The WinUI/.NET attempts stalled.
2. **Dynamic tray icon rendering is the differentiator.** Only Win-CodexBar and UsageBar do it. babakarto fakes it with brand swaps. The Mac app considers this a core feature; the Windows app must too.
3. **DPAPI + AES-GCM for Chromium cookies is well-understood and shipped.** Win-CodexBar’s implementation is the reference.
4. **WSL-only ports lose.** The user expectation is "Windows app like Volume icon," not "open WSL first."
5. **There is no community fork that *embeds* steipete’s Swift sources via swift-on-windows.** Nobody tried. This is a signal — see [03](03-tech-stack-options.md).
