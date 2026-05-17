# CodexBar4Windows

> Every AI coding limit, in your Windows tray.

[![CI](https://github.com/JRub19/CodexBar4Windows/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/JRub19/CodexBar4Windows/actions/workflows/ci.yml)
[![Release](https://github.com/JRub19/CodexBar4Windows/actions/workflows/release.yml/badge.svg)](https://github.com/JRub19/CodexBar4Windows/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-6e5aff?style=flat-square)](LICENSE)

CodexBar4Windows is the Windows-native port of [`steipete/CodexBar`](https://github.com/steipete/CodexBar), built on Tauri 2 + React + a shared Rust core. It lives in the system tray and keeps your AI coding quota visible at a glance across **eleven providers**: Claude, Codex, Cursor, Copilot, Gemini, OpenRouter, Factory, DeepSeek, Moonshot, Z.ai, and Venice.

## Features

- **Live tray icon** that morphs between a primary bar (session quota) and a secondary bar (weekly quota) per provider, and aggregates across providers when several are active.
- **Mica-effect popup** opens from the tray. One card per provider, each with primary/secondary breakdowns, plan utilization, credits remaining, and pace.
- **Cost scanning** of local JSONL session logs for Claude Code, Codex CLI, and pi: daily/monthly totals, model breakdown, and fork inheritance subtraction.
- **Storage footprint** report under Preferences -> Cost & Storage with one-click "Open folder" navigation.
- **Smart toasts** at 50% / 25% / 10% session-quota remaining, with per-provider thresholds.
- **Status overlay** chips for incidents from Statuspage.io and Google Workspace feeds.
- **Global hotkey** (Win+Shift+U by default, rebindable via the Shortcuts pane).
- **Onboarding wizard** for fresh installs: welcome -> provider picker -> per-provider sign-in -> done.
- **i18n** for English, Simplified Chinese (`zh-Hans`), and Brazilian Portuguese (`pt-BR`), live-applied from the Appearance pane.
- **Auto-update** via Tauri's signed manifest pipeline. Stable releases require a real minisign updater key before publication. Authenticode signing is optional; unsigned builds may show Windows SmartScreen warnings.
- **Launch-at-sign-in** via the HKCU Run registry key.
- **No telemetry** by default. Crash reports are opt-in.

## Install

### Winget

```powershell
winget install CodexBar4Windows.CodexBar4Windows
```

### Installer

Download `CodexBar4Windows-<version>-x64.exe` from the [Releases page](https://github.com/JRub19/CodexBar4Windows/releases). It installs per-user at `%LOCALAPPDATA%\Programs\CodexBar4Windows`.

### Portable

`CodexBar4Windows-<version>-portable-x64.zip` is a no-install build. Unzip anywhere; the marker file `portable.marker` makes the app read/write config next to the EXE instead of `%APPDATA%`.

### Beta Channel

See [`BETA.md`](BETA.md).

## Requirements

- Windows 10 build 17763 (1809) or newer; Windows 11 recommended.
- WebView2 Evergreen runtime. Windows 11 normally has it; the installer bootstraps it on Windows 10 when missing.
- About 80 MB RAM steady-state and about 22 MB disk.

## Build From Source

```powershell
# Prerequisites:
#   - Rust stable (rustup install stable)
#   - Node 22 or newer
#   - MSVC Build Tools 2019/2022 with the C++ desktop workload

git clone https://github.com/JRub19/CodexBar4Windows.git
cd CodexBar4Windows\apps\desktop-tauri
npm install
npm run tauri dev
```

Release build:

```powershell
cd apps\desktop-tauri
npm run tauri build
```

The release EXE lands at `target\release\CodexBar4Windows.exe`.

## Current Scope

The v1.0.1 line ships the 11 providers listed above. The macOS upstream and Finesssee Windows fork support a broader provider catalog; parity work is tracked separately in [`docs/PROVIDER_PARITY.md`](docs/PROVIDER_PARITY.md).

There is no shipped `codexbar.exe` CLI peer in v1.0.1. The Rust core is shared with the desktop app, but the installer and portable ZIP currently ship the desktop app plus helper binaries only. The CLI remains post-1.0 work.

Codex usage should prefer OAuth credentials from `~\.codex\auth.json` or the local Codex CLI TUI path. ChatGPT/OpenAI web-cookie scraping is best-effort and can fail behind Cloudflare or Chromium cookie-encryption changes.

## Documentation

| Surface | Doc |
|---|---|
| Architecture overview | [`docs/windows/04-recommended-architecture.md`](docs/windows/04-recommended-architecture.md) |
| 10-phase execution plan | [`docs/windows/plan/00-master-plan.md`](docs/windows/plan/00-master-plan.md) |
| Subsystem specs (14 docs) | [`docs/windows/spec/`](docs/windows/spec/) |
| Provider parity status | [`docs/PROVIDER_PARITY.md`](docs/PROVIDER_PARITY.md) |
| Windows release risks | [`docs/WINDOWS_RELEASE_RISKS.md`](docs/WINDOWS_RELEASE_RISKS.md) |
| Performance budgets | [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) |
| Release runbook | [`docs/RELEASE.md`](docs/RELEASE.md) |
| Beta channel | [`BETA.md`](BETA.md) |
| Support + escalation | [`SUPPORT.md`](SUPPORT.md) |
| Contributor guide | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Security policy | [`SECURITY.md`](SECURITY.md) |

## Acknowledgements

- [`steipete/CodexBar`](https://github.com/steipete/CodexBar): the original macOS project. MIT. Every behavior in CodexBar4Windows is sourced from a deep read of the Swift code, then re-implemented for Windows-native semantics.
- [`Finesssee/Win-CodexBar`](https://github.com/Finesssee/Win-CodexBar): a community Windows port that proved the Tauri-plus-Rust shape. We do not import their source; the shape of the stack is theirs.

## License

MIT. See [LICENSE](LICENSE).
