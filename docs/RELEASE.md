---
summary: "Release runbook for CodexBar4Windows. Covers minisign keypair, Authenticode cert, GitHub secrets, version bump, and tag push."
read_when:
  - Cutting a real v1.x release
  - Onboarding a new maintainer to the release pipeline
  - Diagnosing why a release-workflow run failed
---

# CodexBar4Windows release runbook

This is the operator-facing checklist for shipping a tagged release of CodexBar4Windows. The infrastructure (Inno installer, signtool wrapper, minisign manifest, GitHub Actions release workflow, Winget manifest seeds) all shipped in Phase 9; this document covers the human-gated steps that the workflow itself can't perform.

Read this top-to-bottom the first time you ship from a fresh checkout. After that, only the **"Cut a release"** section at the bottom is the regular per-release routine.

---

## One-time setup

These steps run **once per maintainer machine** (minisign) or **once per repo lifetime** (Authenticode cert, GitHub secrets). They do not repeat for every release.

### 1. Generate the minisign keypair (~30 s)

The Tauri updater plugin verifies every downloaded `latest.json` manifest against a baked-in minisign public key. Without a real keypair the updater is locked out by the runtime guard in `apps/desktop-tauri/src-tauri/src/updater_commands.rs`.

```powershell
# From the repo root. Requires `minisign` on PATH:
#   winget install jedisct1.minisign
pwsh scripts/generate-minisign-keypair.ps1 -Apply
```

The `-Apply` flag patches `apps/desktop-tauri/src-tauri/tauri.conf.json` in place, replacing the `REPLACE_WITH_BASE64_MINISIGN_PUBLIC_KEY` placeholder with the generated base64 public key. The private key lands in `.minisign/codexbar-updater.key` (gitignored).

You will be prompted for a password — pick a strong one and store it where you'd store any other production credential. The CI workflow re-uses both the key file and the password (see step 3).

**Commit the patched `tauri.conf.json` after the script finishes** — that's how the runtime knows the placeholder has been replaced.

### 2. Obtain an Authenticode signing certificate

Windows shows a SmartScreen warning the first time a user runs an unsigned `.exe`. Signing the installer with a real Authenticode certificate eliminates the warning for OV certs (after building reputation, ~10–50 installs over weeks) and immediately for EV certs.

**Choose a CA**: Sectigo, DigiCert, SSL.com, GlobalSign. EV is more expensive (~$300/year) but skips the reputation-building period. OV is cheaper (~$80/year) but the first few hundred users will see the SmartScreen warning until the reputation accumulates.

**After issuance**:

1. Export the cert as a password-protected `.pfx`.
2. Base64-encode the PFX:
   ```powershell
   $bytes = [IO.File]::ReadAllBytes("path\to\codexbar.pfx")
   [Convert]::ToBase64String($bytes) | Out-File -NoNewline pfx-base64.txt
   ```
3. Hand the base64 blob + the password off to step 3 — never commit either to the repo.

### 3. Provision the GitHub Actions secrets

`.github/workflows/release.yml` consumes four required secrets and one optional one. Add them at **Settings → Secrets and variables → Actions → New repository secret**:

| Secret name | Value | Required |
|---|---|---|
| `WINDOWS_SIGNING_CERT_PFX_BASE64` | The base64 blob from step 2.2 | ✅ |
| `WINDOWS_SIGNING_CERT_PASSWORD` | The PFX password | ✅ |
| `TAURI_MINISIGN_PRIVATE_KEY_PATH` | The full text content of `.minisign/codexbar-updater.key` (the file, not a path — GitHub Actions writes it back into the runner's filesystem at job-start) | ✅ |
| `TAURI_MINISIGN_PASSWORD` | The password you typed in step 1 | ✅ |
| `WINGET_GITHUB_TOKEN` | A PAT with `public_repo` scope on a fork of `microsoft/winget-pkgs`. Lets the workflow auto-PR the manifest update | optional |

**Sanity check after adding**: open the most recent Actions run on `main`, click "Re-run all jobs", and confirm the workflow shows the secret-gated steps as `skipped` (when on a fork without secrets) or as `running` (on the canonical repo).

---

## Per-release routine

Every release after the one-time setup follows these steps. They take ~5 minutes once you're in the rhythm.

### 4. Pre-flight: confirm main is shippable

```powershell
# All four gates must be green before you tag.
cargo test --workspace --quiet
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cd apps\desktop-tauri
npm run typecheck
npm test
npm run build
cd ..\..
cargo build --release --workspace
```

A clean run produces three EXEs under `target/release/` and the React bundle under `apps/desktop-tauri/dist/`. If any gate fails, **stop** — the release workflow will hit the same failure and waste a tag.

### 5. Bump the version (~1 min)

Versions live in nine places. The cleanest way to bump is via the dedicated commit message — `git grep -l 'version = "<old>"' rust apps packaging` finds every site.

The places (kept in sync with commit `32af713f`):

```
rust/Cargo.toml
rust/crates/codexbar4windows-claude-watchdog/Cargo.toml
rust/crates/codexbar4windows-claude-webprobe/Cargo.toml
apps/desktop-tauri/src-tauri/Cargo.toml
apps/desktop-tauri/src-tauri/tauri.conf.json
apps/desktop-tauri/package.json
apps/desktop-tauri/package-lock.json   (two occurrences)
packaging/winget/CodexBar4Windows.CodexBar4Windows.installer.yaml
packaging/winget/CodexBar4Windows.CodexBar4Windows.locale.en-US.yaml
packaging/winget/CodexBar4Windows.CodexBar4Windows.yaml
```

Then add a new top section to `CHANGELOG.md` with the release date and a short bullet list of changes since the prior tag.

Commit with the conventional message:

```
chore(release): bump <old> -> <new>
```

### 6. Push the tag

```bash
git tag -a v1.0.0 -m "CodexBar4Windows 1.0.0"
git push origin v1.0.0
```

The tag triggers `.github/workflows/release.yml` which:

1. Runs `cargo build --release` for every workspace crate.
2. Calls `scripts/sign-binaries.ps1` to Authenticode-sign the inner EXEs.
3. Calls `iscc.exe installer/codexbar.iss` to build the Inno installer; Inno's `SignTool=` directive signs the resulting `.exe` at the end of compile.
4. Calls `scripts/build-portable.ps1` to produce the portable ZIP.
5. Calls `scripts/generate-checksums.ps1` to write the sha256sum file.
6. Calls `scripts/sign-update-manifest.ps1` to sign the installer with minisign and produce `latest.json` (stable) / `beta.json` (beta channel).
7. Creates a GitHub Release with all five artifacts attached.
8. (Optional, when `WINGET_GITHUB_TOKEN` is set) Calls `wingetcreate update` against `microsoft/winget-pkgs` to PR the new manifest.

Watch the run at `https://github.com/JRub19/CodexBar4Windows/actions`. A green run takes ~10 minutes; a failure rolls back to a draft release you can delete cleanly.

### 7. Post-release smoke test

After the workflow lands:

1. Download the installer from the Release page.
2. Right-click → Properties → **Unblock** (the Mark-of-the-Web removes the SmartScreen prompt during install).
3. Install + launch. The tray icon should appear within 3 seconds.
4. Open the popup; the version readout in About should match the tag.
5. Open Preferences → Cost & Storage. The first scan should populate within ~2 seconds (instant if the user has no provider data on disk).

If anything's off, file an issue against the milestone and patch in a `v1.0.1` follow-up — never edit a published tag.

---

## SmartScreen reputation seeding (post-release)

For OV certificates only. EV certs skip this section.

The first few hundred installs of a newly-signed binary trigger SmartScreen's "Windows protected your PC" prompt because Microsoft hasn't yet collected enough reputation data. The warning fades automatically as users click "Run anyway" and the binary executes cleanly.

To accelerate seeding:

1. Submit the binary directly to [Microsoft Security Intelligence](https://www.microsoft.com/en-us/wdsi/filesubmission) after every release. Mark it as **Not malicious**. They typically respond within 48 hours and add the binary to the reputation database.
2. Keep the signing cert stable across releases (same Subject + Issuer). Reputation is keyed off the cert, not the binary.
3. Avoid changing the bundle identifier (`com.codexbar4windows.app`) — the OS treats a new identifier as a fresh app with no reputation.

Typical timeline: SmartScreen warning gone for OV certs after ~50 unique installs over ~2 weeks.

---

## Winget manifest screenshots

The Winget manifest seed under `packaging/winget/` references `https://github.com/JRub19/CodexBar4Windows` for screenshots. Winget submission reviewers prefer real screenshots of the running app. To capture them:

1. Build + run the app locally (`npm run tauri dev`).
2. Trigger every UI surface you want to ship (popup, settings panes, onboarding wizard).
3. Use `Win+Shift+S` to capture each surface as a `.png`.
4. Commit them under `docs/screenshots/` and update `packaging/winget/CodexBar4Windows.CodexBar4Windows.locale.en-US.yaml` with the raw GitHub URLs.

A future iteration will land a `scripts/capture-winget-screenshots.ps1` that automates this via the `tauri-driver` test harness — for now it's manual.

---

## Rollback

If a published release turns out to be broken:

1. **Don't delete the tag.** Users who already downloaded it will see a 404 on the updater check.
2. Tag a `v1.0.1` (or `v1.0.0-hotfix.1`) immediately with the fix.
3. Mark the bad release as **pre-release** in the GitHub UI so the updater skips it.
4. The `tauri-plugin-updater` checks the `latest.json` `version` field, not the GitHub Release status — so also re-run `scripts/sign-update-manifest.ps1` with the previous-good version pointed at the new artifact, or build a hotfix from `v1.0.0-1`'s commit.

---

## Reference

- `docs/windows/plan/phase-9-release.md` — the original Phase 9 plan, including rationale for every decision (per-user vs system install, Inno vs MSI, channel-aware manifests).
- `docs/windows/spec/90-cli-widgets-build.md` — the build + packaging contract.
- `scripts/sign-binaries.ps1` — inline doc comments on flags + auto-discovery.
- `scripts/sign-update-manifest.ps1` — inline doc comments on the manifest format.
- `.github/workflows/release.yml` — every step of the release pipeline.
