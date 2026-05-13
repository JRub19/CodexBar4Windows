# CodexBar4Windows

> Every AI coding limit, in your Windows tray.

[![CI](https://github.com/JRub19/CodexBar4Windows/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/JRub19/CodexBar4Windows/actions/workflows/ci.yml)
[![Release](https://github.com/JRub19/CodexBar4Windows/actions/workflows/release.yml/badge.svg)](https://github.com/JRub19/CodexBar4Windows/actions/workflows/release.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-6e5aff?style=flat-square)](LICENSE)

CodexBar4Windows is the Windows-native port of [`steipete/CodexBar`](https://github.com/steipete/CodexBar), built on Tauri 2 + React + a shared Rust core. It lives in the system tray and keeps your AI coding quota visible at a glance — across **eleven providers**: Claude, Codex, Cursor, Copilot, Gemini, OpenRouter, Factory, DeepSeek, Moonshot, Z.ai, and Venice.

## Features

- **Live tray icon** that morphs between a primary bar (session quota) and a secondary bar (weekly quota) per provider — and aggregates across providers when several are active.
- **Mica-effect popup** opens from the tray. One card per provider, each with the same primary/secondary breakdown, plan utilization, credits remaining, and pace.
- **Cost scanning** of local JSONL session logs for Claude Code, Codex CLI, and pi — daily / monthly totals, model breakdown, fork inheritance subtraction.
- **Storage footprint** report under Preferences → Cost & Storage: shows how much disk each provider uses with one-click "Open folder" navigation.
- **Smart toasts** at 50% / 25% / 10% session-quota remaining, with per-provider thresholds.
- **Status overlay** chips for incidents — pulls Statuspage.io feeds for Anthropic, OpenAI, GitHub, Google Workspace.
- **Global hotkey** (Win+Shift+U by default, rebindable via the KeyShortcutRecorder in the Shortcuts pane).
- **Onboarding wizard** for fresh installs: welcome → provider picker → per-provider sign-in → done.
- **i18n** for English, Simplified Chinese (`zh-Hans`), Brazilian Portuguese (`pt-BR`), live-applied from the Appearance pane.
- **Auto-update** via Tauri's signed manifest pipeline. Stable and Beta channels.
- **Launch-at-sign-in** via the HKCU Run registry key (no scheduled-task spaghetti).
- **No telemetry** by default. Crash reports are opt-in.

## Install

### Winget (recommended once GA)

```powershell
winget install CodexBar4Windows.CodexBar4Windows
```

### Inno Setup installer

Download `CodexBar4Windows-<version>-x64.exe` from the [Releases page](https://github.com/JRub19/CodexBar4Windows/releases). Per-user install at `%LOCALAPPDATA%\Programs\CodexBar4Windows`. Authenticode-signed.

### Portable

`CodexBar4Windows-<version>-portable-x64.zip` is a no-install build. Unzip anywhere; the marker file `portable.marker` makes the app read/write config next to the EXE instead of `%APPDATA%`.

### Beta channel

See [`BETA.md`](BETA.md).

## Requirements

- Windows 10 build 17763 (1809) or newer; Windows 11 recommended.
- WebView2 Evergreen runtime (preinstalled on Windows 11; the installer bootstraps it on Windows 10).
- ~80 MB RAM steady-state. ~22 MB disk.

## Build from source

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

The release EXE lands at `target\release\codexbar4windows-desktop.exe`.

## Documentation

| Surface | Doc |
|---|---|
| Architecture overview | [`docs/windows/04-recommended-architecture.md`](docs/windows/04-recommended-architecture.md) |
| 10-phase execution plan | [`docs/windows/plan/00-master-plan.md`](docs/windows/plan/00-master-plan.md) |
| Subsystem specs (14 docs) | [`docs/windows/spec/`](docs/windows/spec/) |
| Performance budgets | [`docs/PERFORMANCE.md`](docs/PERFORMANCE.md) |
| Release runbook | [`docs/RELEASE.md`](docs/RELEASE.md) |
| Beta channel | [`BETA.md`](BETA.md) |
| Support + escalation | [`SUPPORT.md`](SUPPORT.md) |
| Contributor guide | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| Security policy | [`SECURITY.md`](SECURITY.md) |

## Acknowledgements

- [`steipete/CodexBar`](https://github.com/steipete/CodexBar) — the original macOS project. MIT. Every behaviour in CodexBar4Windows is sourced from a deep read of the Swift code, then re-implemented for Windows-native semantics.
- [`Finesssee/Win-CodexBar`](https://github.com/Finesssee/Win-CodexBar) — a community Windows port that proved the Tauri-plus-Rust shape. We don't import their source; the shape of the stack is theirs.

## License

MIT. See [LICENSE](LICENSE).
