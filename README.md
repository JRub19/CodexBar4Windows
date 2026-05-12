# CodexBar4Windows

> Every AI coding limit, in your Windows tray.

[![CI](https://github.com/JRub19/CodexBar4Windows/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/JRub19/CodexBar4Windows/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-6e5aff?style=flat-square)](LICENSE)

CodexBar4Windows is the Windows native port of [`steipete/CodexBar`](https://github.com/steipete/CodexBar), built on Tauri 2 plus React plus a shared Rust core. It lives in the Windows notification area (tray) and keeps AI coding provider limits visible at a glance: Claude, Codex, Cursor, Copilot, Gemini, OpenRouter, Factory at v1, with the long tail to follow.

> **Status: Phase 0 baseline.** The workspace builds a green Tauri tray app. Real providers, popup cards, preferences, cost scanning, status overlays, signed installer all land in later phases. See `docs/windows/plan/00-master-plan.md` for the 10 phase execution plan.

## Install

Not shipping yet. Install paths land in Phase 9. Tracked targets:

- Inno Setup installer signed with Authenticode, per user install at `%LOCALAPPDATA%\Programs\CodexBar4Windows`.
- Portable EXE (no install, just run from any folder).
- Winget: `winget install CodexBar4Windows` (placeholder, ships at GA).

For now, build from source.

## Build from source

Requirements:

- Windows 10 1903 or newer, Windows 11 recommended.
- WebView2 evergreen runtime (preinstalled on Win 11, install from Microsoft on Win 10).
- Rust stable (`rustup install stable`, `x86_64-pc-windows-msvc` target).
- Node 22 or newer.
- MSVC Build Tools 2019 or 2022 with the C++ desktop workload.

```powershell
git clone https://github.com/JRub19/CodexBar4Windows.git
cd CodexBar4Windows\apps\desktop-tauri
npm install
npm run tauri dev
```

On first run the tray icon may live in the overflow flyout. Click the chevron on the taskbar to find it, then drag it next to the Wi Fi icon to pin it. Phase 3 adds an automatic first run nudge.

Release build:

```powershell
cd apps\desktop-tauri
npm run tauri build
```

The release EXE lands at `apps\desktop-tauri\src-tauri\target\release\codexbar4windows-desktop.exe`.

## Project layout

- `rust/`, shared core crate (`codexbar`). Providers, settings, secrets, refresh loop. Grows through Phases 1 to 7.
- `apps/desktop-tauri/`, Tauri 2 desktop shell. React TypeScript popup, Rust tray host. Renders to WebView2.
- `docs/windows/`, the planning and behavioral spec for the Windows port. `docs/windows/README.md` is the index.
- `docs/windows/plan/`, the 10 phase execution plan plus the cross phase test strategy.
- `docs/windows/spec/`, 14 subsystem blueprints derived from a deep read of the macOS sources.

## Documentation

- [`docs/windows/README.md`](docs/windows/README.md), index of all Windows port docs.
- [`docs/windows/00-recommendation.md`](docs/windows/00-recommendation.md), one page summary.
- [`docs/windows/04-recommended-architecture.md`](docs/windows/04-recommended-architecture.md), the target shape.
- [`docs/windows/plan/00-master-plan.md`](docs/windows/plan/00-master-plan.md), the 10 phase execution plan.
- [`CLAUDE.md`](CLAUDE.md), git and workflow rules contributors must follow.
- [`CONTRIBUTING.md`](CONTRIBUTING.md), contributor guide.
- [`SECURITY.md`](SECURITY.md), security policy.

## Acknowledgements

- [`steipete/CodexBar`](https://github.com/steipete/CodexBar), the original macOS project. MIT. Every line of behavior in CodexBar4Windows is sourced from a deep read of the Swift code, then re implemented for Windows.
- [`Finesssee/Win-CodexBar`](https://github.com/Finesssee/Win-CodexBar), a community Windows port that proved the Tauri plus Rust shape on Windows. We do not import their source, but the shape of the stack is theirs.

## License

MIT. See [LICENSE](LICENSE).
