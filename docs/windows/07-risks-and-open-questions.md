---
summary: "Things that will or might bite the Windows port."
read_when:
  - Before committing to scope or a deadline
  - When something starts going sideways
---

# 07 — Risks and open questions

Sorted roughly by likelihood × impact.

## High

### R1 — Chrome v127+ App-Bound Encryption (V20) for cookies

Google rolled out **App-Bound Encryption** in Chrome 127 (mid-2024). Encrypted cookie blobs now begin with a `v20` prefix instead of `v10` and the key is wrapped using a per-app COM call that ties decryption to a service running with the user’s privileges. Plain DPAPI no longer suffices.

- **Impact:** any provider that depends on Chrome cookies (Cursor, OpenCode, Augment, Abacus, Mistral, MiniMax cookie path, Manus, Command Code, Doubao, Amp, Ollama, Alibaba) silently fails on Chrome ≥ 127 unless we work around it.
- **Workarounds:**
  - **Manual cookie paste** UI (must exist anyway for users with no browser installed).
  - **Edge / Brave** are forks but ship the same `os_crypt` code; if their roll-out lags, they may still work via v10 — verify.
  - **Firefox** is unaffected (unencrypted SQLite).
  - **The exotic "decrypt as the elevated COM service" route** is possible (some open-source tools do it) but is fragile and looks like malware behavior to AV.
- **Decision needed:** do we ship v1 with "Chrome cookies require manual paste; Edge/Firefox auto" and revisit later, or block the launch on building the elevated-COM workaround?
- **Recommendation:** ship without the workaround. Document it loudly. Re-evaluate in 6 months.

### R2 — SmartScreen / antivirus false positives

Unsigned EXEs that talk to OAuth endpoints, read browser cookie databases, and run PTY children look exactly like info-stealers to antivirus heuristics. Without a code-signing cert and a reputation curve, every install will get a SmartScreen warning.

- **Mitigation:**
  - Buy an **OV cert** at minimum (~$200/yr); ideally **EV** for instant reputation.
  - Sign installer + main EXE + any helper EXEs (CLI, watchdog).
  - Publish a SHA-256 alongside each release.
  - Submit the binary to Microsoft’s Defender false-positive form on each release for the first few months.
- **Decision needed:** who pays for the cert and registers the org?

### R3 — Win-CodexBar maintenance risk

`Finesssee/Win-CodexBar` is a one-maintainer project, like upstream. If it stalls, the only source of new providers is upstream Swift — and we can’t auto-port that.

- **Mitigation:**
  - Treat the imported code as ours from day one (no live dependency on upstream Win-CodexBar; it’s a snapshot we maintain).
  - Build the parity-audit muscle in Phase 2.
  - Pick one or two of the highest-traffic providers (Claude, Codex) and re-implement them ourselves early to prove we *can* write a new provider end-to-end without leaning on the import.

### R4 — Tray icon hidden in overflow

Default Windows 10/11 behavior is to hide our tray icon behind the chevron flyout. Many users will conclude the app didn’t install correctly.

- **Mitigation:** first-run toast walking them through pinning the icon (see [05 §1](05-windows-ux-spec.md)). Cover this in the README + onboarding video.

## Medium

### R5 — ConPTY behavior differences for the Claude CLI

The Claude CLI’s `/usage` panel renders with ANSI sequences, Unicode box-drawing, and spinner characters. The upstream Swift parser was tuned against macOS PTY output. ConPTY normalizes some sequences differently and uses CRLF.

- **Mitigation:** dedicated fixture-based parser tests using captured ConPTY transcripts; CI runs them on Windows.

### R6 — Per-monitor DPI

The popup is positioned near the tray rect, which lives on the monitor that owns the taskbar. Mixed-DPI multi-monitor setups will mis-place the popup if we don’t opt into `PerMonitorAwareV2`.

- **Mitigation:** explicit DPI awareness in the Tauri config; tests on a 4K + 1080p side-by-side setup.

### R7 — WebView2 runtime missing on locked-down corporate Windows 10

Some managed environments do not ship the WebView2 evergreen runtime. Tauri’s installer bootstraps it but requires network access for the redistributable.

- **Mitigation:** offer a "fixed-version WebView2" bundled build for offline / locked-down customers, in addition to the slim bootstrapper build.

### R8 — Auto-update on user-install vs machine-install

Inno Setup can install per-user (no admin) or per-machine (needs admin). Tauri’s updater expects to replace the EXE on disk; if the user lacks write access to `Program Files` the update silently fails.

- **Mitigation:** default to per-user install (`%LOCALAPPDATA%\Programs\CodexBar`); offer per-machine as an advanced toggle.

### R9 — Localization drift

Upstream just added Brazilian Portuguese (commit `22c44848`). Win-CodexBar has its own locale system. Strings will diverge.

- **Mitigation:** mirror upstream `xcstrings` keys; write a small script to flag missing keys after each upstream sync.

## Lower

### R10 — Swift-side recent fixes we’ll miss

Upstream commits in just the last week of the recent log:
- `009420a7` — hide quota warning markers (display feature)
- `c7729cd0` — stabilize Codex account switcher layout
- `0cb8abd8` — Moonshot / Kimi API provider support
- `22c44848` — Brazilian Portuguese
- `a01bf8c9` — apply selected app language

These need to land in Windows too. Bake into the Phase 2 parity audit.

### R11 — Tray click-vs-context-menu race on Windows

`Shell_NotifyIcon` reports both `NIN_SELECT` (click) and `WM_CONTEXTMENU` (right-click). Some users have low-precision mice and trigger both. We need to be sure we dismiss the popup if the user clicks again on the icon, and don’t open the popup if the right-click context menu is open.

- **Mitigation:** small state machine in the tray handler. Tauri’s `tray-icon` exposes both events; just be deliberate.

### R12 — Multi-language Claude CLI output

The Claude CLI’s `/usage` panel labels translate. The upstream parser keys off the English headers ("Current session", "Current week"). If the user’s shell locale forces a different language, the parser misses.

- **Mitigation:** detect via parsing failure and fall back to OAuth/Web; document a `LANG=en_US.UTF-8` workaround.

## Open questions for you to answer

1. **Project identity** — keep "CodexBar" as the name on Windows, or pick a distinct name (e.g., "CodexBar for Windows," "TokenBar," etc.)? Affects executable name, registry keys, AUMID, install path. Worth deciding before Phase 1.

2. **Org / signing** — under whose legal entity will the code-signing certificate be issued? This blocks the install story more than the code does.

3. **Fork strategy** — Path 1 (import Win-CodexBar, then iterate) or Path 2 (clean rebuild)? Default recommendation is Path 1. If Path 1: do we want to also keep a fast-forward relationship with `Finesssee/Win-CodexBar` (rebase on their releases) or treat the import as a one-time snapshot?

4. **Provider scope at v1** — full 30+ parity, or trim to a "Claude / Codex / Cursor / Copilot / Gemini" launch set and add the long tail in v1.1?

5. **MSIX / Store** — are we publishing to the Microsoft Store, or sticking to GitHub Releases + Winget? Store gets us reputation and auto-update for free but constrains the install layout.

6. **Telemetry** — opt-in error reports only, or also opt-in usage analytics? The Mac app has no telemetry; matching that is the safe default.

7. **Donation / pricing model** — Mac CodexBar is free with a Buy Me a Coffee link. Keep the same on Windows or change?

8. **WSL stance** — formally support running CodexBar against `claude` under WSL when no native Windows binary exists, or refuse and require native? Win-CodexBar supports it; we can inherit.
