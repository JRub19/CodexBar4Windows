# CodexBar4Windows — Beta channel

This document covers the Beta release channel. Stable users should ignore it.

## What is the Beta channel?

Most releases of CodexBar4Windows ship to two channels:

- **Stable** — Tagged `v<major>.<minor>.<patch>` (e.g. `v1.0.0`). Recommended for everyone. The updater fetches `https://github.com/JRub19/CodexBar4Windows/releases/latest/download/latest.json`.
- **Beta** — Tagged `v<major>.<minor>.<patch>-beta.<n>` or `-rc.<n>`. Cuts ~2 weeks before a stable. The updater fetches `https://github.com/JRub19/CodexBar4Windows/releases/latest/download/beta.json`.

Beta builds are signed with the same Authenticode + minisign keys as stable; the only difference is the manifest URL.

## Should I install the beta?

**Install the beta if:**

- You want to help shake out regressions on real hardware before they reach the stable channel.
- You're comfortable reporting issues with detailed reproduction steps + `logs/`.
- You're OK with a slightly higher rate of bugs (typically: 1-2 per beta cycle).

**Stay on stable if:**

- You depend on CodexBar4Windows for your actual workflow.
- You don't have time to file issues when something breaks.

## How to switch to Beta

Two paths:

### From the running app

1. Open **Preferences → About**.
2. Click **Check for updates** → confirm you're on the latest stable.
3. (Future: a "Channel" dropdown in About lets you switch to Beta. Until that ships, use the manual path below.)

### Manual install

Download the latest beta installer from the [Releases page](https://github.com/JRub19/CodexBar4Windows/releases) — look for the pre-release tag. Run it; it overwrites the stable install in place. From that point on the updater follows the Beta channel.

## How to switch back to Stable

Two options:

1. **Wait for the next stable.** When v1.1.0 (stable) lands, the beta updater will offer it as a same-or-newer install. Click "Update now"; the beta is overwritten with the stable build.

2. **Uninstall + reinstall** the latest stable. From Settings → Apps, uninstall CodexBar4Windows, then download the stable installer from Releases.

Your settings + sign-ins persist across the swap (they live in `%APPDATA%\CodexBar4Windows\`, which neither uninstaller touches by default).

## What's in the current beta?

See the topmost pre-release entry on the [Releases page](https://github.com/JRub19/CodexBar4Windows/releases). Each beta release note lists the diffs from the previous stable + any known issues.

## Reporting issues found in beta

Use the [Beta bug report template](https://github.com/JRub19/CodexBar4Windows/issues/new?template=beta-bug.yml). Critical: include

- The exact version string from **Preferences → About** (e.g. `1.1.0-beta.2`).
- A copy of `%LOCALAPPDATA%\CodexBar4Windows\logs\codexbar.log` (rolling daily file).
- Your Windows build number (`winver`).
- Steps to reproduce.

For P0 issues (data loss, crash on launch) escalate per [SUPPORT.md](SUPPORT.md).

## Beta retirement

When a stable cuts, the matching beta tags are kept on GitHub for archaeology but the beta manifest URL is pointed at the new stable. Users on the beta channel auto-update to the stable build on next check.

## FAQ

**Q: How often do betas cut?**
A: 2-week cadence aligned with the stable release rhythm. Typically 2-3 betas per stable.

**Q: Can I run both stable and beta at the same time?**
A: No — both install to the same path (`%LOCALAPPDATA%\Programs\CodexBar4Windows`). For multi-version testing, use the [portable build](README.md#portable) for the second version.

**Q: Does the beta share settings with the stable?**
A: Yes. Both read `%APPDATA%\CodexBar4Windows\config.json`. Switching channels never wipes your provider sign-ins.
