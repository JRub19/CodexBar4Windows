---
phase: 1
title: "Foundations"
status: "planned"
predecessor: "phase-0-bootstrap"
successor: "phase-2-auth-subsystem"
owner: "core team"
audience: "Rust + TypeScript engineer implementing the Windows port"
length_target: "500 to 1000 lines"
---

# Phase 1, Foundations

## Why

Phase 0 produced a project skeleton with a tray icon and green CI. Phase 1 turns that skeleton into a runnable shell that can persist user intent, log its own behavior, schedule work, push state to the popup, and accept a future provider without further plumbing. After this phase the app boots, writes a config file under `%APPDATA%`, rotates logs under `%LOCALAPPDATA%`, runs an empty refresh tick on a real cadence, shows a "No providers configured" popup on left click of the tray, exposes a stable Tauri IPC contract, and ships TypeScript types generated from Rust. No real providers are wired yet, by design.

This phase is the hinge. Every later phase plugs into the seams created here. Doing it carelessly forces rework in Phase 2 (auth), Phase 3 (tray renderer), and Phase 4 (Claude provider). Doing it deliberately means new providers in later phases add exactly one folder under `rust/src/providers/` and one optional folder under `apps/desktop-tauri/src/providers/`.

The phase deliberately ships no business logic. The provider registry is empty. The refresh tick does nothing useful. That is fine. The acceptance bar is that the contracts are stable, the file system layout is correct, and the events flow end to end.

## Dependencies

Phase 0 is complete and on `main`. Concretely that means:

- Cargo workspace at `C:\Code\CodexBar4Windows\Cargo.toml` builds with `cargo check --workspace`.
- `apps\desktop-tauri\src-tauri\Cargo.toml` exists and produces `codexbar-desktop.exe` via `cargo tauri dev`.
- `apps\desktop-tauri\package.json` exists and `pnpm install` succeeds.
- A tray icon appears in the Windows notification area when the dev build runs.
- CI workflow `.github\workflows\ci.yml` runs `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `pnpm --filter desktop-tauri test` against the latest pinned toolchain.
- `version.env` ships a single source of truth for the app version.
- Branch policy from `CLAUDE.md` is in force: all work on `main`, atomic commits, conventional commit format, push after each commit, no em dashes in prose.

If any of the above is not true, return to Phase 0 before starting Phase 1.

## Deliverables

The deliverables below are listed in the order they will be implemented as atomic commits in §Tasks.

1. **Path environment**. A `PathEnvironment` struct under `rust\src\core\paths.rs` that resolves the four canonical directories, creates them on first run, and tightens NTFS ACLs to the current user on `secrets\`. Paths:
   - `%APPDATA%\CodexBar4Windows\config.json`
   - `%APPDATA%\CodexBar4Windows\secrets\` (current user only)
   - `%LOCALAPPDATA%\CodexBar4Windows\cache\`
   - `%LOCALAPPDATA%\CodexBar4Windows\logs\`
2. **Logging**. `tracing` plus `tracing-subscriber` initialized in `rust\src\logging\mod.rs`. JSON output to `logs\codexbar.log` with daily rotation, console mirror gated on `RUST_LOG`, `SensitiveString` newtype in `rust\src\logging\redact.rs`, and a `PersonalInfoRedactor` policy lifted in spirit from the Mac source.
3. **SettingsStore**. Serde model in `rust\src\settings\model.rs`, file-backed store in `rust\src\settings\store.rs`, mirroring the subset of the Mac schema we need at this phase: refresh cadence, enabled providers list, display preferences, debug toggles. Three Tauri commands: `get_settings`, `update_settings`, `reset_settings`.
4. **UsageStore skeleton**. Empty `UsageState` struct in `rust\src\core\usage_store.rs`, identity-siloing invariant enforced at the write boundary, three Tauri events: `usage:updated`, `status:updated`, `settings:changed`.
5. **Refresh loop skeleton**. `tokio` interval driven by `RefreshFrequency` in `rust\src\core\refresh.rs`, dispatchable to a (currently empty) provider registry, per-strategy timeout wrap, manual refresh command, honors `Manual` mode and a `pause_refresh` flag.
6. **Provider registry stub**. `inventory!` based catalog in `rust\src\providers\registry.rs`, validated at startup. Zero descriptors registered. Catalog is iterable, addressable by `ProviderId`, and serializable across IPC as `ProviderDescriptorDTO[]`.
7. **React popup**. Frameless `popup` window opened on left tray click via `apps\desktop-tauri\src-tauri\src\main.rs`. React app at `apps\desktop-tauri\src\App.tsx` mounts, subscribes to the three events, and renders "No providers configured. Open Preferences to enable." when the registry is empty. Mica on Win 11, Acrylic fallback on Win 10.
8. **IPC types**. `ts-rs` generates TypeScript types from Rust into `apps\desktop-tauri\src\bindings\` during `cargo test`. A single `pnpm run check:bindings` script verifies the committed bindings match the generator output.
9. **Workspace folders**. `rust\src\{core,host,logging,settings,locale}` established. `rust\src\locale\en.json` baseline string bundle.
10. **Tray context menu**. Native context menu via `muda` with four items: `Refresh now`, `Pause refresh`, `Preferences`, `Quit`. Preferences and Refresh now wire to no-op stubs that emit a `tracing` info line. Pause refresh toggles a state flag. Quit cleanly tears down the runtime.

## Tasks

Each task is one atomic commit. Title is the commit subject. Files lists the paths touched. Acceptance check is the local verification the engineer runs before pushing. Draft commit message is the Conventional Commit message to use. All commits push to `main` immediately on green local verification, per `CLAUDE.md`.

### Task 1, Path environment

| Field | Value |
| --- | --- |
| Title | `feat(core): add path environment with first run directory creation` |
| Files | `rust\src\core\mod.rs`, `rust\src\core\paths.rs`, `rust\Cargo.toml`, `rust\src\lib.rs` |
| Acceptance | `cargo test -p codexbar core::paths` passes. Running the test creates `%APPDATA%\CodexBar4Windows\` and `%LOCALAPPDATA%\CodexBar4Windows\` if absent, and inspecting `secrets\` via `icacls` shows the current user as the sole grantee. |
| Draft commit | `feat(core): add path environment with first run directory creation` |

API shape:

```rust
pub struct PathEnvironment {
    pub roaming: PathBuf,   // %APPDATA%\CodexBar4Windows
    pub local: PathBuf,     // %LOCALAPPDATA%\CodexBar4Windows
    pub config_file: PathBuf,
    pub secrets_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub logs_dir: PathBuf,
}

impl PathEnvironment {
    pub fn discover() -> Result<Self, PathError>;
    pub fn ensure(&self) -> Result<(), PathError>;
    fn tighten_acl(path: &Path) -> Result<(), PathError>;
}
```

`tighten_acl` calls `SetNamedSecurityInfoW` via the `windows` crate with a DACL that grants only the current user `FILE_ALL_ACCESS` and removes inheritance. The function is a no-op outside Windows targets.

### Task 2, Logging

| Field | Value |
| --- | --- |
| Title | `feat(logging): wire tracing with json file rotation and sensitive string redaction` |
| Files | `rust\src\logging\mod.rs`, `rust\src\logging\redact.rs`, `rust\src\logging\writer.rs`, `rust\Cargo.toml` |
| Acceptance | `cargo run -p codexbar-desktop` produces `%LOCALAPPDATA%\CodexBar4Windows\logs\codexbar.log` with at least one JSON line. Setting `RUST_LOG=codexbar=debug` and rerunning yields debug lines. A `cargo test` checks that `SensitiveString::display` renders `<redacted: 8 chars>` rather than the underlying string. |
| Draft commit | `feat(logging): wire tracing with json file rotation and sensitive string redaction` |

Dependencies: `tracing`, `tracing-subscriber` with `env-filter` and `json`, `tracing-appender` for non blocking rotation. Daily rotation, retention seven files, base name `codexbar`. Console output mirrors only when `RUST_LOG` is set or the binary is built in debug mode.

`SensitiveString` newtype wraps a `String`, implements `Debug` and `Display` to redact, and exposes `.expose_secret()` for the rare call site that needs the raw value. The redactor catalog covers tokens, cookies, emails, account ids, refresh tokens, and OAuth codes, mirroring `PersonalInfoRedactor.swift` from the Mac source.

### Task 3, Locale baseline

| Field | Value |
| --- | --- |
| Title | `feat(locale): add english string bundle and lookup helper` |
| Files | `rust\src\locale\mod.rs`, `rust\src\locale\en.json`, `rust\src\locale\loader.rs` |
| Acceptance | `cargo test -p codexbar locale` passes. `locale::lookup("popup.empty_state")` returns the English string. |
| Draft commit | `feat(locale): add english string bundle and lookup helper` |

The bundle ships keys for the popup empty state, the four tray menu items, and the three error toasts that exist at this phase. Future phases append. Loader supports the same `Localizable.xcstrings` style key paths as the Mac source so the strings can be diffed against upstream.

Keys shipped this phase:

```text
popup.empty_state            = "No providers configured. Open Preferences to enable."
tray.menu.refresh_now        = "Refresh now"
tray.menu.pause_refresh      = "Pause refresh"
tray.menu.resume_refresh     = "Resume refresh"
tray.menu.preferences        = "Preferences..."
tray.menu.quit               = "Quit CodexBar"
popup.title                  = "CodexBar"
error.settings.read_failed   = "Could not read settings. Using defaults."
error.settings.write_failed  = "Could not save settings. Changes were not persisted."
error.refresh.tick_failed    = "Refresh tick failed. See logs."
```

### Task 4, Settings model and store

| Field | Value |
| --- | --- |
| Title | `feat(settings): add serde backed settings store with three tauri commands` |
| Files | `rust\src\settings\mod.rs`, `rust\src\settings\model.rs`, `rust\src\settings\store.rs`, `rust\src\settings\commands.rs`, `apps\desktop-tauri\src-tauri\src\main.rs` |
| Acceptance | `cargo test -p codexbar settings` covers round trip serialize, default fill, atomic write via temp file + rename, and ACL preserved. `cargo tauri dev` followed by `invoke("get_settings")` from the devtools console returns the default JSON. |
| Draft commit | `feat(settings): add serde backed settings store with three tauri commands` |

Schema (subset of Mac, named with Rust idioms, serialized with snake case):

```rust
#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Settings {
    pub schema_version: u32,                  // 1
    pub refresh_frequency: RefreshFrequency,  // FiveMinutes default
    pub pause_refresh: bool,                  // false
    pub providers: Vec<ProviderToggle>,       // empty in phase 1
    pub display: DisplayPreferences,
    pub debug: DebugFlags,
    pub app_language: Option<String>,         // None means system
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub enum RefreshFrequency {
    Manual, OneMinute, TwoMinutes, FiveMinutes, FifteenMinutes, ThirtyMinutes,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProviderToggle { pub id: String, pub enabled: bool, pub order: u32 }

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DisplayPreferences {
    pub merge_icons: bool,
    pub usage_bars_show_used: bool,
    pub hide_quota_warning_markers: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DebugFlags { pub debug_menu_enabled: bool, pub verbose_logging: bool }
```

Atomic write rule: write to `config.json.tmp`, `fsync`, then `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`. On read failure, log a warning, back up the broken file as `config.json.broken-<utc-iso8601>`, and re-emit defaults.

Tauri commands:

```rust
#[tauri::command] async fn get_settings(store: State<'_, SettingsHandle>) -> Result<Settings, String>;
#[tauri::command] async fn update_settings(patch: SettingsPatch, store: State<'_, SettingsHandle>) -> Result<Settings, String>;
#[tauri::command] async fn reset_settings(store: State<'_, SettingsHandle>) -> Result<Settings, String>;
```

`SettingsPatch` is a partial mirror of `Settings` with every field `Option`. Updates emit `settings:changed` with the new full snapshot.

### Task 5, UsageStore skeleton

| Field | Value |
| --- | --- |
| Title | `feat(core): add usage store skeleton with identity siloing invariant` |
| Files | `rust\src\core\usage_store.rs`, `rust\src\core\events.rs`, `rust\src\core\mod.rs` |
| Acceptance | Unit test in `usage_store.rs` asserts that writing a snapshot whose `identity.provider_id` does not match the slot returns `IdentityMismatch` and does not mutate state. A second test asserts that a cross thread `Arc` clone reads consistent state under contention. |
| Draft commit | `feat(core): add usage store skeleton with identity siloing invariant` |

```rust
pub struct UsageStore {
    state: Arc<RwLock<UsageState>>,
    menu_rev: AtomicU64,
    icon_rev: AtomicU64,
    tx: tokio::sync::broadcast::Sender<UsageEvent>,
}

#[derive(Clone, Default, Debug)]
pub struct UsageState { /* empty in phase 1, fields appended later */ }

impl UsageStore {
    pub fn write_snapshot(&self, provider: ProviderId, snapshot: UsageSnapshot) -> Result<(), StoreError>;
    pub fn read(&self) -> RwLockReadGuard<'_, UsageState>;
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<UsageEvent>;
}
```

`write_snapshot` enforces `snapshot.identity.provider_id == provider`, increments `menu_rev` and `icon_rev` as appropriate, and emits a `UsageEvent::Updated { provider }`. The event is bridged to the Tauri side and re-emitted as `usage:updated` with payload `{ provider: string, menu_rev: number, icon_rev: number }`.

### Task 6, Provider registry stub

| Field | Value |
| --- | --- |
| Title | `feat(providers): add inventory backed provider registry stub` |
| Files | `rust\src\providers\mod.rs`, `rust\src\providers\registry.rs`, `rust\src\providers\descriptor.rs`, `rust\src\providers\errors.rs`, `rust\Cargo.toml` |
| Acceptance | `cargo test -p codexbar providers::registry` passes. `ProviderCatalog::build` panics with a clear message when duplicate ids are submitted. With zero submissions the catalog is empty and `provider_descriptors` command returns `[]`. |
| Draft commit | `feat(providers): add inventory backed provider registry stub` |

```rust
pub struct ProviderRegistration {
    pub descriptor: fn() -> ProviderDescriptor,
}
inventory::collect!(ProviderRegistration);

pub struct ProviderCatalog { /* HashMap<ProviderId, ProviderDescriptor> */ }

impl ProviderCatalog {
    pub fn build(iter: impl IntoIterator<Item = &'static ProviderRegistration>) -> Self;
    pub fn descriptors(&self) -> impl Iterator<Item = &ProviderDescriptor>;
    pub fn get(&self, id: ProviderId) -> Option<&ProviderDescriptor>;
}

pub static REGISTRY: Lazy<ProviderCatalog> = Lazy::new(|| {
    ProviderCatalog::build(inventory::iter::<ProviderRegistration>())
});
```

`ProviderDescriptor` is the minimal struct from §2 of the provider architecture spec, with most sub structs marked `non_exhaustive` so they grow without breakage in later phases. Sub structs at this phase: `ProviderMetadata`, `ProviderBranding`, `ProviderFetchPlan` (with empty pipeline), `ProviderCLIConfig`. `ProviderId` is a `&'static str` newtype that doubles as the persistence key.

### Task 7, Refresh loop skeleton

| Field | Value |
| --- | --- |
| Title | `feat(core): add refresh loop skeleton with manual and paused modes` |
| Files | `rust\src\core\refresh.rs`, `rust\src\core\mod.rs`, `rust\src\settings\store.rs`, `apps\desktop-tauri\src-tauri\src\main.rs` |
| Acceptance | Unit test asserts a `OneMinute` cadence yields four ticks across 4 minutes of `tokio::time::pause`. Test asserts `Manual` cadence yields zero automatic ticks. Test asserts `pause_refresh = true` skips ticks until cleared. Test asserts a single in-flight tick is not re-entered while still running. |
| Draft commit | `feat(core): add refresh loop skeleton with manual and paused modes` |

```rust
pub struct RefreshLoop {
    settings: SettingsHandle,
    store: Arc<UsageStore>,
    catalog: &'static ProviderCatalog,
    in_flight: AtomicBool,
    manual_tx: mpsc::Sender<ManualTrigger>,
}

impl RefreshLoop {
    pub fn spawn(self: Arc<Self>) -> JoinHandle<()>;
    pub async fn tick(&self) -> Result<(), RefreshError>;
}
```

Behavior of one tick:

1. Bail if `in_flight` already true. Set true.
2. Snapshot the enabled providers from `SettingsStore`. With an empty registry this is the empty set.
3. For each provider in the snapshot, build a `ProviderFetchContext` and call the pipeline, wrapping each strategy run in `tokio::time::timeout(45s, ...)`. Phase 1 never enters this branch.
4. Fold results into `UsageStore`. Phase 1 writes nothing.
5. Emit `usage:updated` with a zero delta payload.
6. Set `in_flight` false.

The loop subscribes to `settings:changed` and restarts the interval when `refresh_frequency` or `pause_refresh` changes. `restart_timer` cancels the active sleep but never cancels an in-flight tick.

Tauri command: `#[tauri::command] async fn refresh_now() -> Result<(), String>` triggers a manual tick. Honors `Manual` mode by always running, but honors `pause_refresh = true` by returning an error to the caller.

### Task 8, IPC types codegen

| Field | Value |
| --- | --- |
| Title | `feat(host): generate typescript bindings from rust ipc types via ts-rs` |
| Files | `rust\src\host\dto.rs`, `rust\src\host\mod.rs`, `rust\Cargo.toml`, `apps\desktop-tauri\src\bindings\*.ts` (generated), `apps\desktop-tauri\package.json`, `scripts\check-bindings.ps1` |
| Acceptance | `cargo test -p codexbar host::dto::export` writes the `.ts` files. `pnpm run check:bindings` exits zero on a clean repo. Editing a Rust DTO without regenerating fails the check with a clear diff. |
| Draft commit | `feat(host): generate typescript bindings from rust ipc types via ts-rs` |

DTOs exported in phase 1:

```text
Settings
SettingsPatch
RefreshFrequency
ProviderToggle
DisplayPreferences
DebugFlags
ProviderDescriptorDto
ProviderMetadataDto
ProviderBrandingDto
UsageEventPayload
StatusEventPayload
SettingsChangedPayload
```

Generated files live under `apps\desktop-tauri\src\bindings\` and are committed. `scripts\check-bindings.ps1` runs the generator into a temp dir and diffs against the committed copies. CI runs this script in addition to `cargo test`.

### Task 9, Tauri command surface

| Field | Value |
| --- | --- |
| Title | `feat(host): register tauri command surface for phase 1` |
| Files | `apps\desktop-tauri\src-tauri\src\main.rs`, `apps\desktop-tauri\src-tauri\src\commands.rs`, `apps\desktop-tauri\src-tauri\capabilities\default.json` |
| Acceptance | `cargo tauri dev` followed by each `invoke(...)` call from devtools returns the expected JSON. The capability ACL grants the popup window access to exactly the commands listed below. |
| Draft commit | `feat(host): register tauri command surface for phase 1` |

Commands registered:

| Command | Args | Returns | Notes |
| --- | --- | --- | --- |
| `get_settings` | none | `Settings` | Reads the current snapshot. |
| `update_settings` | `{ patch: SettingsPatch }` | `Settings` | Atomic write, emits `settings:changed`. |
| `reset_settings` | none | `Settings` | Restores defaults, emits `settings:changed`. |
| `provider_descriptors` | none | `ProviderDescriptorDto[]` | Empty in phase 1. |
| `provider_snapshots` | none | `Record<ProviderId, UsageSnapshotDto>` | Empty in phase 1. |
| `refresh_now` | none | `()` | Triggers a manual tick. |
| `toggle_pause` | `{ paused: bool }` | `()` | Persists `pause_refresh`. |
| `open_preferences` | none | `()` | No-op stub, logs an info line. |
| `dump_log_window` | `{ window: enum { Last5m, Last1h } }` | `string` | Reads the last N minutes of `codexbar.log`. |

Events emitted:

| Event | Payload | When |
| --- | --- | --- |
| `usage:updated` | `UsageEventPayload` | After every fold step. |
| `status:updated` | `StatusEventPayload` | After every status poll. None in phase 1. |
| `settings:changed` | `SettingsChangedPayload` | After every settings write. |

### Task 10, Tray context menu

| Field | Value |
| --- | --- |
| Title | `feat(tray): add native context menu with four phase 1 items` |
| Files | `apps\desktop-tauri\src-tauri\src\tray.rs`, `apps\desktop-tauri\src-tauri\src\main.rs`, `rust\src\locale\en.json` |
| Acceptance | Right click on tray icon opens a four item menu. Left click on tray icon opens the popup window. `Pause refresh` toggles to `Resume refresh` and back. `Quit` closes the app cleanly with all background tasks joined. |
| Draft commit | `feat(tray): add native context menu with four phase 1 items` |

Implementation notes:

- Use `muda` for the native menu, scoped to the tray.
- Build the menu once at boot; rebuild only when `Pause refresh` toggles (label flips) or the locale changes.
- The Quit handler signals a `CancellationToken` to the refresh loop, awaits its join handle, flushes the tracing appender, then calls `app.exit(0)`.
- Left click handler computes the popup position via `Shell_NotifyIconGetRect`, clamps to the active monitor, and calls `show_popup(x, y)` on the popup window manager.

### Task 11, Popup window and React empty state

| Field | Value |
| --- | --- |
| Title | `feat(popup): add frameless popup window with mica fallback and empty state` |
| Files | `apps\desktop-tauri\src-tauri\src\windows.rs`, `apps\desktop-tauri\src-tauri\tauri.conf.json`, `apps\desktop-tauri\src\App.tsx`, `apps\desktop-tauri\src\components\EmptyState.tsx`, `apps\desktop-tauri\src\hooks\useUsageEvents.ts`, `apps\desktop-tauri\src\hooks\useSettings.ts`, `apps\desktop-tauri\src\styles\popup.css` |
| Acceptance | Left tray click opens a 360x520 frameless window anchored to the tray rect. Closing the window does not exit the app. On Win 11 the background uses Mica. On Win 10 it falls back to Acrylic. With the registry empty the popup shows the empty state copy from `popup.empty_state`. |
| Draft commit | `feat(popup): add frameless popup window with mica fallback and empty state` |

Window config (`tauri.conf.json` window entry):

```json
{
  "label": "popup",
  "title": "CodexBar",
  "width": 360,
  "height": 520,
  "decorations": false,
  "transparent": true,
  "alwaysOnTop": false,
  "skipTaskbar": true,
  "visible": false,
  "resizable": false,
  "shadow": true
}
```

Background material is applied via `tauri::window::WebviewWindowBuilder::effects` on Win 11 with `Mica` and falls back to `Acrylic` via `windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea` plus `SetWindowCompositionAttribute` on Win 10. Detection: `IsWindows11OrGreater` from `windows-version`.

React surface in `App.tsx`:

- Mounts a `<SettingsProvider>` that calls `get_settings` once and subscribes to `settings:changed`.
- Mounts a `<UsageProvider>` that calls `provider_snapshots` once and subscribes to `usage:updated`.
- Renders `<EmptyState />` when `descriptors.length === 0`.
- The empty state CTA `Open Preferences` invokes `open_preferences`.

### Task 12, Phase wiring and CI gates

| Field | Value |
| --- | --- |
| Title | `chore(ci): add phase 1 gates for bindings drift and clippy strictness` |
| Files | `.github\workflows\ci.yml`, `scripts\check-bindings.ps1`, `rust\clippy.toml`, `apps\desktop-tauri\package.json` |
| Acceptance | A PR that edits a DTO without regenerating bindings is red. A PR that introduces a `clippy::unwrap_used` warning is red. A PR that adds a new Tauri command without an ACL entry is red. |
| Draft commit | `chore(ci): add phase 1 gates for bindings drift and clippy strictness` |

CI gates added by this phase:

- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets -- -D warnings -D clippy::unwrap_used -D clippy::expect_used`.
- `cargo test --workspace --all-features`.
- `pnpm --filter desktop-tauri run check:bindings`.
- `pnpm --filter desktop-tauri run lint` (eslint with the existing config).
- `pnpm --filter desktop-tauri run typecheck` (tsc, no emit).
- Capability lint: a small Rust test under `apps\desktop-tauri\src-tauri\tests\capabilities.rs` reads `capabilities\default.json` and asserts every registered command is present.

## Phase level acceptance tests

After all twelve tasks land on `main`, the following manual and automated checks must pass on a clean Windows 11 machine and a clean Windows 10 22H2 machine.

### Boot and persistence

1. Delete `%APPDATA%\CodexBar4Windows\` and `%LOCALAPPDATA%\CodexBar4Windows\` if present.
2. Launch the dev build. Confirm the tray icon appears.
3. Open File Explorer. Confirm:
   - `%APPDATA%\CodexBar4Windows\config.json` exists and contains the default JSON.
   - `%APPDATA%\CodexBar4Windows\secrets\` exists. `icacls` reports only the current user.
   - `%LOCALAPPDATA%\CodexBar4Windows\cache\` exists.
   - `%LOCALAPPDATA%\CodexBar4Windows\logs\codexbar.log` exists and contains at least one JSON record with `level: "INFO"` and `target: "codexbar::core::refresh"`.

### Refresh loop

1. Right click tray, select `Refresh now`. Logs show `refresh.tick.start` and `refresh.tick.end` lines with the same `tick_id`.
2. Right click tray, select `Pause refresh`. Wait two minutes with `refresh_frequency = OneMinute`. Logs show zero new tick lines.
3. Right click tray, select `Resume refresh`. Within sixty seconds a tick line appears.
4. Open Preferences (no-op stub logs `open_preferences.invoked`). Confirm the log line.

### Popup

1. Left click tray. Popup opens within 200 ms.
2. Visually confirm Mica on Win 11, Acrylic on Win 10.
3. Confirm the popup body shows exactly the localized empty state copy.
4. Click outside the popup. Popup hides. Tray icon remains.
5. Left click tray again. Popup re-opens. State is identical (no flicker).

### IPC and bindings

1. From devtools, run `await window.__TAURI__.invoke('get_settings')`. Returns the default `Settings`.
2. Run `await window.__TAURI__.invoke('update_settings', { patch: { refresh_frequency: 'OneMinute' } })`. Returns the updated settings. Confirm `config.json` reflects the change.
3. Run `await window.__TAURI__.invoke('reset_settings')`. Returns defaults. Confirm `config.json` is back to defaults.
4. Run `await window.__TAURI__.invoke('provider_descriptors')`. Returns `[]`.
5. Subscribe to `settings:changed` in devtools. Toggle Pause refresh via tray. Confirm an event fires with the new flag.

### CI gates

1. Open a draft PR that adds a stray `unwrap()` in `rust\src\core\refresh.rs`. CI fails on clippy.
2. Open a draft PR that adds a field to `Settings` without regenerating bindings. CI fails on `check:bindings`.
3. Open a draft PR that registers a new Tauri command without updating `capabilities\default.json`. CI fails on the capabilities test.
4. Close all three PRs without merging.

### Clean shutdown

1. With the dev build running and a tick currently in flight, right click tray and select `Quit`.
2. Confirm the process exits within five seconds. Confirm the log file has a final `app.shutdown.complete` line. Confirm no orphan tray icon in the notification area after a Windows Explorer refresh.

## CI gates introduced

| Gate | Job | Purpose |
| --- | --- | --- |
| `cargo fmt --all --check` | `lint` | Style discipline. |
| `cargo clippy --workspace --all-targets -- -D warnings -D clippy::unwrap_used -D clippy::expect_used` | `lint` | Forbid hidden panics. |
| `cargo test --workspace --all-features` | `test` | Unit and integration coverage. |
| `pnpm --filter desktop-tauri run check:bindings` | `lint` | Prevent IPC type drift. |
| `pnpm --filter desktop-tauri run lint` | `lint` | TS style discipline. |
| `pnpm --filter desktop-tauri run typecheck` | `test` | TS soundness. |
| `cargo test -p codexbar-desktop --test capabilities` | `test` | Every command has an ACL entry. |
| `pnpm --filter desktop-tauri build` | `build` | The popup app compiles to a production bundle. |
| `cargo tauri build --debug` | `build` | The dev installer assembles end to end. |

The `build` job runs only on PR merge to `main` and on tagged releases, to keep PR turnaround under three minutes.

## Risks

| Risk | Likelihood | Impact | Mitigation |
| --- | --- | --- | --- |
| `inventory!` link-time registration drops descriptors when the binary is built with LTO or `--cfg dead_code_elimination`. | medium | high (silent registry truncation) | Ship a `#[used]` shim per registration. Add a startup assertion: count of `inventory::iter::<ProviderRegistration>()` matches a compile-time constant declared in `providers::registry`. The constant is zero in phase 1, so the assertion exercises the path without blocking later growth. |
| `ts-rs` output drifts across host platforms (path separators, trailing newlines). | medium | medium | Normalize via a small post-process step in the test that exports bindings: convert CRLF to LF, sort enum variants, ensure a single trailing newline. `check-bindings.ps1` applies the same normalization before diffing. |
| Mica is unavailable on Win 10 and the Acrylic fallback flickers on first paint. | medium | low | Initial paint uses a solid `--surface-popup` background; the material effect applies once the first frame composites. Tested via a 250 ms `WM_PAINT` watchdog. |
| Tightening NTFS ACLs breaks AV scanners that watch the secrets directory. | low | medium | Tighten only `secrets\`, not the parent. Log the ACL change with the previous and new SDDL. Add a documented escape hatch: `--no-acl-tighten` CLI flag for support cases. |
| `tracing-appender` daily rotation buffers writes during shutdown, losing the final lines. | medium | low | Hold a `WorkerGuard` in `App` state and drop it explicitly in the Quit handler before `app.exit(0)`. Add a regression test that asserts the final `app.shutdown.complete` line is present after a clean shutdown. |
| Popup position math is wrong on multi monitor or DPI mixed setups. | high | medium | Use `Shell_NotifyIconGetRect` (NOTIFYICON_VERSION_4) plus `MonitorFromRect` plus per-monitor DPI awareness in the manifest. Ship a fallback that anchors to the active monitor work area corner. Document the multi monitor smoke test in the acceptance section. |
| `pause_refresh` and `Manual` cadence interact in surprising ways for a future provider that asks "should I run now?". | low | medium | Document in `refresh.rs` that the two flags compose: pause wins. Add a unit test that pins the precedence. |
| Identity siloing invariant is checked only at the write boundary, leaving the read side trusting. | medium | high (privacy bug) | Add an additional debug-only assertion in the React `useUsageEvents` hook that warns if an event's `provider` does not match the slot it is filed under. Promote to a hard error if it ever fires in tests. |
| Settings file corruption (power loss mid write). | medium | high | Atomic write via temp file plus `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`. On parse failure, back up the corrupt file with an ISO 8601 suffix and emit defaults. Phase 2 adds a recovery UI; phase 1 logs and continues. |
| Tauri command surface drifts away from the bindings without anyone noticing. | medium | medium | The `capabilities` test ensures the ACL stays in sync, but does not catch a renamed argument. Add a doctest on each command that calls `invoke_with_bindings::<Settings>("get_settings")` to keep the names and shapes pinned. |

## Time estimate

| Task | Estimate |
| --- | --- |
| 1, Path environment | 0.5 day |
| 2, Logging | 0.5 day |
| 3, Locale baseline | 0.25 day |
| 4, Settings model and store | 1 day |
| 5, UsageStore skeleton | 0.5 day |
| 6, Provider registry stub | 0.5 day |
| 7, Refresh loop skeleton | 1 day |
| 8, IPC types codegen | 0.5 day |
| 9, Tauri command surface | 0.75 day |
| 10, Tray context menu | 0.5 day |
| 11, Popup window and React empty state | 1.25 day |
| 12, CI gates | 0.5 day |
| Phase acceptance run | 0.25 day |
| **Total** | **8 working days** for one senior engineer, end to end |

A two engineer split (one Rust, one TS) can parallelize tasks 4 through 7 (Rust) with tasks 8 and 11 (TS), bringing the wall clock to roughly five days. The bindings handshake in task 8 is the synchronization point.

## Open questions

1. Do we ship a Windows 10 build at all, or do we make Windows 11 the minimum and let task 11 drop the Acrylic fallback? Current assumption: ship Win 10 22H2 minimum, fall back gracefully. Revisit before Phase 4.
2. Should `pause_refresh` survive a restart, or should it always reset to `false` on boot? Current assumption: persisted, mirrors Mac. A pinned tray menu reminder ("Refresh paused") is added in Phase 3.
3. Do we want a `--portable` mode that ignores `%APPDATA%` and uses a directory next to the exe? Current assumption: no for v1. Revisit if user demand emerges.
4. Should `dump_log_window` redact through `PersonalInfoRedactor` even on read, or trust that writes were already redacted? Current assumption: redact on read as a defense in depth. Performance is fine at log sizes we expect.
5. Where do localized strings for the tray context menu live: in the Rust bundle (for native `muda`) or duplicated in the TS bundle (for the popup mirror)? Current assumption: Rust is source of truth; TS imports a generated JSON snapshot via `ts-rs` to stay in sync. Confirm during task 3 review.
6. Do we want a single `usage:updated` event with the full state attached, or the patch envelope from the architecture spec? Current assumption: patch envelope (just `{ provider, menu_rev, icon_rev }`), pull state via `invoke("provider_snapshots")`. Matches §3 of `30-provider-system-architecture.md`. Confirm with the React side before merging task 5.
7. Should the manual `refresh_now` command honor `Manual` cadence (run anyway) or refuse when paused? Current assumption: runs in `Manual` mode, refuses when `pause_refresh = true`. The error string is surfaced via toast in Phase 3.
8. Do we keep `cargo tauri build --debug` in CI for every PR, or only on merge? Current assumption: merge only. The PR build remains under three minutes; release builds remain under fifteen.
