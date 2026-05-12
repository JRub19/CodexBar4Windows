---
summary: "Concrete map of every macOS-only API CodexBar uses and what Windows offers in its place."
read_when:
  - Before estimating port effort
  - When deciding which stack can serve a given subsystem
---

# 01 — What CodexBar depends on macOS for

Don’t guess scope. This is the actual surface, taken from the Swift sources in this fork.

## 1. Status bar / tray

| macOS today | What it does | Windows equivalent |
|---|---|---|
| `NSStatusBar.system.statusItem(...)` | Per-provider icons in the menu bar; one item per provider or merged | `Shell_NotifyIcon` (Win32) / `NotifyIcon` (.NET) / `tray-icon` crate (Rust) |
| `NSStatusItem.button.image` + 18×18 template image | Live-rendered dynamic icon (two bar meter, dim on stale, incident dot, brand mode) | 16×16 / 20×20 / 32×32 multi-size ICO atlas, regenerated in memory on each refresh and `Shell_NotifyIcon(NIM_MODIFY)` |
| Template image auto-dark/light | macOS inverts template images for light/dark menu bars | Detect taskbar theme from `Personalize\SystemUsesLightTheme` reg key and render two variants |
| Status-item autosave names | macOS preserves position when the user drags items around | Windows hides infrequent tray icons by default → must educate users to drag the icon out of the overflow flyout once |
| `NSPopover` anchored to the status item | Click → rich popup with provider tiles | A frameless, click-outside-dismiss top-level window positioned near the cursor / tray rectangle (`Shell_NotifyIconGetRect`) |

**Key gotcha:** macOS lets you keep menu-bar position; Windows does not. Plan a first-run nudge to drag the icon to the visible tray (the “show in taskbar corner” flyout).

## 2. Menus, popups, hosted SwiftUI

- `StatusItemController*.swift` (12 files) — drives `NSMenu` construction, hosted SwiftUI submenus, animated menu transitions, hover highlight.
- `MenuCardView.swift` and friends — SwiftUI provider cards with bars, countdowns, sparklines.
- `Vortex` package — particle confetti for weekly reset.

There is **no** direct port for hosted SwiftUI in `NSMenu`. The realistic options:

- Render everything inside a single frameless popup window (Tauri/WebView2 React, or WinUI/XAML, or a native Win32 layered window). Drop the “submenu out of native NSMenu” idiom; everything lives in the popup.
- Keep a minimal native right-click context menu (`muda` crate or `TrackPopupMenu`) for *Quit / Refresh now / Preferences*, since Windows users expect right-click on the tray to be a real OS menu.

## 3. Secrets, Keychain, browser cookies

| macOS today | What it does | Windows equivalent |
|---|---|---|
| `Security.framework` Keychain via `SecItem*` | Stores OAuth tokens, cached cookie headers, account credentials | **DPAPI** (`CryptProtectData` / `CryptUnprotectData`) for at-rest blobs; **Windows Credential Manager** (`CredWrite`) for named credentials; Rust crate: `keyring` 3.x (wraps Credential Manager), `windows` crate for raw DPAPI |
| Chrome/Chromium on macOS: `Safe Storage` Keychain item | Decrypts Chrome cookies | Chromium on Windows: `Local State` JSON → `os_crypt.encrypted_key` is DPAPI-wrapped AES-256-GCM key. Newer Chrome (v127+) uses **App-Bound Encryption (V20)** which is harder; fall back to manual cookie paste for those |
| Safari `~/Library/Cookies/Cookies.binarycookies` | Reads Safari cookies | **No equivalent.** Drop Safari path. Edge/Chrome/Brave/Firefox cover Windows; document Safari as macOS-only |
| Firefox `cookies.sqlite` on macOS | Reads Firefox cookies | Firefox uses the same unencrypted SQLite store at `%APPDATA%\Mozilla\Firefox\Profiles\*\cookies.sqlite` — same code, different path |
| `~/.codexbar/config.json` with 0600 perms | App config + manual tokens | `%APPDATA%\CodexBar\config.json` with NTFS ACL restricted to the current user (use `SetNamedSecurityInfo` or simply DPAPI-encrypt the sensitive parts) |

## 4. CLI integration / PTY

- `ClaudeStatusProbe.swift`, `ClaudeCLISession.swift` and `Sources/CodexBarClaudeWatchdog` use a **PTY** to drive the `claude` CLI and parse its `/usage` panel.
- Codex CLI is launched as a subprocess and produces JSON.

Windows realities:

- There is no `forkpty`. Use the **ConPTY** API (`CreatePseudoConsole`) introduced in Windows 10 1809+. Rust: `portable-pty` or `conpty` crate. .NET: `System.Management.Automation` host or third-party libs.
- The Claude CLI is also distributed on Windows via npm/`claude-code`. Verify Windows binaries exist and that the parser handles CRLF and Windows-style spinner glyphs.
- WSL is a legitimate fallback: many users will already run `claude` inside WSL. Win-CodexBar supports this. We should too.

## 5. Auto-update

- Mac uses **Sparkle 2.x** with an `appcast.xml`.
- Windows equivalents: **WinSparkle** (drop-in), **Squirrel.Windows**, **Velopack**, **Tauri’s built-in updater** (signed JSON manifest + zipped bundle).
- Recommendation: Tauri’s updater if we go Tauri; otherwise Velopack (modern Squirrel successor, simple).

## 6. Notifications / toasts

- Mac uses `UserNotifications`.
- Windows uses **Toast Notifications** (`Windows.UI.Notifications.ToastNotificationManager`). Requires an AUMID, ideally registered via an MSIX-shortcut or a registry-based COM activator.
- Rust crates: `winrt-notification`, `tauri-plugin-notification`.

## 7. Autostart / Launch at login

- Mac uses `ServiceManagement` / `SMAppService`.
- Windows: write `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CodexBar` or place a shortcut in `shell:startup`. MSIX packages can declare a startup task in their manifest.

## 8. Global hotkeys

- Mac uses **sindresorhus/KeyboardShortcuts**.
- Windows: `RegisterHotKey` Win32 API (per-thread), or `global-hotkey` Rust crate, or Tauri’s global-shortcut plugin.

## 9. Widgets

- `Sources/CodexBarWidget` is a WidgetKit extension — **macOS/iOS-only.** No port. Windows has no first-class widget equivalent at the platform level.
- The roadmap should explicitly *not* try to ship widgets at v1. If demand emerges, the Windows 11 “widgets board” has an Adaptive Cards API that could be revisited, but it is a separate, optional surface.

## 10. App-bundle / packaging

- Mac: `.app` bundle, Sparkle, Homebrew cask, notarization (Developer ID + `notarytool`).
- Windows: a signed `.exe` installer (Inno Setup or WiX/MSI), optionally an MSIX/AppX, optionally a Winget manifest. Signing requires an EV or OV code-signing cert (avoid the SmartScreen warning).

## 11. Filesystem layout — every config / cache path the app reads

We hit the FS in a lot of places. These are the ones that need a per-platform mapping:

| Mac path | Purpose | Windows mapping |
|---|---|---|
| `~/.codexbar/config.json` | App config + manual tokens | `%APPDATA%\CodexBar\config.json` |
| `~/Library/Caches/CodexBar/cost-usage/*.json` | Cost-scan cache | `%LOCALAPPDATA%\CodexBar\cache\cost-usage\*.json` |
| `~/.claude/projects/**/*.jsonl` | Claude assistant logs (cost scan) | `%USERPROFILE%\.claude\projects\**\*.jsonl` (Claude CLI uses the same path on Windows) |
| `$CLAUDE_CONFIG_DIR/projects` | Override | Same env var; document on Windows |
| `~/.claude/.credentials.json` | Claude OAuth cache fallback | Same path |
| `~/Library/Application Support/Google/Chrome/*/Cookies` | Chrome cookies | `%LOCALAPPDATA%\Google\Chrome\User Data\*\Network\Cookies` (note `Network\` subfolder on recent Chrome) |
| `~/Library/Application Support/Firefox/Profiles/*/cookies.sqlite` | Firefox cookies | `%APPDATA%\Mozilla\Firefox\Profiles\*\cookies.sqlite` |
| `~/Library/Cookies/Cookies.binarycookies` | Safari cookies | **drop** |
| `~/.config/manicode/credentials.json` | Codebuff fallback | Same path (`%USERPROFILE%\.config\...`) |
| `~/.pi/agent/sessions/**/*.jsonl` | pi sessions for cost scan | Same path |
| `~/.profile` | Sparkle/App Store Connect keys (release-time) | n/a |

All paths must go through a single `PathEnvironment`-style helper. The Mac repo already has `Sources/CodexBarCore/PathEnvironment.swift` — that file is the natural seam for the abstraction.

## 12. What is *not* a porting concern

- HTTP fetches (URLSession → `reqwest` / `HttpClient` / `fetch`).
- JSON parsing.
- Cost log parsing (pure data work).
- Status polling against vendor APIs.
- OAuth device-flow logic (mostly stateless HTTP).
- Provider-specific JSON / cookie / CLI parsing.

That’s ~70% of `CodexBarCore`. Whatever stack we pick, this code is *re-expressible*, not blocked.

## 13. Net effort estimate

- **Stays the same logic**: provider fetchers, parsers, cost scanners, pace calculators, settings model — ~150–180 files of pure logic.
- **Needs a Windows rewrite**: status item, popup window, icon renderer, keychain, browser cookie decryptor (Windows variant only), PTY runner, autostart, hotkeys, updater, packaging — ~30–40 files.
- **Drop entirely**: WidgetKit extension, Safari support, macOS Sparkle integration, `ictool` icon pipeline.

The 30–40 files of platform glue is what every fork has been re-doing. Win-CodexBar has already done it. The rest is parity work against vendor APIs.
