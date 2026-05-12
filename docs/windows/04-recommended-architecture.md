---
summary: "Concrete target architecture for CodexBar on Windows."
read_when:
  - Implementing the port
  - Reviewing PRs that change module boundaries
---

# 04 — Recommended architecture

## Headline

Adopt the **`Finesssee/Win-CodexBar` architecture** as the base: **Tauri 2 (React + TS popup) + a shared Rust core crate** that owns providers, settings, browser cookies, secrets, status polling, cost scanning, ConPTY, and the tray-icon renderer. Same crate compiles a `codexbar.exe` CLI in parallel with the desktop app.

Two paths to get there. Both end in the same place — pick based on how much of a "clean repo" feel you want.

### Path 1 — Rebase this fork onto Win-CodexBar (fastest)

1. Wipe `Sources/`, `Tests/`, `TestsLinux/`, `Package.swift`, `.swiftformat`, `.swiftlint.yml`, the `Scripts/` shell scripts, `Makefile`, `Icon.icon`, `appcast.xml`, `Package.resolved`. Keep `README.md` (rewrite), `LICENSE`, `CHANGELOG.md` (annotate), `docs/`.
2. Import the Win-CodexBar tree as a baseline: `rust/`, `apps/desktop-tauri/`, `extra-docs/`, `scripts/`, `Cargo.toml`, `Cargo.lock`, `dev.ps1`, `dev.sh`, `version.env`.
3. Preserve their MIT `LICENSE` and add upstream attribution: "Forked from steipete/CodexBar (MIT) and Finesssee/Win-CodexBar (MIT)."
4. Reset the version. Rename app identifier from `Win-CodexBar` to whatever you ship under (suggest keeping `CodexBar` and disambiguating by org).
5. Open issues for known upstream-Mac features that aren’t yet in Win-CodexBar and triage them.

Pros: weeks, not months. Get to a working build on day one.
Cons: lots of inherited code you didn’t write. Mitigated by their MIT license and clean module layout.

### Path 2 — Rebuild in place with the same architecture (cleaner)

1. Same Swift wipe as above.
2. Set up an empty Cargo workspace with the *same module layout* (`rust/src/{providers,browser,tray,host,...}`, `apps/desktop-tauri`).
3. For each module, write your own implementation, using Win-CodexBar as a *reference* (read, don’t copy-paste) and the original Swift sources as the behavioral spec.
4. Land providers one at a time, starting with the highest-traffic three (Codex, Claude, Cursor).

Pros: every line is yours; cleaner provenance; easier to deviate when needed.
Cons: months of work to reach parity. Burns the project’s biggest reuse opportunity.

**Default recommendation: Path 1**, with a one-week dedicated cleanup pass after import to delete anything we don’t want and replace anything that smells, before the first tagged release.

## Module layout (target)

```
/Cargo.toml                          # workspace
/rust/
  Cargo.toml                         # core crate "codexbar"
  src/
    lib.rs                           # public API surface
    main.rs                          # CLI entry (when built as bin)
    core/                            # provider registry, dispatcher
    providers/
      claude/   codex/   cursor/   copilot/   gemini/  ...   (one folder per provider)
    browser/                         # Chromium DPAPI + Firefox SQLite cookie import
    tray/
      icon.rs                        # dynamic icon: tiny-skia/resvg → multi-size buffer
      render.rs                      # bar-meter, brand, indicator overlay rendering
      mod.rs
    secure_file.rs                   # DPAPI-wrapped at-rest blob format
    settings.rs                      # serde-backed config; %APPDATA%\CodexBar\config.json
    login.rs                         # OAuth device-flow shared helpers
    status.rs                        # provider status / incidents polling
    cost_scanner.rs                  # Claude/Codex JSONL log scanner
    notifications.rs                 # Windows toast wrapper
    shortcuts.rs                     # global hotkeys (RegisterHotKey)
    sound.rs                         # optional sound on warning thresholds
    updater.rs                       # Tauri updater bindings (if app), GitHub releases (if CLI)
    wsl.rs                           # WSL detection + fallback hints
    locale.rs                        # i18n strings
    logging.rs                       # tracing init
    host/                            # IPC types shared with the Tauri shell
/apps/
  desktop-tauri/
    src-tauri/                       # Rust Tauri shell; thin wrapper that calls core
      tauri.conf.json
      src/main.rs                    # registers commands, owns tray + windows
      capabilities/                  # Tauri ACL
    src/                             # React + TS popup app
      components/
      hooks/
      styles/
      App.tsx
    index.html
    package.json
    vite.config.ts
/extra-docs/                         # Windows-specific deep-dives (BUILDING, COOKIES, WSL)
/docs/                               # parity docs, mirrored/forked from upstream
/scripts/                            # PowerShell helpers (build, sign, release)
/dev.ps1                             # one-shot dev loop
/version.env
/README.md
```

## Data flow

```
┌──────────────────────────────┐
│  Tauri shell (Rust)          │
│  - owns TrayIcon             │
│  - owns popup BrowserWindow  │
│  - owns right-click menu     │
│  - registers IPC commands    │
└──────────────┬───────────────┘
               │ Tauri commands (serde-over-IPC)
┌──────────────▼───────────────┐
│  React popup (TS)            │
│  - provider cards, bars      │
│  - settings UI               │
│  - charts (Recharts/uplot)   │
└──────────────────────────────┘
               ▲
               │ events ("usage:updated")
┌──────────────┴───────────────┐
│  codexbar core (Rust)        │
│                              │
│  refresh_loop ──► UsageStore │
│        │                     │
│        ▼                     │
│  provider dispatch           │
│  ┌──────────────────────┐    │
│  │ ClaudeFetcher        │    │
│  │ CodexFetcher         │    │
│  │ CursorFetcher        │    │
│  │ ...39 more           │    │
│  └──────────────────────┘    │
│        │                     │
│        ▼                     │
│  http + sqlite + DPAPI       │
│  + ConPTY + JSONL scanner    │
└──────────────────────────────┘
```

- **Refresh loop** runs in `tokio`. Cadence reads from settings (Manual / 1m / 2m / 5m / 15m / 30m). On each tick: dispatch provider fetchers in parallel, fold results into `UsageStore`, emit `usage:updated` event, rebuild the tray icon, push state into the popup if it’s open.
- **Tray icon redraw** is the only thing that runs *every* refresh. Implementation: render to a 32-bit RGBA buffer with `tiny-skia` at 16/20/32/40 px, build an in-memory ICO, hand to `tray-icon::TrayIcon::set_icon`. Cache by `(primary_pct, weekly_pct, stale, indicator, style, theme)` to avoid wasted draws.
- **Popup** is a frameless transient `WebviewWindow`. Open on left-click; close on focus-loss. Position calc reads the tray rect via `Shell_NotifyIconGetRect` (in `Shell_NotifyIcon`’s `NOTIFYICON_VERSION_4` mode) and offsets above-or-below based on which screen edge holds the taskbar.
- **Right-click menu** is native (`muda` crate). Items: *Refresh now*, *Preferences…*, *About*, *Quit*. Refresh and Preferences also live inside the popup, but the native menu is what Windows users reach for.
- **Settings storage**: `%APPDATA%\CodexBar\config.json` (non-sensitive) + DPAPI-wrapped per-account credential blobs alongside. Named credentials (e.g., per-provider OAuth) go to **Windows Credential Manager** via `keyring`.
- **Secrets policy**: never log raw tokens, cookies, or user-identifying account fields. Mirror the original repo’s `PersonalInfoRedactor.swift` semantics in `logging.rs`.

## Cross-cutting decisions

| Concern | Decision |
|---|---|
| **Async runtime** | `tokio` multi-threaded |
| **HTTP** | `reqwest` with `rustls` (avoid bundling OpenSSL) |
| **JSON** | `serde_json` |
| **SQLite** | `rusqlite` with `bundled` feature (Chrome/Firefox cookie DBs) |
| **Encryption** | `aes-gcm` for Chromium V10 cookies; for V20 ("App-Bound Encryption"), fall back to manual cookie paste |
| **DPAPI** | `windows` 0.58 crate, raw `CryptProtectData`/`CryptUnprotectData` |
| **Credential storage** | `keyring` 3 (Windows Credential Manager) for OAuth refresh tokens |
| **PTY** | `portable-pty` (ConPTY backend) |
| **Tray** | `tray-icon` + `muda` |
| **Window** | Tauri built on `winit` 0.30 |
| **Icon rendering** | `tiny-skia` (no GPU dependency), `resvg`/`usvg` for SVG brand icons |
| **Logging** | `tracing` + `tracing-subscriber` writing to `%LOCALAPPDATA%\CodexBar\logs\codexbar.log` |
| **Updater** | Tauri updater plugin, signed JSON manifest in GitHub Releases |
| **i18n** | `serde` + locale files mirroring the upstream Mac `Localizable.xcstrings` strings; keep Brazilian Portuguese already in upstream |
| **Test** | `cargo test` per crate, plus Playwright e2e against the Tauri dev build for the popup |

## Two binaries, one crate

The Rust workspace builds two artifacts:

1. **`codexbar-desktop.exe`** — Tauri shell. Tray + popup + settings + updater.
2. **`codexbar.exe`** — CLI peer (`codexbar usage`, `codexbar cost --provider claude`, etc.). Same `codexbar` crate, different `[[bin]]` entry. Same on-disk config. Useful for scripts, CI, status-bar tools (Powerline / oh-my-posh).

This mirrors the upstream Mac repo (`CodexBar.app` + `bin/codexbar`) and is one of Win-CodexBar’s sharpest design choices.

## What we deliberately drop

- **WidgetKit widgets** — no Windows equivalent worth shipping at v1.
- **Safari cookie import** — Safari doesn’t exist on Windows.
- **Sparkle macOS appcast** — replaced by Tauri updater.
- **macOS Keychain prompt policies, Security CLI reader** — Windows DPAPI has no equivalent prompt flow.
- **macOS Full Disk Access prompt UX** — Windows doesn’t have FDA; ACL on `%APPDATA%` is enough.
- **`ictool` icon pipeline** — replace with a single PNG → ICO pipeline using `image` crate.

## What we keep at v1 (parity targets)

- All 30+ providers from upstream.
- Per-provider tray icon **or** "Merge Icons" mode with provider switcher.
- Dynamic two-bar tray meter (session + weekly), dim on stale, incident indicator overlay.
- Provider status polling.
- Cost-usage scan for Claude + Codex over the last 30 days.
- Manual / OAuth / browser-cookie / CLI auth paths per provider.
- Refresh cadence presets.
- Notifications + optional reset celebration (Windows toast with hero image instead of Vortex confetti).
- CLI peer with `usage`, `cost`, `status` subcommands.
