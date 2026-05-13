# Support

How to get help with CodexBar4Windows and how to escalate broken builds.

## Where to ask

| Type | Channel | Response time |
|---|---|---|
| Bug report | [Issues](https://github.com/JRub19/CodexBar4Windows/issues/new/choose) | Best-effort, typically <1 week |
| Feature request | [Issues](https://github.com/JRub19/CodexBar4Windows/issues/new/choose) | Triaged into the backlog |
| Usage question | [Discussions](https://github.com/JRub19/CodexBar4Windows/discussions) | Community-driven |
| Security issue | See [SECURITY.md](SECURITY.md) | Disclosure path |
| P0 escalation | See below | Same-day acknowledgement |

## What counts as P0

These are issues that warrant a same-day acknowledgement (we'll triage within 24 hours and ship a hotfix within ~1 week if confirmed):

1. **Crash on launch.** The tray icon never appears; the process exits before reaching `Shell_NotifyIcon NIM_ADD`. Reproducible on a clean Windows 10/11 install.
2. **Data loss.** Settings, sign-ins, or cookie cache erased without user action.
3. **Credential exfiltration.** Any path that writes a secret outside `%APPDATA%\CodexBar4Windows\secrets\` (DPAPI-wrapped) or transmits it to a non-provider endpoint.
4. **Active update poisoning.** An update banner appears that points at an unsigned or tampered installer (the runtime guard should make this impossible, but if it slips through it's P0).
5. **Tray icon never paints / phantom icon.** Icon corruption that leaves an orphaned slot in the system tray after uninstall.
6. **OS-level handle leak.** GDI/USER handle count climbs unbounded over hours, eventually triggering Windows-wide UI corruption.

For anything else, the regular bug template is the right channel.

## How to file a P0

1. Open a [Bug Report](https://github.com/JRub19/CodexBar4Windows/issues/new?template=bug.yml).
2. Add the **`P0`** label.
3. In the title, prefix with `[P0]`.
4. Include:
   - Your Windows build (`winver`).
   - Your installed CodexBar4Windows version (from Preferences → About).
   - A copy of `%LOCALAPPDATA%\CodexBar4Windows\logs\codexbar.log` (rolling daily file).
   - Exact reproduction steps.
   - What you expected vs what happened.
5. Mention `@JRub19` in the issue body for a direct ping.

If the issue involves credential exposure, **do not file a public issue**. Follow [SECURITY.md](SECURITY.md) — the disclosure path is a private GitHub Security Advisory.

## What information to gather

Before filing any bug, please collect:

### Version + build info

```powershell
# Installed version (from the popup About pane, or):
Get-Command codexbar4windows-desktop.exe |
  ForEach-Object { (Get-Item $_.Source).VersionInfo.ProductVersion }

# Windows build:
winver

# WebView2 runtime version:
Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' |
  Select-Object pv
```

### Logs

```powershell
# Rolling daily log file:
%LOCALAPPDATA%\CodexBar4Windows\logs\codexbar.log

# Settings + state (sometimes useful for repro):
%APPDATA%\CodexBar4Windows\config.json
%APPDATA%\CodexBar4Windows\state.json
```

Redact any provider sign-in cookies/tokens you see in the log before pasting.

### Repro recipe

A one-line repro beats a paragraph of prose. Examples that work well:

- "Open popup → click Settings cog → click Claude row → Preferences crashes with the attached panic trace."
- "Launch app on a machine with no `~\.claude\` directory → tray icon paints, but Claude card shows `No usage yet` forever (expected: card shows the empty-state hint within 60s)."

### Reverting to a known-good build

If you've hit a P0 on the latest stable:

1. Roll back to the previous stable from the [Releases page](https://github.com/JRub19/CodexBar4Windows/releases). Run its installer; it overwrites the broken build.
2. Mark the bad release as **affected** in your bug report so other users get the same advice.

## Backporting

Hotfixes go to the most recent stable line only. If you're on an older stable (`v1.0.x` while `v1.2` is current), expect to upgrade to receive the fix — we don't maintain separate `v1.0.x` and `v1.2.x` branches.

## What we don't support

To set expectations:

- **macOS / Linux** — this is the Windows port. The macOS source is [steipete/CodexBar](https://github.com/steipete/CodexBar); please file there for macOS issues.
- **Windows on ARM** — deferred to a post-1.0 release. The x64 build does work under emulation, but is not gated by CI.
- **Windows 10 builds older than 17763 (1809)** — too old for the WebView2 evergreen runtime.
- **Custom installations** (`/USERS`, Group Policy roll-outs, mass deployment) — best-effort only. The installer is per-user by design.

## Acknowledgement window

We aim for:

- **P0**: same-day acknowledgement, triage within 24 hours.
- **Regular bug**: triage within ~1 week.
- **Feature request**: best-effort during a release cycle.

This is a small-team project. Patience appreciated.
