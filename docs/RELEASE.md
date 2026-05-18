---
summary: "Release runbook for CodexBar4Windows stable and beta builds."
read_when:
  - Cutting a GitHub release
  - Configuring signing or updater credentials
  - Diagnosing release workflow failures
---

# CodexBar4Windows Release Runbook

This runbook is the source of truth for shipping v1.0.3 and later. Stable
releases are blocked unless the Tauri minisign updater path is configured.
Authenticode signing is recommended, but not required for v1.0.3.

## One-Time Setup

### 1. Generate the Tauri updater minisign keypair

```powershell
# Requires minisign on PATH.
pwsh scripts/generate-minisign-keypair.ps1 -Apply
```

The `-Apply` flag patches `apps/desktop-tauri/src-tauri/tauri.conf.json` by
replacing `REPLACE_WITH_BASE64_MINISIGN_PUBLIC_KEY` with the generated public
key. Commit that public-key change. The private key is written under
`.minisign/` and is gitignored.

Store the private key text in the GitHub Actions secret
`TAURI_MINISIGN_PRIVATE_KEY`, and store the key password in
`TAURI_MINISIGN_PASSWORD`.

### 2. Obtain an Authenticode certificate

Export the certificate as a password-protected `.pfx`, then base64-encode it:

```powershell
$bytes = [IO.File]::ReadAllBytes("path\to\codexbar.pfx")
[Convert]::ToBase64String($bytes) | Out-File -NoNewline pfx-base64.txt
```

### 3. Provision GitHub Actions secrets

| Secret | Required for stable | Purpose |
|---|---:|---|
| `CODESIGN_PFX_BASE64` | no | Base64-encoded Authenticode PFX. When absent, Windows artifacts are published unsigned. |
| `CODESIGN_PFX_PASSWORD` | no | Password for the PFX. |
| `CODESIGN_THUMBPRINT` | no | Certificate thumbprint used by `scripts/sign-binaries.ps1`. |
| `TAURI_MINISIGN_PRIVATE_KEY` | yes | Full text of the minisign private key file. |
| `TAURI_MINISIGN_PASSWORD` | yes | Password for the minisign private key. |
| `WINGET_PAT` | no | PAT for automatic Winget manifest PRs. |

The release workflow fails stable tags before publishing when any required
updater secret is missing or when `tauri.conf.json` still contains the
placeholder updater pubkey. If Authenticode secrets are absent, the workflow
publishes unsigned Windows artifacts.

## Per-Release Routine

### 1. Preflight

Run the local gates from the repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace -- -D warnings -D clippy::unwrap_used
cargo clippy --workspace --tests -- -D warnings
cargo test --workspace --all-features
cd apps\desktop-tauri
npm ci
npm run typecheck
npm test
npm run build
cd ..\..
```

For a release candidate, also run:

```powershell
cd apps\desktop-tauri
npm run tauri build
cd ..\..
pwsh scripts/build-portable.ps1 -Version 1.0.3
pwsh scripts/generate-checksums.ps1 -Version 1.0.3
```

### 2. Version bump

Keep these surfaces in sync:

- `version.env`
- `rust/Cargo.toml`
- helper crate `Cargo.toml` files under `rust/crates/`
- `apps/desktop-tauri/src-tauri/Cargo.toml`
- `apps/desktop-tauri/src-tauri/tauri.conf.json`
- `apps/desktop-tauri/src-tauri/app.manifest`
- `apps/desktop-tauri/package.json`
- `apps/desktop-tauri/package-lock.json`
- `Cargo.lock`
- `packaging/winget/*.yaml`
- `CHANGELOG.md`

### 3. Tag

```powershell
git tag -a v1.0.3 -m "CodexBar4Windows 1.0.3"
git push origin v1.0.3
```

The tag-triggered workflow builds the desktop app, signs inner binaries,
packages the installer and portable ZIP, generates checksums, signs the updater
manifest, verifies signatures, publishes the GitHub Release, and optionally
submits a Winget update.

## Post-Release Smoke Test

After the release workflow is green:

1. Download and install `CodexBar4Windows-1.0.3-x64.exe`.
2. Confirm the tray icon appears within a few seconds.
3. Open the popup and Preferences -> About; the version must read `1.0.3`.
4. Click Check for updates. A stable build must not report the placeholder-key
   disabled message.
5. Open Preferences -> Cost & Storage and confirm the first scan completes.
6. Hover a Claude or Codex cost row and confirm the cost popover opens.
7. Download the portable ZIP, extract it, run the EXE, and confirm config writes
   beside the portable build.

## Rollback

Do not delete published tags. If a stable release is broken, tag a patch release
from the fixed commit and mark the broken GitHub Release as a prerelease. If the
updater manifest points at a bad build, regenerate and republish the manifest for
the fixed version.
