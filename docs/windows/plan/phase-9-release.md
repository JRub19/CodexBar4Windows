---
phase: 9
title: "Phase 9, Polish, Packaging, and Release v1.0"
status: "Planned"
audience: "Rust/TS engineer driving the v1.0 GA of CodexBar4Windows."
budget: "3 calendar weeks for one engineer at ~50% allocation, doubled to 6 if Rust/Tauri is unfamiliar."
prerequisites:
  - "Phase 0 (repo reset)"
  - "Phase 1 (re-grounding)"
  - "Phase 2 (parity audit)"
  - "Phase 3 (Windows polish)"
  - "Phase 4 (packaging scaffolding)"
  - "Phase 5 (Tauri updater wiring)"
  - "Phase 6 (locale infrastructure)"
  - "Phase 7 (preferences About pane channel toggle)"
  - "Phase 8 (telemetry opt-in plumbing)"
next_phase: "Phase 10, post-launch parity for the long-tail providers (tracked separately)."
references:
  - "docs/windows/spec/80-feel-and-polish.md (the 64-item polish checklist)"
  - "docs/windows/spec/90-cli-widgets-build.md (build, packaging, updater contract)"
  - "docs/windows/06-roadmap.md (older roadmap phases 4 through 6)"
  - "docs/windows/07-risks-and-open-questions.md (SmartScreen, signing)"
  - "docs/windows/05-windows-ux-spec.md (Windows-feel acceptance bar)"
---

# Phase 9, Polish, Packaging, and Release v1.0

## Why this phase exists

Everything before Phase 9 made CodexBar4Windows correct, complete, and Windows-native
at the feature level. Phase 9 turns the working prototype into something a stranger on
the public internet can install in 60 seconds without a support call. The phase is
bounded by a single objective: a user runs `winget install CodexBar.CodexBar` on a
fresh Windows 11 machine, the tray icon appears within a second, the popup is
keyboard, screen reader, and high-contrast friendly, providers can be enabled, an
opt-in update channel is selectable, and a future release flows through the Tauri
updater into a silent in-place upgrade.

Phase 9 has nine sub-areas. They are largely independent, so the work parallelises if
staffing allows. With one engineer at 50% allocation the path is roughly serial.
Performance work and the polish checklist together account for most of the calendar
time. Distribution and auto-update are short in code but long in iteration because
every release requires a real GitHub Releases artifact, a real signature, and a real
installer run.

The phase ends with a `v1.0.0` tag, a Winget manifest pull request, a CHANGELOG entry,
a README rewrite, and an upstream pull request to `steipete/CodexBar` that points at
this fork from the "Looking for a Windows version?" section.

## Dependencies and assumptions

At the start of week 1 the following are already in place.

1. The Rust workspace builds clean on Windows 11 with `cargo build --release` and
   `npm run tauri build` from a fresh checkout.
2. CI on `windows-2022` runs `cargo fmt --check`, `cargo clippy -D warnings`,
   `cargo test --workspace`, `pnpm lint`, and `pnpm test` green on `main`.
3. The Inno Setup script `installer/codexbar.iss` exists in skeleton form from Phase 4.
4. The Tauri updater plugin is wired in from Phase 5; no production manifest signed yet.
5. The Preferences About pane has a channel selector (Stable, Beta) from Phase 8 that
   writes to `config.json` but is not yet wired at runtime.
6. The locale loader in `src/i18n/index.ts` resolves `en`, `pt-BR`, and `zh-Hans` from
   `src/locales/<lang>/translation.json` files seeded by the Phase 6 import script.
7. A code-signing certificate has been procured (EV preferred, OV acceptable). If the
   cert is not in hand by start of phase, see the Risks section for the contingency.

The phase does not assume MSIX packaging, Microsoft Store publishing, or ARM64 builds.
Those are explicit non-goals for v1.0.

## Deliverables by sub-area

### A. Performance pass

Goal: idle and active resource budgets are met on the minimum-spec machine (4-core,
8 GB RAM, SATA SSD, Windows 11 23H2 stable).

Budgets:

1. Idle resident set size below 70 MB after 60 seconds of inactivity.
2. Cold start from process launch to tray icon visible below 800 ms on a SATA SSD.
3. Click on the tray icon to popup first frame below 100 ms.
4. Per-refresh CPU spike below 2 percent sustained for under 500 ms on 4 cores.
5. Zero disk I/O at idle when refresh cadence is set to Manual, for at least 30 seconds
   after launch.
6. Profiled with the Windows Performance Toolkit. Regressions versus budget either fix
   in phase or file with workaround for Phase 10.

The baseline profiling workload runs three providers (Claude, Codex, Copilot) at 60
second cadence, popup closed, system idle. A second pass opens the popup, switches
tabs three times, runs a manual refresh on Claude, dismisses, and idles 30 seconds.
Traces live at `docs/windows/perf/phase-9-baseline-<date>.etl` and are compared
against a release-candidate trace at end of phase.

Watch points: the WebView2 process dominates idle RSS and must be suspended whenever
the popup is hidden. The Rust core allocates from `reqwest`, `rustls`, `tokio`, and
the provider plugin set. The `Shell_NotifyIcon` redraw cost multiplies the 30 Hz
loading cadence; any redraw outside of an active animation is a bug.

Acceptance:

1. `Get-Process CodexBar | Select WorkingSet64` after 60 seconds idle is under 73.4 MB.
2. Cold-start probe returns under 800 ms median across 10 SATA SSD runs.
3. Popup-open `console.time` boundary under 100 ms across 10 manual clicks.
4. WPT trace shows zero file I/O for 30 seconds after launch in Manual cadence mode.
5. Baseline and release-candidate traces committed to `docs/windows/perf/`.

### B. Polish pass

Goal: every item in `docs/windows/spec/80-feel-and-polish.md` section 20 (64 items) is
either implemented or filed as a documented deviation. The reviewer walks each item,
opens the relevant source file, confirms the implementation, and ticks
`docs/windows/perf/polish-checklist-status.md`. Items that miss get an issue and a
follow-up PR.

Items needing explicit attention this phase:

1. No white flash on launch. The tray icon must be drawn synchronously before
   `Shell_NotifyIcon NIM_ADD`. Verified with a 60 fps screen capture of cold launch.
2. Hover delay 80 ms for the explanation icon tooltip, 800 ms for OS-style tooltips,
   0 ms for hover state changes. The 80 ms value is the Phantom-grade detail.
3. Ctrl+R while the popup is open triggers a refresh of the focused provider without
   closing the popup. Mirrors the macOS Cmd+R behaviour inside the system menu.
4. In-popup confetti renders if the popup is open at celebration time: a 1.2 second
   canvas-confetti burst anchored to the provider card header.
5. Celebration only fires when at least 1 percent utilization happened in the past 24
   hours, filtered on the `UsageStore` plan utilization snapshot.

Voice and microcopy rules from spec 80 section 19, with the Windows-specific deltas:

1. The tagline is preserved verbatim, "May your tokens never run out, keep agent
   limits in view." The comma replaces the upstream em dash to honour the local no-em-
   dash policy. Only the tagline is allowed to be a warm string.
2. No exclamation marks anywhere outside the tagline. Lint the English bundle and
   fail the build on `!`.
3. Use `·` (U+00B7) with thin spaces (U+2009) for the dot separator.
4. Use `≈` (U+2248) for approximations, e.g. "≈ 35% run-out risk."
5. Lowercase compact units: `1d 4h`, `3h 12m`. No `hr`, `hrs`, `min`, `mins`.

Acceptance:

1. The checklist status file shows 64/64 items closed or deviated with a follow-up issue.
2. The cold-launch capture shows no white flash, no main-window flash, no flickering
   taskbar entry.
3. A grep over `src/locales/en/translation.json` returns zero `!` and zero em dashes.
4. The in-popup confetti renders correctly when a manual celebration is triggered
   from the debug menu with the popup open.

### C. Accessibility

Goal: the app is usable with keyboard only, with Narrator, and in High Contrast mode.

Keyboard navigation. The popup is reachable via the user-configurable global hotkey
(default `Win+Shift+U`) or via `Win+B` then arrow keys to the CodexBar icon then
`Enter`. Once open, the focus order matches the visual top-to-bottom order from spec
80 section 16:

1. App icon header, focusable, `Enter` opens dashboard URL.
2. Provider tabs, `Left`/`Right`/`Home`/`End` cycle, `Enter` selects.
3. Provider cards: bars, reset times, pace, details.
4. Status pill (if an incident is visible).
5. Footer: Refresh now, Preferences, Quit.

`Tab` walks forward, `Shift+Tab` walks back. `Esc` always dismisses. No focus traps.

Focus rings. Every interactive element gets a `:focus-visible` outline at 2 px accent
color (sourced from `UISettings::GetColorValue(UIColorType::Accent)`), 2 px offset,
6 px corner radius. Use `outline`, not `box-shadow`, so NVDA and Narrator detect it.

Narrator labels. Spec 80 section 16 lists the per-element ARIA mapping. The phase
confirms each by running Narrator over the popup and recording a transcript at
`docs/windows/perf/narrator-walkthrough.md`.

Reduce motion. When `UISettings::AnimationsEnabled` is false or WebView2 sees
`prefers-reduced-motion: reduce`, suppress: popup open/close transition, tray loading
animation (fall back to the static ellipsis hint), critter blink/wiggle/tilt, tab
slide, bar tween, pace text crossfade, canvas confetti. Keep: pressed scale,
copy-flash. The kept items are sub-100 ms essential feedback.

High contrast. Detect via CSS `@media (forced-colors: active)` in WebView2 and
`SystemParametersInfo(SPI_GETHIGHCONTRAST)` in the Rust shell. Strip Mica/Acrylic for
a flat `Canvas`/`CanvasText`/`Highlight` surface, give bars 2 px borders with solid
fill, suppress critter visuals, switch focus rings to `Highlight`.

Acceptance:

1. Keyboard-only walk of the popup matches the spec; recorded as a video.
2. Narrator transcript at `docs/windows/perf/narrator-walkthrough.md` covers every
   surface with expected roles and values.
3. With reduced motion on, popup transitions are instantaneous; pressed scale and
   copy-flash remain.
4. With High Contrast Black selected, every readable surface meets 7:1 contrast
   measured by the Inspect tool.

### D. Localization

Goal: three locales ship in v1.0: `en`, `pt-BR`, `zh-Hans`, matching the upstream
macOS set after commit `22c44848`.

Stack. The React UI uses `i18next` with JSON resource bundles. The Rust core uses
`fluent-rs` with `.ftl` files at `rust/locales/<lang>/codexbar.ftl` for strings that
originate in the core (errors, toast bodies, tray-icon tooltip text).

Selection rule:

1. If `config.appLanguage` is set, use it.
2. Otherwise take the first locale from `GetUserPreferredUILanguages` that is in the
   bundled set, normalized to lowercase BCP-47 (`pt-br`, `zh-hans`).
3. Otherwise fall back to `en`.

Per-locale, every string in the popup card stack, the Preferences pane (every tab and
help text), the first-run onboarding card and welcome toast, the four notification
surfaces (quota warning, session depleted, session restored, weekly reset), and the
tray multi-line tooltip must render in the locale with no English fallthrough.

Validation is automated. `tests/locale_coverage.rs` loads each locale, walks the
required-keys fixture, and asserts that every required key has a value. Missing keys
fail the test with locale and key names.

Translation sourcing:

1. The macOS shipping translations from `Sources/CodexBar/Resources/{pt-BR,zh-Hans}.lproj/`
   are imported via the strings-to-json script.
2. Re-run the import as the final locale commit before tagging GA to capture late
   upstream changes.
3. Translate Windows-only strings by hand (first-run toast, Windows-specific tooltips,
   the in-popup celebration banner, fewer than 20 strings per locale).
4. A native speaker reviews the Windows-only strings before tagging.

Acceptance:

1. The locale-coverage integration test passes for all three locales.
2. A manual walk of the popup, Preferences, and notifications in `pt-BR` surfaces no
   English. Repeat in `zh-Hans` with no broken glyphs (font fallback correct).
3. Switching `appLanguage` at runtime updates every visible string without a relaunch.

### E. Packaging

Goal: two signed, checksummed artifact families per release: an Inno Setup `.exe`
installer and a portable ZIP.

Inno Setup. The script at `installer/codexbar.iss` is expanded from the Phase 4
skeleton. Skeleton outline below; the full file is approximately 200 lines.

```ini
; installer/codexbar.iss
#define MarketingVersion GetEnv("CODEXBAR_VERSION")
#define Arch             GetEnv("CODEXBAR_ARCH")
#define SourceBase       "..\target\release"
#define AssetsBase       "..\installer\assets"

[Setup]
AppId={{B7C2A6A0-8C1D-4C0D-9F0C-9C0D5F0A1234}}
AppName=CodexBar
AppVersion={#MarketingVersion}
AppPublisher=CodexBar4Windows
DefaultDirName={localappdata}\Programs\CodexBar
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesInstallIn64BitMode=x64
ArchitecturesAllowed=x64
OutputBaseFilename=CodexBar-{#MarketingVersion}-{#Arch}
SetupIconFile={#AssetsBase}\Icon.ico
WizardStyle=modern
Compression=lzma2/ultra
SolidCompression=yes
SignTool=signtool sign /tr http://timestamp.digicert.com /td SHA256 /fd SHA256 /a $f
SignedUninstaller=yes

[Languages]
Name: "en";     MessagesFile: "compiler:Default.isl"
Name: "ptBR";   MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "zhHans"; MessagesFile: "compiler:Languages\ChineseSimplified.isl"

[Files]
Source: "{#SourceBase}\CodexBar.exe";                  DestDir: "{app}"; Flags: ignoreversion sign
Source: "{#SourceBase}\codexbar.exe";                  DestDir: "{app}"; Flags: ignoreversion sign
Source: "{#SourceBase}\codexbar-claude-watchdog.exe";  DestDir: "{app}"; Flags: ignoreversion sign
Source: "{#AssetsBase}\Icon.ico";                      DestDir: "{app}"; Flags: ignoreversion

[Tasks]
Name: "addtopath";     Description: "{cm:AddToPath}";     Flags: checkedonce
Name: "launchatlogin"; Description: "{cm:LaunchAtLogin}"; Flags: checkedonce
Name: "desktopicon";   Description: "{cm:DesktopIcon}";   Flags: unchecked

[Registry]
Root: HKCU; Subkey: "Environment"; ValueType: expandsz; ValueName: "Path";
   ValueData: "{olddata};{app}"; Check: NeedsAddPath('{app}'); Tasks: addtopath
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run";
   ValueType: string; ValueName: "CodexBar";
   ValueData: """{app}\CodexBar.exe"" --minimized"; Flags: uninsdeletevalue; Tasks: launchatlogin
Root: HKCU; Subkey: "Software\Classes\codexbar"; ValueType: string;
   ValueName: ""; ValueData: "URL:CodexBar Protocol"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\codexbar"; ValueType: string;
   ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\codexbar\shell\open\command";
   ValueType: string; ValueName: "";
   ValueData: """{app}\CodexBar.exe"" ""--launch=%1"""

[Run]
Filename: "{tmp}\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install";
   Flags: skipifsilent; Check: NeedsWebView2

[Code]
function NeedsWebView2: Boolean;
var Value: String;
begin
   Result := not RegQueryStringValue(HKLM,
      'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
      'pv', Value);
end;

function NeedsAddPath(Param: String): Boolean;
var OrigPath: String;
begin
   if not RegQueryStringValue(HKCU, 'Environment', 'Path', OrigPath) then begin
      Result := True; exit;
   end;
   Result := Pos(';' + Param + ';', ';' + OrigPath + ';') = 0;
end;
```

The WebView2 evergreen bootstrap is downloaded to `{tmp}` via a `[Files]` `Download`
entry (omitted above for brevity). If WebView2 is already installed, `NeedsWebView2`
skips it. The `--launch=` argument carries the custom URI from a clicked toast. The
`--minimized` flag suppresses any startup window flash on auto-launch.

Portable build. Produced from the same `cargo build --release` output without an
installer wrapper. Contents:

```
CodexBar-<ver>-portable-x64\
  CodexBar.exe                     ; Tauri shell
  codexbar.exe                     ; CLI binary
  codexbar-claude-watchdog.exe     ; watchdog
  WebView2Loader.dll               ; static loader; runtime still must be on host
  Icon.ico
  portable.marker                  ; tells runtime to use sibling config dir
  README-PORTABLE.txt
```

The portable runtime writes config to a `portable-config\` directory adjacent to the
EXE instead of `%APPDATA%\CodexBar`. The discriminator is the `portable.marker` file.

Authenticode signing. Two stages in CI: every inner EXE signed before bundling, then
the installer signed by Inno's `SignTool=` directive at the end of compile.

```powershell
signtool sign `
  /tr "http://timestamp.digicert.com" /td SHA256 /fd SHA256 `
  /sha1 "$env:CODESIGN_THUMBPRINT" /v `
  "target\release\CodexBar.exe" `
  "target\release\codexbar.exe" `
  "target\release\codexbar-claude-watchdog.exe"
```

The cert loads from a YubiKey HSM via the Microsoft Trusted Signing dispatcher on the
GitHub Actions runner; thumbprint comes from the `CODESIGN_THUMBPRINT` secret.

SHA-256 checksums. After signing, `scripts/generate-checksums.ps1` writes
`CodexBar-<ver>-checksums.txt` in standard `sha256sum` format:

```powershell
$artifacts = @(
  "CodexBar-$ver-x64.exe",
  "CodexBar-$ver-portable-x64.zip"
)
$artifacts | ForEach-Object {
   $h = (Get-FileHash -Algorithm SHA256 -Path $_).Hash.ToLower()
   "$h  $_"
} | Set-Content -Encoding utf8 "CodexBar-$ver-checksums.txt"
```

Acceptance:

1. `iscc.exe /Qp installer\codexbar.iss` compiles clean, no warnings.
2. Installer runs to completion without admin by default; per-machine path re-runs
   elevated and installs to `%ProgramFiles%\CodexBar`.
3. Installer downloads and runs WebView2 evergreen on a host that lacks the runtime.
4. Portable ZIP runs from any directory; config lands in `portable-config\`.
5. `signtool verify /pa /v` succeeds on all four signed artifacts.
6. Checksums file matches actual SHA-256 of uploaded artifacts.

### F. Auto-update

Goal: the installed app polls a signed `latest.json` manifest on launch and on demand,
downloads the new installer, verifies the minisign signature, runs the installer
silently, and restarts.

Manifest format (Tauri updater plugin convention):

```json
{
  "version": "1.0.0",
  "notes": "Initial public release of CodexBar4Windows. See CHANGELOG.md for details.",
  "pub_date": "2026-06-02T15:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "<base64 minisign signature over the installer bytes>",
      "url": "https://github.com/JRub19/CodexBar4Windows/releases/download/v1.0.0/CodexBar-1.0.0-x64.exe"
    }
  }
}
```

Two manifests:

1. `latest.json` for Stable, hosted at the GitHub Release `latest` download path.
2. `beta.json` for Beta, points at whichever of Beta or Stable is newest.

Manifest signing. Tauri minisign with an Ed25519 keypair. Public key baked into
`tauri.conf.json` at build time. Private key in a YubiKey or Azure Key Vault secret.
Sign step:

```powershell
minisign -S -s "$env:TAURI_MINISIGN_PRIVATE_KEY_PATH" `
  -m "CodexBar-$ver-x64.exe" `
  -x "CodexBar-$ver-x64.exe.sig"
$signatureB64 = [Convert]::ToBase64String([IO.File]::ReadAllBytes("CodexBar-$ver-x64.exe.sig"))
```

`$signatureB64` populates `platforms.windows-x86_64.signature` in the manifest.

Channel selection. The About-pane toggle (Phase 8) writes `updateChannel` to config.
At updater init the plugin reads it and selects the matching manifest URL.

Lifecycle:

1. On launch, fetch the channel manifest. If `version` exceeds the installed version,
   emit `update-available`.
2. The popup renders the update banner (spec 80 section 13.1) with Update now, Later,
   dismiss.
3. Update now downloads the installer, verifies the minisign signature, and runs it
   silently. The installer kills the running app, replaces the EXEs in
   `%LOCALAPPDATA%\Programs\CodexBar`, and restarts.
4. The manual "Check Now" button in the About pane runs the same check on demand.

Per-machine installs that lack elevation reprompt for UAC. If the user cancels, the
banner moves to a "retry on next launch" state.

Acceptance:

1. A staged release in a `codexbar-update-test` repo at v0.99.0 sees a manifest
   pointing to v1.0.0 and completes the update.
2. The minisign signature is verified before the installer runs; tampered bytes
   cause the updater to refuse the download.
3. The Preferences toggle between Stable and Beta changes the manifest URL on next
   check.
4. The update applies silently with no UAC prompt on a per-user install.
5. Post-update launch carries the user's config and locale forward.

### G. Distribution

Goal: a release tag pushed to `main` triggers a workflow that builds, signs, uploads,
generates notes, and submits a Winget update PR.

Release workflow outline at `.github/workflows/release.yml`. The full file is around
180 lines; the structure is:

```yaml
name: Release
on:
  push:
    tags: ["v*.*.*"]
permissions:
  contents: write
jobs:
  build:
    runs-on: windows-2022
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - name: Read version
        id: ver
        shell: pwsh
        run: |
          $tag = "${{ github.ref_name }}"
          $version = $tag.TrimStart("v")
          $channel = if ($version -match "-beta|-rc") { "beta" } else { "stable" }
          "version=$version" | Out-File -FilePath $env:GITHUB_OUTPUT -Append
          "channel=$channel" | Out-File -FilePath $env:GITHUB_OUTPUT -Append
      - uses: actions-rs/toolchain@v1
        with: { toolchain: stable, components: "rustfmt, clippy" }
      - uses: actions/setup-node@v4
        with: { node-version: "20", cache: "pnpm" }
      - run: npm i -g pnpm@9
      - run: cargo install tauri-cli --version "^2" --locked
      - run: choco install innosetup minisign -y
      - name: Restore signing cert
        env:
          CODESIGN_PFX_BASE64: ${{ secrets.CODESIGN_PFX_BASE64 }}
          CODESIGN_PFX_PASSWORD: ${{ secrets.CODESIGN_PFX_PASSWORD }}
        shell: pwsh
        run: |
          $bytes = [Convert]::FromBase64String($env:CODESIGN_PFX_BASE64)
          [IO.File]::WriteAllBytes("$env:RUNNER_TEMP\codesign.pfx", $bytes)
          certutil -f -p $env:CODESIGN_PFX_PASSWORD -importpfx "$env:RUNNER_TEMP\codesign.pfx"
      - name: Lint and test
        run: |
          cargo fmt --check
          cargo clippy --all-targets --all-features -- -D warnings
          pnpm install --frozen-lockfile
          pnpm lint; pnpm typecheck; pnpm test --run
          cargo test --workspace
      - name: Build
        env: { CODEXBAR_VERSION: "${{ steps.ver.outputs.version }}" }
        run: |
          pnpm build
          cargo tauri build --target x86_64-pc-windows-msvc
      - name: Sign inner binaries
        run: pwsh scripts/sign-binaries.ps1
      - name: Build installer
        env:
          CODEXBAR_VERSION: ${{ steps.ver.outputs.version }}
          CODEXBAR_ARCH: x64
        run: '& "C:\Program Files (x86)\Inno Setup 6\iscc.exe" /Qp installer\codexbar.iss'
      - name: Build portable ZIP
        env: { CODEXBAR_VERSION: "${{ steps.ver.outputs.version }}" }
        run: pwsh scripts/build-portable.ps1
      - name: Generate checksums
        env: { CODEXBAR_VERSION: "${{ steps.ver.outputs.version }}" }
        run: pwsh scripts/generate-checksums.ps1
      - name: Sign update manifest
        env:
          TAURI_MINISIGN_PRIVATE_KEY: ${{ secrets.TAURI_MINISIGN_PRIVATE_KEY }}
          TAURI_MINISIGN_PASSWORD: ${{ secrets.TAURI_MINISIGN_PASSWORD }}
        run: |
          cargo run --bin codexbar-xtask -- sign-manifest `
            --version "${{ steps.ver.outputs.version }}" `
            --channel "${{ steps.ver.outputs.channel }}" `
            --installer "dist\CodexBar-${{ steps.ver.outputs.version }}-x64.exe"
      - name: Extract release notes
        run: |
          cargo run --bin codexbar-xtask -- extract-notes `
            --version "${{ steps.ver.outputs.version }}" `
            --out "dist\RELEASE-NOTES.md"
      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          name: CodexBar ${{ steps.ver.outputs.version }}
          body_path: dist/RELEASE-NOTES.md
          prerelease: ${{ steps.ver.outputs.channel == 'beta' }}
          files: |
            dist/CodexBar-${{ steps.ver.outputs.version }}-x64.exe
            dist/CodexBar-${{ steps.ver.outputs.version }}-portable-x64.zip
            dist/CodexBar-${{ steps.ver.outputs.version }}-checksums.txt
            dist/latest.json
            dist/beta.json
      - name: Submit Winget update
        if: steps.ver.outputs.channel == 'stable'
        env: { WINGET_PAT: "${{ secrets.WINGET_PAT }}" }
        shell: pwsh
        run: |
          $ver = "${{ steps.ver.outputs.version }}"
          $url = "https://github.com/JRub19/CodexBar4Windows/releases/download/v$ver/CodexBar-$ver-x64.exe"
          wingetcreate update CodexBar.CodexBar --version $ver --urls $url --submit --token $env:WINGET_PAT
```

Winget manifest. Three files per version under
`microsoft/winget-pkgs/manifests/c/CodexBar/CodexBar/<ver>/`.

`CodexBar.CodexBar.installer.yaml`:

```yaml
PackageIdentifier: CodexBar.CodexBar
PackageVersion: 1.0.0
InstallerType: inno
Scope: user
Architectures: [x64]
Installers:
  - Architecture: x64
    InstallerUrl: https://github.com/JRub19/CodexBar4Windows/releases/download/v1.0.0/CodexBar-1.0.0-x64.exe
    InstallerSha256: <sha256>
    InstallerSwitches:
      Silent: /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
      SilentWithProgress: /SILENT /SUPPRESSMSGBOXES /NORESTART
    ProductCode: CodexBar_is1
    ReleaseDate: 2026-06-02
ManifestType: installer
ManifestVersion: 1.6.0
```

`CodexBar.CodexBar.locale.en-US.yaml`:

```yaml
PackageIdentifier: CodexBar.CodexBar
PackageVersion: 1.0.0
PackageLocale: en-US
Publisher: CodexBar4Windows
PublisherUrl: https://github.com/JRub19/CodexBar4Windows
PublisherSupportUrl: https://github.com/JRub19/CodexBar4Windows/issues
PackageName: CodexBar
License: MIT
LicenseUrl: https://github.com/JRub19/CodexBar4Windows/blob/main/LICENSE
ShortDescription: Tray app that surfaces agent quota and usage for Claude, Codex, and 30 other providers.
Moniker: codexbar
Tags: [ai, agents, claude, codex, copilot, cursor, quota, tokens, tray]
ReleaseNotesUrl: https://github.com/JRub19/CodexBar4Windows/releases/tag/v1.0.0
ManifestType: defaultLocale
ManifestVersion: 1.6.0
```

`CodexBar.CodexBar.yaml`:

```yaml
PackageIdentifier: CodexBar.CodexBar
PackageVersion: 1.0.0
DefaultLocale: en-US
ManifestType: version
ManifestVersion: 1.6.0
```

First submission is manual via `wingetcreate new`. Subsequent versions are the
automated `wingetcreate update` step in the workflow.

CHANGELOG. Keep a Changelog format. The v1.0.0 entry:

```markdown
## [1.0.0] - 2026-06-02

### Added
- Initial public release of CodexBar4Windows.
- Tray icon with dynamic bar rendering, six loading patterns, quota-warning flash,
  reset celebration.
- Popup card UI with provider tabs, pace text, copy-to-clipboard affordances.
- Provider support: Claude, Codex, Copilot, Cursor, Gemini, plus 25 more.
- Auto-update via Tauri updater with Stable and Beta channels.
- Localized for English, Brazilian Portuguese, Simplified Chinese.
- Keyboard navigation, Narrator support, High Contrast theme.
- Reduce-motion fallback honoured.
- Inno Setup installer with WebView2 evergreen bootstrap.
- Portable ZIP distribution.
- Authenticode-signed binaries.
- SHA-256 checksums published per release.
- Winget package published as CodexBar.CodexBar.

### Known limitations
- ARM64 build deferred to a later release.
- MSIX distribution deferred to a later release.
- Chrome cookies on Chrome 127+ require manual cookie paste; Edge and Firefox work.

[1.0.0]: https://github.com/JRub19/CodexBar4Windows/releases/tag/v1.0.0
```

README. Rewritten for v1.0: short "What is CodexBar4Windows" intro, animated 60
second demo gif at the top, install instructions for winget and Inno, a screenshot
grid (tray, popup, preferences, celebration), a Known Limitations callout for the
Chrome v127+ story, and a Contributors section.

Acceptance:

1. A tag push of `v0.99.0-rc1` to the test repo runs the workflow end to end in under
   30 minutes.
2. The release surfaces all four artifact files plus the manifest on the GitHub
   Release page.
3. `wingetcreate update` opens a PR against `microsoft/winget-pkgs` and validates.
4. `winget install CodexBar.CodexBar` works on a fresh Windows 11 24H2 host.
5. CHANGELOG.md and README.md reflect the v1.0.0 entry.

### H. Beta program

Goal: a two-week supervised beta before GA. Beta users opt in to error reports, file
issues against templates, and get a clear triage SLA.

Invite. `BETA.md` in the repo root with the install link to the Beta manifest URL
and instructions on toggling the channel inside the app. Beta is public; no
credentials. Two-week calendar window: tag `v1.0.0-beta.1` on day 0, tag `v1.0.0` on
day 14, no `-beta` tags between except bugfix iterations.

Opt-in error reporting. `sentry-rust` for the Rust core, `@sentry/browser` for the
React UI. Both gated on a single Preferences toggle, default OFF. Errors only, no
usage analytics, no breadcrumbs with user data. The `before_send` PII filter strips
home-directory paths, every email-shaped string, every cookie-shaped string, and the
entire `auth` object family. The DSN is baked into `tauri.conf.json` at build time.

Issue templates. Two YAML forms under `.github/ISSUE_TEMPLATE/`:

```yaml
# bug-report.yml
name: Bug report
description: Something is broken on Windows.
title: "[bug] <short description>"
labels: ["bug", "windows-only"]
body:
  - type: input
    id: version
    attributes:
      label: CodexBar version
      placeholder: 1.0.0
    validations: { required: true }
  - type: input
    id: windows
    attributes:
      label: Windows version
      placeholder: Windows 11 24H2 (build 26100.1234)
    validations: { required: true }
  - type: dropdown
    id: install_type
    attributes:
      label: Install type
      options: [winget, "Inno installer (per-user)", "Inno installer (per-machine)", "portable ZIP"]
  - type: textarea
    id: reproduction
    attributes: { label: Steps to reproduce }
    validations: { required: true }
  - type: textarea
    id: expected
    attributes: { label: What did you expect }
  - type: textarea
    id: actual
    attributes: { label: What happened }
```

```yaml
# feature-request.yml
name: Feature request
description: Suggest a new provider, setting, or behaviour.
title: "[feature] <short description>"
labels: ["enhancement", "windows-only"]
body:
  - type: textarea
    id: problem
    attributes: { label: What problem are you solving }
  - type: textarea
    id: idea
    attributes: { label: What is the proposed change }
```

Triage SLA at `docs/SUPPORT.md`:

1. P0 (crash, data loss, security): ack within 24 hours, fix within 7 days.
2. P1 (broken feature, no workaround): ack within 72 hours, fix within 14 days or
   schedule with a workaround.
3. P2 (broken feature with workaround, polish, performance): ack within 7 days, no
   fix commitment.
4. P3 (feature request): ack within 14 days, may be closed with a note.

Acceptance:

1. BETA.md exists and the Beta channel installs from it.
2. `sentry-rust` is wired with the opt-in toggle, default OFF; a manual crash posts
   to Sentry only after the user enables it.
3. The PII filter strips home-directory paths, emails, and cookies from at least the
   `tests/sentry_pii.rs` fixture.
4. Both issue templates render in the GitHub new-issue picker.
5. `docs/SUPPORT.md` is published and linked from the README.

### I. GA cutover

Goal: take the Beta-validated build, ship it as v1.0.0, and announce.

Sequence:

1. Confirm zero open P0 or P1 issues from the Beta run.
2. Tag `v1.0.0` on `main`. The release workflow fires.
3. Manually inspect the release artifacts. Run the installer on a clean Windows 11
   sandbox VM. Verify the tray icon and a refresh on Claude or Codex.
4. Approve the Winget PR submitted by the workflow. Once merged upstream,
   `winget search CodexBar` returns the new version within an hour.
5. Update README.md with an Announcing v1.0 line at the top plus the demo gif.
6. Open an upstream PR against `steipete/CodexBar` README to add this fork to the
   "Looking for a Windows version?" section. PR body:
   > Adds CodexBar4Windows to the Windows version listing. CodexBar4Windows is a
   > Tauri 2 + React port that ships the Tier 1 provider set, an Authenticode-signed
   > installer, and the Tauri updater. License MIT, attribution preserved.
7. Post the announcement to the project README. Match the dry technical voice.

Acceptance:

1. The `v1.0.0` tag exists on `main` and the GitHub Release is published.
2. The Winget package surfaces in `winget search CodexBar` after upstream merge.
3. The upstream PR is open against `steipete/CodexBar`.
4. README.md and CHANGELOG.md are updated and present on `main`.

## Atomic-commit task list

Each task is one commit. Conventional Commits format. Files, acceptance, draft message.

### 1, perf: ETW baseline doc

Files: `docs/windows/perf/phase-9-baseline-template.md`, `docs/windows/perf/.gitkeep`.
Acceptance: doc lists six budgets, workload model, and profiling tools.
Message: `perf(windows): document phase 9 performance baseline workload and budgets`.

### 2, perf: suspend WebView2 on popup close

Files: `rust/src/popup/lifecycle.rs`, `rust/src/popup/webview.rs`,
`tests/popup_lifecycle.rs`.
Acceptance: with popup closed, `msedgewebview2` CPU at zero and RSS shrinks within
5 seconds.
Message: `perf(popup): suspend the WebView2 process when the popup is hidden`.

### 3, perf: gate disk IO on Manual cadence

Files: `rust/src/refresh/scheduler.rs`, `tests/refresh_scheduler.rs`.
Acceptance: in Manual cadence, no filesystem writes for 30 seconds after launch
(Process Monitor filter).
Message: `perf(refresh): skip cache writes when cadence is set to Manual`.

### 4, polish: no white flash on cold launch

Files: `rust/src/tray/init.rs`, `rust/src/render/icon.rs`.
Acceptance: 60 fps cold-launch capture shows no white frame between NIM_ADD and the
first painted bitmap.
Message: `fix(tray): render the initial icon synchronously before NIM_ADD`.

### 5, polish: 80 ms hover delay for explanation icons

Files: `src/components/ExplainTooltip.tsx`, `src/components/ExplainTooltip.test.tsx`.
Acceptance: tooltip appears at 80 ms hover; under 80 ms does not fire.
Message: `feat(tooltip): add 80 ms hover delay for explanation tooltips`.

### 6, polish: Ctrl+R refresh keeps popup open

Files: `src/keyboard/shortcuts.ts`, `rust/src/popup/keyboard.rs`,
`tests/popup_keyboard.rs`.
Acceptance: popup remains mounted after Ctrl+R; the focused provider refreshes.
Message: `feat(popup): keep popup open when Ctrl+R triggers a refresh`.

### 7, polish: in-popup confetti on celebration

Files: `src/components/CelebrationBurst.tsx`, `src/components/CelebrationBurst.test.tsx`.
Acceptance: debug-menu celebration with popup open plays a 1.2 second burst anchored
to the provider card.
Message: `feat(celebration): render an in-popup confetti burst when popup is open`.

### 8, polish: celebration utilization gate

Files: `rust/src/celebration/gate.rs`, `tests/celebration_gate.rs`.
Acceptance: celebration fires only with >=1% past-24h utilization (fixture).
Message: `feat(celebration): gate weekly reset celebration on 1% past-day utilization`.

### 9, polish: lint exclamation marks in en locale

Files: `scripts/lint-locales.mjs`, `package.json`, `.github/workflows/ci.yml`.
Acceptance: a stray `!` in the English bundle fails CI.
Message: `chore(locales): lint exclamation marks out of the English locale`.

### 10, a11y: focus ring across all interactive surfaces

Files: `src/styles/focus.css`, `src/components/*.tsx`.
Acceptance: tabbing through the popup shows a 2 px accent ring on every interactive
element.
Message: `feat(a11y): apply consistent 2 px accent focus ring to all controls`.

### 11, a11y: Narrator labels and roles

Files: `src/components/ProviderCard.tsx`, `src/components/UsageBar.tsx`,
`src/components/CopyButton.tsx`, `src/components/Tabs.tsx`.
Acceptance: Narrator walk reads each element with expected role and value.
Message: `feat(a11y): set roles and ARIA labels on popup elements`.

### 12, a11y: prefers-reduced-motion fallback

Files: `src/hooks/useReducedMotion.ts`, `src/styles/motion.css`,
`rust/src/animation/driver.rs`.
Acceptance: with OS Animations off, transitions instant, critter suppressed, tray
animation falls back to ellipsis hint.
Message: `feat(a11y): honour prefers-reduced-motion in popup and tray animations`.

### 13, a11y: High Contrast theme adaptation

Files: `src/styles/forced-colors.css`, `src/components/UsageBar.tsx`,
`rust/src/render/icon.rs`.
Acceptance: High Contrast Black strips Mica, bars get 2 px borders, critter
suppressed.
Message: `feat(a11y): adapt popup and tray rendering to High Contrast mode`.

### 14, i18n: locale coverage integration test

Files: `tests/locale_coverage.rs`, `tests/fixtures/required-keys.json`.
Acceptance: test fails on missing keys with locale and key in the error.
Message: `test(i18n): assert that pt-BR and zh-Hans cover every required key`.

### 15, i18n: refresh pt-BR and zh-Hans from upstream

Files: `src/locales/pt-BR/translation.json`, `src/locales/zh-Hans/translation.json`,
`rust/locales/pt-BR/codexbar.ftl`, `rust/locales/zh-Hans/codexbar.ftl`.
Acceptance: coverage test passes; native-speaker review note in PR body.
Message: `chore(i18n): refresh pt-BR and zh-Hans translations from upstream`.

### 16, packaging: expand Inno Setup script

Files: `installer/codexbar.iss`, `installer/assets/WizardLeft.bmp`,
`installer/assets/WizardSmall.bmp`, `installer/README-PORTABLE.txt`.
Acceptance: `iscc.exe /Qp installer\codexbar.iss` produces a clean
`CodexBar-<ver>-x64.exe` installable per-user without admin.
Message: `build(installer): expand Inno Setup script with WebView2 bootstrap and tasks`.

### 17, packaging: portable ZIP build script

Files: `scripts/build-portable.ps1`, `installer/README-PORTABLE.txt`.
Acceptance: produces `dist/CodexBar-<ver>-portable-x64.zip`; unzipped folder runs
from any location.
Message: `build(portable): add a portable ZIP build script`.

### 18, packaging: signing wrapper

Files: `scripts/sign-binaries.ps1`, `.github/workflows/release.yml`.
Acceptance: with a test cert, signs the three inner EXEs and the installer;
`signtool verify /pa /v` succeeds on all four.
Message: `build(sign): wrap signtool calls in a reusable PowerShell script`.

### 19, packaging: checksum generator

Files: `scripts/generate-checksums.ps1`.
Acceptance: produces a checksum file matching SHA-256 of listed artifacts.
Message: `build(release): generate SHA-256 checksums alongside release artifacts`.

### 20, updater: sign-manifest xtask

Files: `rust/xtask/src/sign_manifest.rs`, `rust/xtask/Cargo.toml`.
Acceptance: `cargo run --bin codexbar-xtask -- sign-manifest --version 1.0.0
--channel stable --installer <path>` writes a verifiable `latest.json`.
Message: `feat(xtask): add sign-manifest subcommand for Tauri update manifests`.

### 21, updater: channel-aware updater wiring

Files: `rust/src/updater/mod.rs`, `rust/src/updater/channel.rs`,
`tests/updater_channel.rs`.
Acceptance: the updater reads `updateChannel` from config and selects the matching
manifest URL on each check.
Message: `feat(updater): select manifest URL by configured channel`.

### 22, updater: update banner in popup

Files: `src/components/UpdateBanner.tsx`, `src/components/UpdateBanner.test.tsx`.
Acceptance: with a staged manifest pointing at a newer version, the banner renders
at the top of the popup with Update now / Later / dismiss.
Message: `feat(updater): render an update-available banner in the popup`.

### 23, ci: release workflow

Files: `.github/workflows/release.yml`.
Acceptance: tag push to a test repo runs end to end and produces all four release
artifacts plus the manifest.
Message: `ci(release): add tag-triggered release workflow for Windows`.

### 24, ci: lint-locales step

Files: `.github/workflows/ci.yml`.
Acceptance: CI run on a PR with an exclamation mark in the en bundle fails at the
lint-locales step.
Message: `ci(lint): run lint-locales on every PR`.

### 25, docs: README v1.0 rewrite

Files: `README.md`, `docs/screenshots/popup.png`, `docs/screenshots/preferences.png`,
`docs/screenshots/tray.png`, `docs/demo.gif`.
Acceptance: README opens with the demo gif, has install instructions for winget and
Inno, plus a known-limitations callout.
Message: `docs(readme): rewrite README for v1.0 with install instructions and demo gif`.

### 26, docs: CHANGELOG v1.0.0 entry

Files: `CHANGELOG.md`.
Acceptance: file follows Keep a Changelog format; v1.0.0 section lists features and
known limitations.
Message: `docs(changelog): add v1.0.0 entry in Keep a Changelog format`.

### 27, docs: BETA.md and SUPPORT.md

Files: `BETA.md`, `docs/SUPPORT.md`.
Acceptance: BETA.md explains the Beta channel install path; SUPPORT.md lists P0
through P3 SLA.
Message: `docs(beta): publish BETA.md and SUPPORT.md ahead of v1.0`.

### 28, telemetry: opt-in error reporting

Files: `rust/src/telemetry/sentry.rs`, `src/telemetry/sentry.ts`,
`src/components/preferences/TelemetryToggle.tsx`, `tests/sentry_pii.rs`.
Acceptance: toggle ON, manual panic posts to Sentry; toggle OFF (default), zero
network calls to Sentry.
Message: `feat(telemetry): add opt-in Sentry error reporting`.

### 29, github: issue templates and labels

Files: `.github/ISSUE_TEMPLATE/bug-report.yml`,
`.github/ISSUE_TEMPLATE/feature-request.yml`, `.github/labels.yml`.
Acceptance: issue picker shows both templates; `windows-only` label exists on the
repo.
Message: `chore(github): publish bug and feature issue templates with windows-only label`.

### 30, winget: manifest seed

Files: `docs/winget/CodexBar.CodexBar.installer.yaml`,
`docs/winget/CodexBar.CodexBar.locale.en-US.yaml`,
`docs/winget/CodexBar.CodexBar.yaml`.
Acceptance: `winget validate --manifest docs/winget/` passes clean.
Message: `chore(winget): seed manifest files for the CodexBar.CodexBar package`.

### 31, ga: tag v1.0.0 and announcement

Files: `README.md`, `version.env`.
Acceptance: `version.env` is bumped, README has the announcement, tag push triggers
the workflow.
Message: `release: cut v1.0.0 and announce on README`.

### 32, ga: upstream PR draft

Files: `docs/windows/upstream-pr-body.md`.
Acceptance: a Markdown file with the upstream PR title and body, ready to copy into
`steipete/CodexBar`.
Message: `docs(upstream): draft the upstream README PR body`.

## Phase acceptance tests

Phase is complete when every test below passes on a fresh Windows 11 24H2 sandbox VM.

1. Winget install. `winget install --id CodexBar.CodexBar --version 1.0.0 --silent`.
   Tray icon appears within 800 ms. Popup opens on click. Enabling Claude or Codex
   in Preferences and pasting a valid cookie causes a refresh that surfaces a
   session and weekly window.

2. Inno installer. Download `CodexBar-1.0.0-x64.exe` from GitHub Releases.
   Double-click. SmartScreen shows the publisher name from the EV cert with no
   "Don't run" warning. Installer completes without elevation. Tray icon appears
   and the app works as in test 1.

3. Portable. Download `CodexBar-1.0.0-portable-x64.zip`. Extract anywhere. Run
   `CodexBar.exe`. Tray icon appears. Config writes to the adjacent
   `portable-config\` directory.

4. Auto-update. Install v0.99.0-rc1 from a private test repo. Push the v1.0.0-rc2
   tag. Restart the installed app. Update banner appears within 5 seconds. Click
   Update now. New version is downloaded, signature verified, installer runs
   silently, app restarts, About pane reads 1.0.0-rc2.

5. Localization. Switch OS display language to `pt-BR`, launch the app, walk the
   popup and Preferences. Zero English strings. Repeat for `zh-Hans`.

6. Accessibility. Tab through the popup, focus rings visible on every element. Run
   Narrator. Each element announces with expected role and value. Enable High
   Contrast Black. Mica is stripped; bars render with system colors and 2 px
   borders.

7. Performance. ETW trace with popup closed, three providers active, 60 second
   cadence. Idle RSS under 70 MiB, idle CPU under 0.5 percent averaged over 60
   seconds, zero file I/O for 30 seconds after launch in Manual cadence.

8. Reduce motion. Toggle OS Animations off. Open the popup. Transitions are
   instant. Tray icon does not animate during refresh.

9. Crash reporting. Toggle telemetry ON. Trigger the debug "throw test panic"
   entry. A Sentry event appears in the dashboard within 30 seconds. Toggle OFF.
   Trigger again. No event appears.

10. Uninstall. Run the uninstaller from Settings > Apps.
    `%LOCALAPPDATA%\Programs\CodexBar` is removed. The Run registry key is removed.
    The custom URI protocol key is removed. A subsequent reinstall starts clean.

## CI gates

The release tag does not publish if any of these fail.

1. `cargo fmt --check`.
2. `cargo clippy --all-targets --all-features -- -D warnings`.
3. `cargo test --workspace`.
4. `pnpm lint` and `pnpm typecheck`.
5. `pnpm test --run`.
6. Locale-coverage integration test for all three locales.
7. `lint-locales` reports zero exclamation marks in the English bundle.
8. The polish-checklist tracker on `main` reports 64/64 items closed or deviated.
9. `signtool verify /pa /v` succeeds on all signed artifacts.
10. The Tauri updater manifest minisign signature verifies against the bundled key.

## Risks

Sorted by likelihood times impact.

1. Code-signing cert delay. If the EV cert is not in hand by start of phase, the
   SmartScreen story is materially worse. Fallback: ship with an OV cert, accept
   the "Don't run" warning for the first few weeks while reputation accrues, call
   it out in release notes. Mitigation: start cert procurement as early as possible.

2. SmartScreen reputation lag. Even with an OV cert, SmartScreen warns until enough
   installs build reputation. The threshold is not published. Mitigation: submit
   the binary to Microsoft Defender's false-positive form on each release for the
   first month, document the Run anyway path prominently in the README.

3. WebView2 redistributable on locked-down hosts. Some corporate environments
   block the bootstrap from Microsoft's CDN. Mitigation: publish a second installer
   variant (`CodexBar-<ver>-fixed-x64.exe`) that bundles the fixed-version
   WebView2 runtime inline (adds ~130 MB). Defer to Phase 10 unless a beta user
   reports the problem.

4. Tauri updater regression. The plugin is on 2.x and has had breaking changes
   between minor versions before. Mitigation: pin the plugin version in
   `Cargo.lock` and test the update lifecycle against the staged test repo before
   tagging GA.

5. Localization drift. Upstream may add strings between the Phase 6 import and the
   v1.0 tag. Mitigation: re-run the strings-to-json import as the last
   locale-touching commit before tag.

6. Per-user vs per-machine install split. A user who runs both installers ends up
   with two installations and two tray icons. Mitigation: startup check that scans
   both registry hives and offers a Remove duplicate prompt on first launch after
   install.

7. Winget submission rejection. First submission may bounce on validator nits
   (missing fields, locale inconsistencies, hash mismatches). Mitigation: validate
   manifests locally with `winget validate` and do the first submission manually
   via `wingetcreate new`.

8. Native-speaker review backlog. If no reviewer is available for pt-BR or zh-Hans
   Windows-only strings inside the two-week beta window, ship with machine-
   translated strings and a known limitation in CHANGELOG; iterate in 1.0.1.

## Time estimate

| Sub-area | Effort |
| --- | --- |
| A. Performance pass | 4 working days |
| B. Polish pass (64 items) | 6 working days |
| C. Accessibility | 3 working days |
| D. Localization | 3 working days |
| E. Packaging | 3 working days |
| F. Auto-update | 2 working days |
| G. Distribution (CI, manifests, docs) | 3 working days |
| H. Beta program (running it, triaging) | 5 working days, calendar parallel |
| I. GA cutover | 1 working day |
| Buffer for risk recovery | 3 working days |

Engineering time totals roughly 28 working days. At 50 percent allocation, that is
roughly 11 calendar weeks. With one engineer fully allocated, the path compresses to
about 6 calendar weeks. The two-week beta window runs in calendar parallel with the
polish and accessibility work, so the headline range is 3 to 6 weeks depending on
staffing.

## Open questions

1. EV or OV cert. EV removes SmartScreen friction at the cost of around $300/yr and
   an HSM. OV is cheaper but takes weeks to build reputation. Decision owed by
   start of Phase 9.
2. Sentry DSN ownership. Does the maintainer own the DSN, or do we publish a
   placeholder and ask each contributor to plug in their own?
3. ARM64 timing. The release workflow only builds x64. Ship ARM64 in v1.0 or
   defer to v1.1? Recommendation: defer.
4. MSIX timing. Microsoft Store path needs MSIX. Add a Store listing alongside Inno
   for v1.0 or defer? Recommendation: defer unless a marketing case is made.
5. Beta channel population. How do we attract testers in a two-week window?
   Recommendation: maintainer's audience plus the upstream README PR is sufficient;
   do not pay for ads.
6. Telemetry default. Even with an opt-in toggle, do we add a one-shot anonymous
   launch ping (version, OS build) for headcount? Recommendation: no, match the
   Mac app's zero-telemetry posture.
7. Donation model. Add a Buy Me a Coffee link or stay donation-free? Defer to the
   fork maintainer.
8. CHANGELOG audience. User-facing language or technical? Recommendation:
   user-facing with a small For Developers subsection.
9. Demo gif. Under 5 MB, under 60 seconds. Storyboard: tray icon appears, popup
   opens, switch tabs, click copy, trigger celebration, dismiss. Who records on
   which display? Recommendation: 1440p, zoom into the tray, 30 fps export with
   palettegen.
10. README screenshot retake cadence. Manual per release or automate in CI?
    Recommendation: manual for v1.0, revisit in v1.2 once visuals stabilize.

End of phase-9-release.md.
