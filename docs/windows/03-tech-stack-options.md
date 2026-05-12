---
summary: "Honest trade-off matrix for every plausible stack we could build the Windows app on."
read_when:
  - Before committing to a stack
  - When pushing back on a stack choice
---

# 03 — Tech stack options

Six plausible directions. Scoring is honest, not flattering.

## Option A — Tauri 2 + React (TypeScript) + shared Rust core   ★ recommended

- **Tray:** `tray-icon` + `muda` crates; tray icon is a regenerated multi-size PNG/ICO buffer assigned to the `TrayIcon` each refresh — true dynamic two-bar meter.
- **Popup:** A frameless transient WebView2 window with React inside, positioned next to the tray rect (`Shell_NotifyIconGetRect`).
- **Backend:** Rust async (`tokio`, `reqwest`, `rusqlite`, `aes-gcm`, `windows`, `keyring`, `chrono`). Same crate compiles a `codexbar.exe` CLI.
- **PTY:** `portable-pty` (ConPTY-backed) for Claude CLI probes.
- **Secrets:** DPAPI for at-rest blobs, Credential Manager (`keyring` crate) for named creds.
- **Updater:** Tauri’s signed JSON-manifest updater (compatible with a GitHub Releases feed).
- **Install:** Tauri produces NSIS or WiX/MSI installers out of the box; Win-CodexBar uses Inno Setup with a WebView2 bootstrapper.

**Pros**
- Proven: every healthy fork and most comparable tray apps (Raycast Windows previews, Tabby, Logseq, Cursor, etc.) use this shape.
- Tiny installed footprint (5–15 MB), low idle RAM (<50 MB), uses the OS WebView2 — no Chromium bundled.
- The provider plumbing (HTTP/JSON/SQLite/regex/CLI parsing) maps 1:1 from Swift to Rust at roughly the same line count.
- Win-CodexBar is **already this stack**, MIT-licensed, with 40 providers ported. Reuse value is enormous.
- One Rust crate produces both the GUI app and the CLI binary, like upstream.

**Cons**
- Two languages (Rust + TypeScript). Acceptable trade — they have distinct concerns (backend vs. popup UI).
- WebView2 must be installed (the installer bootstraps it for Win10/11 < 22H2). Tauri handles this.
- Rust learning curve if you don’t know it. The provider code is mostly straight-line async HTTP, which is the easy 80% of Rust.

**When this is wrong:** If you’re a pure C#/.NET shop and adding Rust is politically a no-go. Or if you want to literally compile the Swift sources on Windows (option F).

## Option B — Electron + TypeScript

- **Tray:** Electron’s `Tray` API; dynamic icon = generate PNG buffer with `node-canvas` or `sharp`, hand to `tray.setImage(nativeImage)`.
- **Popup:** A `BrowserWindow` with `frame: false`, `transparent: true`.
- **Backend:** Node.js — works fine for HTTP/JSON; awkward for ConPTY (use `node-pty`), awkward for DPAPI (use `windows-dpapi` or call out to a native addon).
- **Install:** electron-builder → NSIS.
- **Updater:** electron-updater.

**Pros**
- Lower bar of entry; pure JS/TS.
- Lots of recipes and Stack Overflow coverage.

**Cons**
- **Heavy**: 90–120 MB installed, 150–300 MB RAM at idle. For a tray app that just polls APIs, this is gross.
- DPAPI / Credential Manager via native modules — one extra layer of pain per Windows release.
- No real CLI peer; you can ship a Node CLI but you’ll regret it on user machines without Node.
- Doesn’t reduce porting work — you still re-implement every provider from scratch.
- Existing Electron fork (`UsageBar`) only ships 3 providers tested.

**Verdict:** Works, but strictly worse than Tauri for this app shape. Choose only if Rust is a hard constraint.

## Option C — .NET 8 / 9 + WinUI 3 (XAML) + C#

- **Tray:** `H.NotifyIcon` library (or `WinForms NotifyIcon` interop) — WinUI 3 itself has no first-party tray API as of early 2026.
- **Popup:** A frameless XAML window with Mica/Acrylic chrome.
- **Backend:** .NET — best-in-class for Windows APIs (DPAPI via `ProtectedData`, Credential Manager via `CredWrite`, ConPTY via `System.Diagnostics.Process` + `PseudoConsole`).
- **Install:** MSIX (Store), or WiX MSI, or Velopack.

**Pros**
- First-class Windows look-and-feel (Mica, Fluent, native theming).
- Single language. Excellent debugging story in Visual Studio.
- Easy code signing & Store submission.

**Cons**
- WinUI 3 + tray is a known rough edge — `H.NotifyIcon` works but is third-party.
- HTTP/JSON/SQLite porting still required from scratch — no Win-CodexBar reuse.
- The two existing C# attempts (`nek0der`, `rjdoesntcode`) both stalled. That’s circumstantial but consistent.
- Larger installed footprint than Tauri (~40 MB self-contained), comparable RAM.
- Cross-platform CLI is awkward; AOT-compiled .NET is possible but adds complexity.

**Verdict:** Plausible, but you trade a proven 40-provider Rust core for a from-scratch C# rewrite. Only choose if you want every line in C#.

## Option D — Avalonia 11 + C#

- **Tray:** Avalonia has built-in `TrayIcon` with dynamic image support, cross-platform.
- **Popup:** Avalonia window with native chrome.
- **Backend:** .NET, same as Option C.
- **Install:** Velopack / Squirrel / Inno.

**Pros**
- True cross-platform (Windows + Linux + macOS) from one codebase if you ever want that.
- Cleaner XAML than WinUI 3.

**Cons**
- Looks "close to but not quite Windows-native" — Mica/Acrylic isn’t out-of-the-box, fonts and corner radius differ subtly.
- Same from-scratch rewrite as Option C; no reuse of Win-CodexBar.
- Smaller ecosystem.

**Verdict:** Best C# option if cross-platform is a future goal. Otherwise no advantage over the proven Tauri+Rust path.

## Option E — Flutter Desktop

- `tray_manager` plugin + `window_manager` for frameless popup. Hot reload during dev.

**Pros**
- Slick UI, hot reload, single Dart codebase.

**Cons**
- Desktop story is still less mature than mobile; tray + popup recipes are thinner than Tauri’s.
- Dart for backend work is fine but isolates the project from the Rust ecosystem of Windows-specific crates we’ll need (DPAPI, ConPTY).
- Zero existing CodexBar work in Dart.

**Verdict:** Skip unless you already love Flutter.

## Option F — Swift on Windows (compile the existing sources)

- Swift 6 has a Windows toolchain. `swift-corelibs-foundation` works. AppKit does not exist on Windows; SwiftUI does not exist on Windows.

**Pros**
- "Literally the same code" appeal.

**Cons**
- All the UI — `NSStatusBar`, `NSStatusItem`, `NSMenu`, `NSPopover`, SwiftUI prefs — has no Windows counterpart and must be replaced entirely.
- `Security.framework`, `ServiceManagement`, `WidgetKit`, `KeyboardShortcuts` (sindresorhus), `Vortex`, `Sparkle` — none compile on Windows.
- `SweetCookieKit` is macOS-only (its purpose is Mac browser cookie extraction).
- After removing the macOS-only parts you’re left with maybe 30–40% of `CodexBarCore` (the pure model/parsing code) compiling, and then you’re binding it to a Windows GUI written in… what? You end up at Tauri+Rust or WinUI+C# anyway, with an extra integration boundary across a Swift FFI.
- No-one in the community attempted this. That’s the loudest signal.

**Verdict:** Strongly against. The Swift sources are a *reference implementation* for behavior, not a portable codebase.

## Decision matrix

| Criterion | A — Tauri+Rust | B — Electron | C — WinUI+C# | D — Avalonia+C# | E — Flutter | F — Swift on Windows |
|---|---|---|---|---|---|---|
| Native Windows look | Good (WebView2 + custom CSS) | Average | **Best** | Good | Average | n/a |
| Installed footprint | **~10 MB** | 100 MB | 40 MB | 30 MB | 25 MB | unknown |
| Idle RAM | <60 MB | 200 MB | 100 MB | 90 MB | 80 MB | unknown |
| Dynamic tray icon | **Proven** | Yes | Yes (3rd-party) | Yes | Yes | n/a |
| Reuse of Win-CodexBar | **Full** | None | None | None | None | None |
| Provider porting effort | **Low (reuse 40)** | High | High | High | High | High |
| ConPTY / Claude CLI | `portable-pty` | `node-pty` | First-class | First-class | Manual | Manual |
| DPAPI / Credential Mgr | `keyring` crate | Native module pain | First-class | First-class | Manual | Manual |
| Updater story | Built-in | electron-updater | Velopack | Velopack | Manual | Manual |
| MSI/installer | NSIS/WiX built-in | NSIS built-in | MSIX/WiX | Velopack | Manual | Manual |
| Same CLI binary as GUI | **Yes (single cargo workspace)** | No | Possible | Possible | No | No |
| Community precedent | 392★ proven | 4★ partial | 12★ stalled | none | none | none |
| Risk of stalling | Low | Medium | High (observed) | Medium | Medium | Very high |

## Recommendation

**Option A — Tauri 2 + React + shared Rust core**, modelled directly on `Finesssee/Win-CodexBar`. See [04-recommended-architecture.md](04-recommended-architecture.md) for the concrete shape.

Second choice (if Rust is off-limits politically): **Option C — WinUI 3 + .NET 8** with `H.NotifyIcon`, accepting a from-scratch port.
