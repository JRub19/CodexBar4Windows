---
summary: "Phase 8 plan for CodexBar4Windows: rich Mica-styled Preferences window with seven panes, first-run onboarding flow, global hotkeys via tauri-plugin-global-shortcut, Launch-at-sign-in via the Run registry key, and a Stable/Beta update channel selector wired to the Tauri updater."
read_when:
  - Executing Phase 8 work
  - Verifying acceptance for entry into Phase 9 (Polish, Packaging, Release v1.0)
  - Onboarding a new engineer to the Preferences, onboarding, or hotkey subsystems
related:
  - docs/windows/spec/20-preferences-ui.md
  - docs/windows/spec/80-feel-and-polish.md
  - docs/windows/05-windows-ux-spec.md
  - docs/windows/plan/phase-0-bootstrap.md
---

# Phase 8 Preferences UI, Onboarding, and Hotkeys

One-line goal: deliver the rich Mica-styled Preferences window, the first-run onboarding flow, and the global hotkeys subsystem so a fresh user installs CodexBar4Windows, opens the popup, walks through onboarding, picks providers, signs in, and is reading their usage in the tray inside five minutes without ever opening the docs.

## 1. Why this phase exists

Phases 0 through 7 produced an app that *works*. The tray icon lights up, providers fetch usage, cost data flows, status checks land. But the only way a user can configure any of it is by editing `%APPDATA%\CodexBar4Windows\config.json` by hand. That is acceptable for the engineering team. It is not acceptable for a v1.0 release.

Phase 8 is the moment CodexBar4Windows becomes self-service. After this phase a brand-new user installs the MSI, sees a welcome toast, gets walked through provider selection, signs into each provider through a real OAuth or device-flow button, and never needs to read a markdown file. Right-click on the tray icon opens Preferences. Preferences looks like a native Windows 11 Settings window: Mica background, sidebar nav, live-apply controls, no Save button, no modal validation, no friction.

This phase also lands two pieces of infrastructure that the rest of the product has been waiting for. The first is the global hotkey: Win+Shift+U toggles the popup from anywhere. The second is Launch at sign-in: the user's preference survives reboot via a `HKCU\...\Run` registry entry. Without these two, the app does not feel native.

Finally, this phase wires the Tauri updater's channel selector so beta testers can opt in without rebuilding from source, and so the v1.0 release pipeline in Phase 9 has somewhere to publish to.

## 2. Dependencies on earlier phases

This phase depends on the following landed work:

- Phase 0: Tauri 2 + React + TypeScript workspace, Cargo workspace at root, green CI on `windows-latest`.
- Phase 1: tray icon subsystem, popup webview, IPC bridge between Rust core and React.
- Phase 2: settings store crate (`codexbar-core::config`) with `%APPDATA%\CodexBar4Windows\config.json` read/write, registry shim for `HKCU\Software\CodexBar4Windows\Defaults\*`, and DPAPI wrapper for secret fields.
- Phase 3: provider registry, per-provider settings descriptors (`ProviderMetadata`), `ProviderError` shape.
- Phase 4: provider auth subsystem (Claude OAuth, Codex managed accounts, Copilot device flow, cookie import, manual cookie textarea backing store).
- Phase 5: refresh cadence engine, `refreshFrequency` enum, manual refresh command.
- Phase 6: cost-scan and quota-warning engine with `quotaWarningThresholds`, `quotaWarningMarkersVisible`, `quotaWarningSessionEnabled`, `quotaWarningWeeklyEnabled`, `quotaWarningSoundEnabled`.
- Phase 7: status check engine, sidebar status dots, vendor statuspage poller.

Phase 8 does not depend on Phase 9 (packaging) or any later phase.

## 3. Deliverables

A numbered list of concrete artifacts that exist on disk at the end of Phase 8.

1. A new Tauri window `settings` configured in `apps/desktop-tauri/src-tauri/tauri.conf.json`: label `settings`, default 880x640, min 720x560, `decorations: false`, Mica background applied at runtime via `window_vibrancy::apply_mica` with Acrylic and solid fallbacks, single-instance, hidden on app start, opened via tray menu "Preferences" or hotkey.
2. A React route tree under `apps/desktop-tauri/src/settings/` with seven panes: General, Providers, Display, Notifications, Shortcuts, Advanced, About. Each pane is one TSX file in `apps/desktop-tauri/src/settings/panes/`.
3. A sidebar component `apps/desktop-tauri/src/settings/components/Sidebar.tsx` rendering the 220 px nav rail, search field at the top, "Quit CodexBar" ghost button at the bottom.
4. A custom 32 px non-client title strip at `apps/desktop-tauri/src/settings/components/TitleBar.tsx` with app icon, title text, and min/max/close buttons using Segoe Fluent Icons.
5. Settings persistence wiring: a `SettingsBridge` module in `apps/desktop-tauri/src/settings/lib/bridge.ts` that exposes `getSetting(key)`, `setSetting(key, value)`, `subscribe(key, cb)`. Backed by Tauri commands `get_setting`, `set_setting`, `subscribe_settings` in `apps/desktop-tauri/src-tauri/src/settings_cmds.rs`. Writes debounced 350 ms before hitting `%APPDATA%\CodexBar4Windows\config.json`.
6. A first-run detector in `apps/desktop-tauri/src-tauri/src/onboarding.rs`. Triggered when `%APPDATA%\CodexBar4Windows\config.json` does not exist or `onboardingCompleted` flag is false. Emits an IPC event `onboarding:start` to the popup.
7. An onboarding flow in `apps/desktop-tauri/src/onboarding/` with three React steps: welcome toast trigger, provider picker, per-provider sign-in.
8. A global hotkey subsystem in `apps/desktop-tauri/src-tauri/src/hotkeys.rs` built on `tauri-plugin-global-shortcut`. Registers Win+Shift+U by default, exposes commands `register_hotkey(action, chord)`, `unregister_hotkey(action)`, `list_hotkey_conflicts(chord)`.
9. Launch-at-sign-in implementation in `apps/desktop-tauri/src-tauri/src/launch_at_login.rs`. Writes/removes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CodexBar4Windows` with value `"C:\Program Files\CodexBar4Windows\CodexBar4Windows.exe" --hidden`. CLI flag `--hidden` recognized in `main.rs` skips the splash, hides the popup, only the tray icon paints.
10. Update channel selector in About pane: dropdown `stable` / `beta`, wired to a new Tauri command `set_update_channel(channel)` in `apps/desktop-tauri/src-tauri/src/updater.rs` which configures the Tauri updater endpoint to `https://github.com/JRub/CodexBar4Windows/releases/latest/download/latest-<channel>.json`.
11. A live-apply settings event bus: any settings mutation emits `settings:changed` with `{ key, value }`. The tray icon controller, popup, and other windows subscribe to relevant keys.
12. Validation feedback components in `apps/desktop-tauri/src/settings/components/`: `InlineError.tsx`, `FieldShake.tsx`, `SavingIndicator.tsx`.
13. i18n wiring: `apps/desktop-tauri/src/i18n/` with `en`, `zh-Hans`, `pt-BR` JSON dictionaries. `appLanguage` setting drives a remount of the settings window.
14. A `docs/windows/plan/branch-protection.md` update if the CI gates change; otherwise no docs changes besides this plan.
15. Telemetry events (opt-in, off by default; this phase wires the bus, does not light up any backend): `prefs_opened`, `prefs_pane_switched`, `onboarding_started`, `onboarding_step_completed`, `onboarding_finished`, `hotkey_registered`, `hotkey_conflict`, `launch_at_login_enabled`, `launch_at_login_disabled`.

## 4. Tasks

Each task below is one atomic commit. The numbering is the recommended commit order. Each task targets 30 minutes to 2 hours of work. Every task ends with the changes pushed to `origin/main`.

### Task 1: Scaffold the `settings` Tauri window

- Files touched: `apps/desktop-tauri/src-tauri/tauri.conf.json`, `apps/desktop-tauri/src-tauri/src/lib.rs`, `apps/desktop-tauri/src-tauri/src/windows/settings.rs` (new).
- What changes: add a second window definition `{ "label": "settings", "title": "CodexBar4Windows", "url": "/settings", "width": 880, "height": 640, "minWidth": 720, "minHeight": 560, "decorations": false, "visible": false, "skipTaskbar": false, "transparent": false }`. Add a Rust function `open_settings_window(app: &AppHandle)` that creates the window if absent and focuses it. Wire it to a new Tauri command `open_settings` and to the existing tray menu "Preferences" item.
- Acceptance check: `npm run tauri dev` launches the app; tray menu "Preferences" opens an empty WebView2 window at 880x640. Title bar is OS default (will be replaced in Task 4). Closing the window does not quit the app. Reopening reuses the existing window (no double-open).
- Draft commit message: `feat(settings): scaffold preferences webview window`

### Task 2: React route for `/settings` with empty pane skeleton

- Files touched: `apps/desktop-tauri/src/App.tsx`, `apps/desktop-tauri/src/main.tsx`, `apps/desktop-tauri/src/settings/SettingsApp.tsx` (new), `apps/desktop-tauri/src/settings/panes/General.tsx` (new) through `About.tsx` (new), 7 stub files.
- What changes: add a router (use `react-router-dom` if already present, else a minimal switch on `window.location.pathname`). When path is `/settings`, render `<SettingsApp />`. `SettingsApp` renders a fixed 220 px sidebar with the seven pane names hardcoded, and a content area showing the selected pane stub. Each pane stub renders `<h1>{paneName}</h1>` for now.
- Acceptance check: opening the settings window shows a left sidebar with General, Providers, Display, Notifications, Shortcuts, Advanced, About. Clicking each item swaps the heading. No keyboard nav yet.
- Draft commit message: `feat(settings): add seven-pane sidebar shell`

### Task 3: Apply Mica via `window_vibrancy` with Acrylic and solid fallbacks

- Files touched: `apps/desktop-tauri/src-tauri/Cargo.toml` (add `window-vibrancy = "0.5"`), `apps/desktop-tauri/src-tauri/src/windows/settings.rs`.
- What changes: after the settings window is created, call `apply_mica(&window, Some(true))` on Win11; on failure call `apply_acrylic(&window, Some((30, 30, 35, 200)))`; on failure set background `#1B1B1F` (dark) or `#FFFFFF` (light) based on the registry key `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`. Read the theme key on window creation and on `WM_SETTINGCHANGE`.
- Acceptance check: on Win11 the settings window background is Mica (transparent, tinted by accent color). Toggling Windows theme between Light and Dark flips the React background variables within 500 ms. On a Win10 VM the window is Acrylic.
- Draft commit message: `feat(settings): apply mica with acrylic fallback`

### Task 4: Custom 32 px title strip with min/max/close

- Files touched: `apps/desktop-tauri/src/settings/components/TitleBar.tsx` (new), `apps/desktop-tauri/src/settings/SettingsApp.tsx`, `apps/desktop-tauri/src-tauri/src/windows/settings.rs`, `apps/desktop-tauri/src-tauri/Cargo.toml`.
- What changes: render a 32 px tall `<header>` at the top with `-webkit-app-region: drag`. Place the app icon (16x16) and the title "CodexBar Preferences" on the left. On the right place three buttons using Segoe Fluent Icons codepoints: Minimize ``, Maximize `` (toggles to Restore ``), Close ``. Buttons set `-webkit-app-region: no-drag`. Wire each button to `window.minimize()`, `window.toggleMaximize()`, `window.close()` via Tauri's window API.
- Acceptance check: window has no native title bar. The custom strip drags the window. Min/max/close work. Snap layouts (hover over Maximize) show the Win11 zone picker. Close hides the window (does not quit the app).
- Draft commit message: `feat(settings): add custom 32px title strip`

### Task 5: Sidebar component with selection state and keyboard nav

- Files touched: `apps/desktop-tauri/src/settings/components/Sidebar.tsx` (new), `apps/desktop-tauri/src/settings/SettingsApp.tsx`, `apps/desktop-tauri/src/settings/styles/sidebar.css` (new).
- What changes: replace the inline sidebar from Task 2 with a proper component. Each row is 36 px tall with a 4 px accent bar on the left when selected. Icons from Segoe Fluent: General ``, Providers ``, Display ``, Notifications ``, Shortcuts ``, Advanced ``, About ``. ArrowUp / ArrowDown move selection. Enter and Space activate. Selection writes to local state, not yet to a settings key.
- Acceptance check: clicking and arrow-key navigation both work. Selected row has accent bar and `--surface-selected` background. Focus ring is 2 px accent + 2 px halo, not the OS dotted ring.
- Draft commit message: `feat(settings): sidebar with selection and keyboard nav`

### Task 6: Settings persistence bridge (`SettingsBridge`, `set_setting`, `get_setting`)

- Files touched: `apps/desktop-tauri/src-tauri/src/settings_cmds.rs` (new), `apps/desktop-tauri/src-tauri/src/lib.rs`, `apps/desktop-tauri/src/settings/lib/bridge.ts` (new), `apps/desktop-tauri/src/settings/lib/keys.ts` (new), `rust/codexbar-core/src/config.rs`.
- What changes: define a typed `SettingsKey` enum in `keys.ts` listing every key in spec 20 section 10.4 plus the new keys from PR #918 (`quotaWarningMarkersVisible`). In Rust, `set_setting(key: String, value: serde_json::Value)` validates the key, writes to the in-memory config, schedules a 350 ms debounced flush to `%APPDATA%\CodexBar4Windows\config.json`. `get_setting(key: String) -> serde_json::Value` reads from memory. `subscribe_settings(keys: Vec<String>) -> Channel<SettingsChanged>` emits on every mutation. Bridge module in TS wraps `invoke()` and exposes `useSetting(key)` React hook with optimistic state.
- Acceptance check: `useSetting("appLanguage")` returns the persisted value. Mutating it triggers a write to disk after 350 ms (measured: `Get-Item config.json | select LastWriteTime`). Restarting the app preserves the value. Concurrent writes coalesce into one disk write.
- Draft commit message: `feat(settings): typed bridge for settings get/set/subscribe`

### Task 7: General pane controls

- Files touched: `apps/desktop-tauri/src/settings/panes/General.tsx`, `apps/desktop-tauri/src/settings/components/PreferenceToggleRow.tsx` (new), `apps/desktop-tauri/src/settings/components/PickerRow.tsx` (new).
- What changes: render three sections per spec 20 section 3. System: Language picker (`appLanguage`, options `system`, `en`, `zh-Hans`, `pt-BR`) and Launch at sign-in switch (`launchAtLogin`). Usage / cost: Show cost summary switch (`tokenCostUsageEnabled`). Automation: Refresh cadence dropdown (`refreshFrequency`, options Manual/1m/2m/5m/15m/30m, default 5 min), Check provider status switch (`statusChecksEnabled`), Session quota notifications switch (`sessionQuotaNotificationsEnabled`), Quota warning notifications switch (`quotaWarningNotificationsEnabled`). Add Hide quota warning markers as a separate switch wired to `quotaWarningMarkersVisible` (inverted: UI label "Show quota warning markers", default true). Footer: right-aligned "Quit CodexBar" button with inline popover confirmation.
- Acceptance check: every control reads and writes its setting key; refresh cadence change immediately changes the next refresh timer (visible in tray icon spinner cadence); language change re-mounts the settings window. Launch at sign-in switch is wired to a stub `set_launch_at_login(bool)` command (full implementation in Task 16). Quit button confirmation popover renders inline, not modal.
- Draft commit message: `feat(settings): general pane controls`

### Task 8: Providers pane sidebar list

- Files touched: `apps/desktop-tauri/src/settings/panes/Providers.tsx`, `apps/desktop-tauri/src/settings/components/ProviderSidebar.tsx` (new), `apps/desktop-tauri/src/settings/components/ProviderRow.tsx` (new), `apps/desktop-tauri/src-tauri/src/settings_cmds.rs`.
- What changes: a 280 px wide rounded card on the left listing all enabled-in-build providers from `ProviderRegistry`. Each row is 56 px: drag handle (6-dot grid), 20x20 brand icon, two-line text (display name + subtitle status), trailing enable switch. Subtitle reads from provider state: "Updated 12m ago" / "Last fetch failed" / "Usage not fetched yet" / "Disabled". Status dot color follows `statusChecksEnabled`. Refresh spinner appears when row is actively refreshing. Search input at the top (Ctrl+F focuses) filters by display name and CLI name. Drag-to-reorder writes to `providers[]` array order via `set_provider_order(order: Vec<String>)`.
- Acceptance check: list renders all providers from the build. Drag-reorder commits on drop and persists. Search filters live. Enable toggle flips the provider's `enabled` flag and triggers a tray icon repaint within 350 ms.
- Draft commit message: `feat(settings): providers sidebar list with reorder`

### Task 9: Providers pane detail panel (header, info grid, usage block)

- Files touched: `apps/desktop-tauri/src/settings/panes/Providers.tsx`, `apps/desktop-tauri/src/settings/components/ProviderDetail.tsx` (new), `apps/desktop-tauri/src/settings/components/ProviderHeader.tsx` (new), `apps/desktop-tauri/src/settings/components/InfoGrid.tsx` (new), `apps/desktop-tauri/src/settings/components/UsageBlock.tsx` (new), `apps/desktop-tauri/src/settings/components/ErrorCard.tsx` (new).
- What changes: right-side detail panel with max content width 640 px. Sticky header card: 28x28 brand icon, display name, subtitle, refresh icon button, enable switch. Info grid: label/value rows with auto-sized label column for State, Source, Version, Updated, Status, Account, Plan/Balance. Usage block per-metric: title, horizontal usage bar, percent label, reset text, pace badge. Error card collapsible with "Show details" expander and clipboard copy button. Empty states: "Disabled, no recent data" and "No usage yet".
- Acceptance check: selecting Claude in the sidebar renders the header with the Claude icon, the info grid filled from current state, and the usage block showing session/weekly bars. Triggering a refresh shows the spinner. A simulated error renders the error card with copy button working.
- Draft commit message: `feat(settings): provider detail header, info grid, usage block`

### Task 10: Providers pane detail panel (settings section: per-provider catalog)

- Files touched: `apps/desktop-tauri/src/settings/components/ProviderSettingsSection.tsx` (new), `apps/desktop-tauri/src/settings/components/rows/PickerRow.tsx`, `apps/desktop-tauri/src/settings/components/rows/FieldRow.tsx` (new), `apps/desktop-tauri/src/settings/components/rows/SecureFieldRow.tsx` (new), `apps/desktop-tauri/src/settings/components/rows/ActionsRow.tsx` (new), `apps/desktop-tauri/src/settings/data/providerCatalog.ts` (new).
- What changes: the per-provider settings catalog from spec 20 section 5 ported into a single TS data file describing each provider's settings rows (key, type, default, validation, hidden-when, depends-on, storage). The detail panel renders rows by iterating this catalog. Rows: PickerRow (auth source picker with options auto/oauth/cli/web/api), FieldRow (workspace ID, region), SecureFieldRow (cookie header textarea, API key), ActionsRow (OAuth login button, Sign-in button, Test connection button). Validation feedback inline below the field. Saving indicator at the top right of the header card flashes to a check.
- Acceptance check: opening Claude's detail panel shows source picker, cookie source picker, cookie header textarea (only when manual), peakHoursEnabled toggle. Opening Codex shows source picker, OpenAI web access toggle, battery saver sub-toggle. Opening Copilot shows the device-flow login button as the Add Account primary. Editing the Moonshot region dropdown writes `providers.moonshot.region`. Pasting a Netscape cookie file shows the friendly inline error.
- Draft commit message: `feat(settings): per-provider catalog rows`

### Task 11: Providers pane detail panel (token accounts, Codex Accounts subsection, quota warnings, options)

- Files touched: `apps/desktop-tauri/src/settings/components/TokenAccountsRow.tsx` (new), `apps/desktop-tauri/src/settings/components/CodexAccountsSection.tsx` (new), `apps/desktop-tauri/src/settings/components/QuotaWarningsSection.tsx` (new), `apps/desktop-tauri/src/settings/components/OptionsSection.tsx` (new).
- What changes: Token Accounts row with header, optional primary "Add Account" button (Copilot only: device-flow), account list with radio dot for active, inline add form, "Open token file" and "Reload" footer links. Codex Accounts subsection (only when provider == codex): active picker, system picker, account rows with Re-auth and Remove, Add Account button, Remove confirm modal. Quota warnings section: customize Session, customize Weekly, threshold fields (Upper, Lower, 1-99). Options section: toggle list per spec 20 with on-state status text and inline actions.
- Acceptance check: adding a Claude cookie token account writes a new entry to `providers.claude.tokenAccounts`. Selecting an account flips the active radio. "Open token file" opens `%APPDATA%\CodexBar4Windows\config.json` in the default editor via `ShellExecute`. Codex "Add Account" launches the managed-account OAuth flow. Per-provider quota threshold edits commit on blur and update the usage bar markers within 350 ms.
- Draft commit message: `feat(settings): token accounts, codex accounts, quota warnings`

### Task 12: Display pane

- Files touched: `apps/desktop-tauri/src/settings/panes/Display.tsx`, `apps/desktop-tauri/src/settings/components/OverviewProviderPicker.tsx` (new).
- What changes: section 1 (Tray icon) controls: Merge icons into one tray button (`mergeIcons`), Switcher shows brand icons (`switcherShowsIcons`, gated on mergeIcons), Auto-pick highest-usage provider (`menuBarShowsHighestUsage`, gated on mergeIcons), Show brand icon + percent (`menuBarShowsBrandIconWithPercent`), Display mode picker (`menuBarDisplayMode`: percent / percentDimmedBars / barsOnly / iconOnly). Section 2 (Menu content) controls: Show usage as used (`usageBarsShowUsed`), Show quota warning markers (`quotaWarningMarkersVisible`), Show reset time as absolute clock (`resetTimesShowAbsolute`), Show credits and extra usage (`showOptionalCreditsAndExtraUsage`), Multi-account layout picker (`multiAccountMenuLayout`: segmented or stacked), Overview tab providers configure button popover (max 3, `mergedOverviewSelectedProviders`).
- Acceptance check: toggling Merge icons enables / disables the dependent switches with a 200 ms opacity transition. Display mode picker is disabled unless brand-icon-with-percent is ON. Overview picker enforces max 3 selection with a visible hint. Toggling Show quota warning markers OFF hides tick marks on the tray usage bars and on the in-popup bars within 350 ms.
- Draft commit message: `feat(settings): display pane controls`

### Task 13: Notifications pane

- Files touched: `apps/desktop-tauri/src/settings/panes/Notifications.tsx`, `apps/desktop-tauri/src/settings/components/ThresholdList.tsx` (new), `apps/desktop-tauri/src/settings/components/ThresholdEditor.tsx` (new).
- What changes: top toggle "Enable toasts" (`quotaWarningNotificationsEnabled`). Below: a per-threshold list with default `[50, 20]`; each row shows the integer threshold, with Edit and Remove buttons; "Add threshold" button appends and caps at 2 active. Threshold editor uses an integer input (1-99) with live filter to digits only, commit on blur. Weekly reset celebration toggle (`confettiOnWeeklyLimitResetsEnabled`). Sound toggle (`quotaWarningSoundEnabled`). Session enabled toggle (`quotaWarningSessionEnabled`) and Weekly enabled toggle (`quotaWarningWeeklyEnabled`).
- Acceptance check: setting Upper threshold to 80 writes `quotaWarningThresholds = [80, 20]`. Removing a threshold removes it from the array. Enabling toasts and crossing a threshold during a refresh produces a Windows toast.
- Draft commit message: `feat(settings): notifications pane with threshold editor`

### Task 14: Shortcuts pane with KeyShortcutRecorder

- Files touched: `apps/desktop-tauri/src/settings/panes/Shortcuts.tsx`, `apps/desktop-tauri/src/settings/components/KeyShortcutRecorder.tsx` (new), `apps/desktop-tauri/src-tauri/src/hotkeys.rs` (new), `apps/desktop-tauri/src-tauri/Cargo.toml` (add `tauri-plugin-global-shortcut = "2"`).
- What changes: list of rebindable actions with action id, default chord, current chord, recorder button, Reset button, Disable trash button. Actions: `openMenu` default `Win+Shift+U`, `refreshNow` default `Ctrl+R` (in-app only), `quickSwitchProvider1` through `quickSwitchProvider9` default `Ctrl+1` through `Ctrl+9` (in-app only). The recorder UI shows "Recording..." placeholder, captures the next chord, Esc cancels. On commit, invokes `register_hotkey(action, chord)` Tauri command. Conflicts surface inline as red text "Already used by Refresh now" or "Already in use by another app". On register failure for `openMenu`, fall back to `Ctrl+Alt+U` and surface a yellow info banner.
- Acceptance check: pressing the recorder for `openMenu` and pressing Win+Shift+J binds to that combo and unbinds Win+Shift+U. Pressing Win+Shift+U globally toggles the popup. Pressing Esc during recording cancels. Setting a chord already used by `refreshNow` shows the inline conflict.
- Draft commit message: `feat(shortcuts): global hotkey recorder and registry`

### Task 15: Advanced pane

- Files touched: `apps/desktop-tauri/src/settings/panes/Advanced.tsx`, `apps/desktop-tauri/src/settings/components/DebugLogLevelPicker.tsx` (new), `apps/desktop-tauri/src-tauri/src/settings_cmds.rs`.
- What changes: CLI install button group (status text right of button). Misc switches: Show debug settings (`debugMenuEnabled`), Random blink (`randomBlinkEnabled`), Confetti (`confettiOnWeeklyLimitResetsEnabled`). Privacy switches: Hide personal info (`hidePersonalInfo`), Show provider storage usage (`providerStorageFootprintsEnabled`). Security: Disable secret access switch (`debugDisableKeychainAccess`). Debug logging: file logging switch with subtitle showing `%LOCALAPPDATA%\CodexBar4Windows\Logs\codexbar.log`, verbosity picker `verbose/info/notice/warn/error` default `info`, "Reveal log folder" button that runs `ShellExecute(open, log_folder)`.
- Acceptance check: toggling Show debug settings reveals the Debug pane (Phase 8 ships the toggle wiring; the Debug pane itself stays out of v1.0 per phase scope, leaving the room item set to the seven panes listed in the spec). Toggling Disable secret access shows the explanatory toast. Reveal log folder opens Explorer at the right path.
- Draft commit message: `feat(settings): advanced pane controls`

### Task 16: Launch at sign-in registry implementation

- Files touched: `apps/desktop-tauri/src-tauri/src/launch_at_login.rs` (new), `apps/desktop-tauri/src-tauri/Cargo.toml` (add `winreg = "0.52"`), `apps/desktop-tauri/src-tauri/src/main.rs`, `apps/desktop-tauri/src-tauri/src/lib.rs`.
- What changes: implement `set_launch_at_login(enabled: bool) -> Result<()>` writing or deleting `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\CodexBar4Windows`. When enabled, value is `"<exe-path>" --hidden` where `<exe-path>` is resolved via `std::env::current_exe()`. Implement `is_launch_at_login_enabled() -> bool`. Recognize `--hidden` in `main()` and call `app.run_iteration()` without showing the popup; the tray icon still paints. The Tauri command bridges the settings key `launchAtLogin` to this Rust module.
- Acceptance check: toggling Launch at sign-in ON in General creates the registry entry (`reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run`). After a reboot, the app is in the tray, no window opens, splash never paints. Toggling OFF removes the entry.
- Draft commit message: `feat(launch): registry-backed launch at sign-in`

### Task 17: About pane

- Files touched: `apps/desktop-tauri/src/settings/panes/About.tsx`, `apps/desktop-tauri/src/settings/components/LinkRow.tsx` (new), `apps/desktop-tauri/src-tauri/src/updater.rs` (new), `apps/desktop-tauri/src-tauri/Cargo.toml` (add tauri-plugin-updater).
- What changes: centered vertical stack. App icon 92x92 with rounded corners; hover scale 1.05 (matches spec 80). Title "CodexBar4Windows". Version line pulled from `tauri::app_handle().package_info().version` and Cargo build metadata. Build timestamp from `vergen` env var `VERGEN_BUILD_TIMESTAMP`. Tagline. Link rows: GitHub (https://github.com/JRub/CodexBar4Windows), Upstream fork attribution row "Forked from steipete/CodexBar (MIT)", Donation "Buy me a coffee" link, Email. Auto-update group: "Check for updates automatically" switch (`autoUpdateEnabled`, default true), Update channel picker (`updateChannel`: stable / beta) with description text, "Check for updates" button. Changelog link opens GitHub releases. "Re-run onboarding" button wires to Task 22.
- Acceptance check: clicking the app icon opens the GitHub repo in the default browser. Switching channel from stable to beta immediately fires a Tauri updater check and surfaces "Update available" or "Up to date". "Check for updates" calls the same flow on demand.
- Draft commit message: `feat(settings): about pane with updater channel`

### Task 18: Updater wiring for channel filter

- Files touched: `apps/desktop-tauri/src-tauri/src/updater.rs`, `apps/desktop-tauri/src-tauri/tauri.conf.json`, `.github/workflows/release.yml` (if it exists from Phase 9 scaffolding, otherwise leave a TODO comment in `docs/windows/plan/phase-9-release.md`).
- What changes: configure Tauri updater with two endpoints picked by channel: `https://github.com/JRub/CodexBar4Windows/releases/latest/download/latest-stable.json` and `latest-beta.json`. Each endpoint is a manifest pointing at the latest signed installer for that channel. Implement `set_update_channel(channel)` which rewrites the updater endpoint at runtime via `Update::builder().endpoints(...)` and triggers a fresh check. Pre-release detection: if `version` from `Cargo.toml` contains `alpha`, `beta`, `rc`, `pre`, or `dev`, default the channel to `beta` on first run.
- Acceptance check: with channel `stable`, `tauri-plugin-updater` queries the stable manifest URL. Switching to beta queries the beta URL. On a pre-release build, first run defaults to beta.
- Draft commit message: `feat(updater): stable/beta channel manifest selection`

### Task 19: i18n wiring (en, zh-Hans, pt-BR)

- Files touched: `apps/desktop-tauri/src/i18n/index.ts` (new), `apps/desktop-tauri/src/i18n/en.json` (new), `apps/desktop-tauri/src/i18n/zh-Hans.json` (new), `apps/desktop-tauri/src/i18n/pt-BR.json` (new), every settings TSX file (replace hardcoded strings with `t("key")` calls).
- What changes: minimal i18n provider via React context with a `useT()` hook. Dictionaries cover every string surfaced by the seven panes plus onboarding. Use the same key naming convention as the mac Localizable.strings (`tab_general`, `refresh_cadence_title`, etc.). Loader picks `appLanguage` setting; on `system` resolves via `GetUserPreferredUILanguages` (Tauri command `get_system_locale`). Language change forces a remount of the settings window by toggling a React key.
- Acceptance check: switching language to zh-Hans flips every visible string. Switching to system on a Brazilian Windows install renders pt-BR. Missing keys fall back to en with a console warning.
- Draft commit message: `feat(i18n): en, zh-Hans, pt-BR for preferences`

### Task 20: First-run detection and welcome toast

- Files touched: `apps/desktop-tauri/src-tauri/src/onboarding.rs` (new), `apps/desktop-tauri/src-tauri/src/main.rs`, `apps/desktop-tauri/src/onboarding/Welcome.tsx` (new).
- What changes: at app start, check if `%APPDATA%\CodexBar4Windows\config.json` exists. If not, or if `onboardingCompleted == false`, set a flag `onboarding_active`. After the tray icon paints, post a Windows toast via `tauri-plugin-notification`: title "CodexBar4Windows lives in the tray", body "Pin the icon so it stays visible.", action button "Show me how" that opens `ms-settings:taskbar` via `ShellExecute`. The toast does not appear again on subsequent launches.
- Acceptance check: with `config.json` deleted, launching the app posts the toast within 2 seconds. Clicking "Show me how" opens Windows Taskbar Settings. Relaunching with `onboardingCompleted = true` posts no toast.
- Draft commit message: `feat(onboarding): first-run welcome toast`

### Task 21: Onboarding popup steps (provider picker, per-provider sign-in)

- Files touched: `apps/desktop-tauri/src/onboarding/OnboardingApp.tsx` (new), `apps/desktop-tauri/src/onboarding/Step1Welcome.tsx` (new), `apps/desktop-tauri/src/onboarding/Step2Providers.tsx` (new), `apps/desktop-tauri/src/onboarding/Step3SignIn.tsx` (new), `apps/desktop-tauri/src/onboarding/Step4Done.tsx` (new), `apps/desktop-tauri/src-tauri/src/onboarding.rs`.
- What changes: when `onboarding_active` is true, the popup auto-opens centered on the primary monitor (not anchored to the tray). It renders `OnboardingApp` which switches between four steps. Step 1: large brand mark + "Welcome to CodexBar4Windows. Let's get you set up." + Next. Step 2: provider picker with checkbox list; pre-checked Claude and Codex; user toggles Cursor, Copilot, Gemini, OpenRouter, Factory. Step 3: per-enabled-provider mini-card with auth source picker (Auto / OAuth / CLI / Web / API depending on what the provider supports) and a "Sign in" primary button that runs the provider's OAuth or device-flow or opens the cookie textarea inline. Step 4: "You're all set. The tray icon shows your usage. Right-click for more options." + "Open Preferences" + "Done". On Done, write `onboardingCompleted = true` and dismiss the popup.
- Acceptance check: with `config.json` deleted, after the welcome toast the popup auto-opens centered. Walking through the four steps writes provider enables, source modes, and tokens. After Done, `onboardingCompleted = true`, the popup closes, and the tray icon starts showing live usage within 60 seconds.
- Draft commit message: `feat(onboarding): provider picker and per-provider signin`

### Task 22: "Re-run onboarding" button in About pane

- Files touched: `apps/desktop-tauri/src/settings/panes/About.tsx`, `apps/desktop-tauri/src-tauri/src/onboarding.rs`.
- What changes: a footer-row button "Run onboarding again" that sets `onboardingCompleted = false`, closes the settings window, and opens the popup centered on the primary monitor with the onboarding flow active. Does not delete provider settings.
- Acceptance check: clicking the button after first-run re-opens the four-step flow without losing existing tokens.
- Draft commit message: `feat(onboarding): re-run from about pane`

### Task 23: Live-apply event bus

- Files touched: `apps/desktop-tauri/src-tauri/src/settings_cmds.rs`, `apps/desktop-tauri/src-tauri/src/lib.rs`, `apps/desktop-tauri/src/tray/iconController.ts` (existing), `apps/desktop-tauri/src/popup/PopupApp.tsx` (existing).
- What changes: every settings mutation emits `settings:changed` via Tauri event system with `{ key, value }`. The tray icon controller subscribes to the keys it cares about: `mergeIcons`, `menuBarDisplayMode`, `menuBarShowsBrandIconWithPercent`, `menuBarShowsHighestUsage`, `quotaWarningMarkersVisible`, `usageBarsShowUsed`, `selectedMenuProvider`. The popup subscribes to provider-related keys. Each subscriber repaints within 350 ms of the mutation landing.
- Acceptance check: toggling Merge icons OFF in settings causes the tray to render one icon per enabled provider within 350 ms. Toggling Show quota warning markers OFF hides the tick marks across tray and popup at the same time.
- Draft commit message: `feat(settings): live-apply event bus`

### Task 24: Validation feedback (inline error, field shake, saving indicator)

- Files touched: `apps/desktop-tauri/src/settings/components/InlineError.tsx`, `apps/desktop-tauri/src/settings/components/FieldShake.tsx`, `apps/desktop-tauri/src/settings/components/SavingIndicator.tsx`.
- What changes: InlineError renders 12 px text with a small alert icon below the offending field. FieldShake wraps a field and applies a 280 ms shake animation when the validate prop becomes false. SavingIndicator shows a small "Saving..." pill that fades to a check after the debounce flush lands. Wired into every secure field and threshold input.
- Acceptance check: pasting a Netscape cookie file into the Claude cookie textarea shows the inline error AND the shake. Editing a quota threshold to 150 reverts to the last good value after 1.5 s and shows the inline hint "1-99".
- Draft commit message: `feat(settings): validation feedback components`

### Task 25: Accessibility audit pass

- Files touched: every settings TSX file, `apps/desktop-tauri/src/settings/SettingsApp.tsx`, `apps/desktop-tauri/src/onboarding/OnboardingApp.tsx`.
- What changes: ensure every interactive element has an `aria-label` or visible label; every icon-only button has an `aria-label`; tab order goes Sidebar -> Title strip controls -> Content area; focus rings visible on every focusable element; sidebar uses `role="navigation"` and `role="list"` with `role="listitem"`; pane content uses `role="main"`; the title strip uses `role="banner"`. Verify with Narrator: open settings, navigate by Tab and arrows, every control announces. Color contrast verified at 4.5:1 minimum via the Color Contrast Analyzer.
- Acceptance check: Narrator (Ctrl+Win+Enter) reads each sidebar item and pane heading. Tab cycles through every control without traps. Color Contrast Analyzer reports no failures.
- Draft commit message: `chore(settings): accessibility audit pass`

### Task 26: Window state persistence (last size, last position, last pane)

- Files touched: `apps/desktop-tauri/src-tauri/src/windows/settings.rs`, `apps/desktop-tauri/src/settings/SettingsApp.tsx`.
- What changes: on window close, persist `(monitor_id, x, y, w, h, lastPane)` to `%APPDATA%\CodexBar4Windows\windowState.json`. On open, read back and apply, clamping into the nearest visible monitor. Default if no state: 880x640 centered on the primary monitor with General pane.
- Acceptance check: resize the settings window to 1100x720, switch to Display pane, close. Reopen: 1100x720, Display pane selected. Unplug the external monitor where the window was last positioned: window comes up on the remaining monitor.
- Draft commit message: `feat(settings): persist window state across sessions`

### Task 27: Telemetry event bus (off by default)

- Files touched: `apps/desktop-tauri/src-tauri/src/telemetry.rs` (new), `apps/desktop-tauri/src/settings/SettingsApp.tsx`, `apps/desktop-tauri/src/onboarding/OnboardingApp.tsx`.
- What changes: a Rust module that records events into a ring buffer (no network egress). Wired into `prefs_opened`, `prefs_pane_switched`, `onboarding_started`, `onboarding_step_completed`, `onboarding_finished`, `hotkey_registered`, `hotkey_conflict`, `launch_at_login_enabled`, `launch_at_login_disabled`. A settings key `telemetryEnabled` defaults to false; only when true does the ring buffer fill. Backend wiring deferred to a later phase.
- Acceptance check: with `telemetryEnabled` true, walking through the settings window emits one `prefs_pane_switched` per pane click. With it false, no events fill the buffer.
- Draft commit message: `feat(telemetry): event bus for prefs and onboarding`

### Task 28: Unit and integration tests

- Files touched: `rust/codexbar-core/tests/config_settings.rs` (new), `apps/desktop-tauri/src-tauri/tests/hotkeys.rs` (new), `apps/desktop-tauri/src-tauri/tests/launch_at_login.rs` (new), `apps/desktop-tauri/src/settings/__tests__/SettingsApp.test.tsx` (new), `apps/desktop-tauri/src/onboarding/__tests__/OnboardingApp.test.tsx` (new).
- What changes: Rust tests cover the debounced settings write (assert one disk write after N rapid mutations), the registry write for launch-at-login (using a `winreg` mock or `wineventlog` capture), the hotkey conflict detector. React tests cover sidebar keyboard nav, threshold editor validation, onboarding step transitions. Run via `cargo test` and `npm test`.
- Acceptance check: `cargo test --workspace` and `npm test` both pass on `windows-latest` CI.
- Draft commit message: `test: cover settings, hotkeys, launch-at-login, onboarding`

### Task 29: CI gate updates

- Files touched: `.github/workflows/ci.yml`.
- What changes: extend the existing windows-latest CI matrix to run `npm test` in addition to `npm run tauri build`. Add a job that runs `npx playwright test` against the built `apps/desktop-tauri/dist/` to smoke-test the settings window opens and the seven panes render without errors. Add a check that `quotaWarningMarkersVisible` defaults to true (regression guard for PR #918).
- Acceptance check: a green CI run on a clean push of this phase's branch.
- Draft commit message: `ci: extend gates for settings smoke and unit tests`

### Task 30: Phase 8 acceptance run-through and changelog entry

- Files touched: `CHANGELOG.md`, optionally `docs/windows/plan/phase-8-prefs-onboarding-hotkeys.md` (this file) for any post-implementation corrections.
- What changes: add a CHANGELOG entry under `## [Unreleased]` summarizing Phase 8 in three lines: "Preferences window with seven panes (General, Providers, Display, Notifications, Shortcuts, Advanced, About)", "First-run onboarding flow with provider picker and per-provider sign-in", "Global hotkey Win+Shift+U with rebinder; Launch at sign-in; Stable/Beta update channel". Walk through the phase acceptance tests below on a clean Windows 11 VM. File any unresolved items as GitHub issues with `phase-9-blocker` label.
- Acceptance check: changelog entry merged; phase acceptance run-through completed; no open issues tagged `phase-8-blocker`.
- Draft commit message: `docs(changelog): phase 8 preferences, onboarding, hotkeys`

### Task 31: Sidebar search across all panes

- Files touched: `apps/desktop-tauri/src/settings/components/Sidebar.tsx`, `apps/desktop-tauri/src/settings/data/searchIndex.ts` (new), `apps/desktop-tauri/src/settings/SettingsApp.tsx`.
- What changes: a search input at the top of the sidebar (Ctrl+F focuses) filters the pane list by name; if any pane title matches, it floats to the top of the list. Below the filtered panes, a "Found in settings" section lists individual settings rows whose label matches. Clicking a row jumps to the pane and scrolls to the row, briefly highlighting it with a 600 ms fade.
- Acceptance check: typing "language" in the sidebar search highlights General as the top match and lists "Language" under the Found in settings header. Clicking the row jumps to General and scrolls the Language row into view.
- Draft commit message: `feat(settings): sidebar search across panes`

### Task 32: Cross-pane "see also" rails

- Files touched: `apps/desktop-tauri/src/settings/components/SeeAlso.tsx` (new), `apps/desktop-tauri/src/settings/data/relatedSettings.ts` (new), `apps/desktop-tauri/src/settings/panes/General.tsx`, `apps/desktop-tauri/src/settings/panes/Display.tsx`, `apps/desktop-tauri/src/settings/panes/Notifications.tsx`, `apps/desktop-tauri/src/settings/panes/Advanced.tsx`.
- What changes: at the bottom of every pane, render a "See also" rail with two or three deep-links to related settings. Mapping defined in `relatedSettings.ts`: General points to Display (icon style) and Shortcuts (Open menu). Display points to Notifications (markers visibility) and General (refresh cadence). Notifications points to General (quota warning notifications). Advanced points to About (update channel) and Shortcuts (rebind).
- Acceptance check: scrolling to the bottom of General shows two clickable "See also" cards. Clicking one navigates to the linked pane and scrolls to the linked row.
- Draft commit message: `feat(settings): cross-pane see-also rails`

### Task 33: Restore-defaults action per pane

- Files touched: `apps/desktop-tauri/src/settings/components/RestoreDefaults.tsx` (new), every pane TSX file, `apps/desktop-tauri/src-tauri/src/settings_cmds.rs`.
- What changes: each pane has a small "Restore pane defaults" link in the bottom right. Clicking it opens an inline confirm popover listing the keys that will be reset. Pressing Restore writes the per-key defaults back. Provider-scoped settings are not touched by General's restore.
- Acceptance check: changing several General settings then clicking Restore reverts them all without touching any provider's `enabled` flag.
- Draft commit message: `feat(settings): per-pane restore defaults`

## 5. Settings keys reference (Phase 8 scope)

Every settings key Phase 8 reads or writes. Bold keys are new in Phase 8; the rest were defined in earlier phases and surfaced here.

| Key | Type | Default | Storage |
|---|---|---|---|
| `appLanguage` | enum (system, en, zh-Hans, pt-BR) | system | defaults |
| `launchAtLogin` | bool | false | defaults + Run registry |
| `refreshFrequency` | enum (manual, 1m, 2m, 5m, 15m, 30m) | 5m | defaults |
| `statusChecksEnabled` | bool | true | defaults |
| `sessionQuotaNotificationsEnabled` | bool | true | defaults |
| `quotaWarningNotificationsEnabled` | bool | false | defaults |
| `quotaWarningThresholds` | int[] | [50, 20] | defaults |
| `quotaWarningSessionEnabled` | bool | true | defaults |
| `quotaWarningWeeklyEnabled` | bool | true | defaults |
| `quotaWarningSoundEnabled` | bool | true | defaults |
| `quotaWarningMarkersVisible` | bool | true | defaults |
| `tokenCostUsageEnabled` | bool | false | defaults |
| `mergeIcons` | bool | true | defaults |
| `switcherShowsIcons` | bool | true | defaults |
| `menuBarShowsHighestUsage` | bool | false | defaults |
| `menuBarShowsBrandIconWithPercent` | bool | false | defaults |
| `menuBarDisplayMode` | enum (percent, percentDimmedBars, barsOnly, iconOnly) | percent | defaults |
| `usageBarsShowUsed` | bool | false | defaults |
| `resetTimesShowAbsolute` | bool | false | defaults |
| `showOptionalCreditsAndExtraUsage` | bool | true | defaults |
| `multiAccountMenuLayout` | enum (segmented, stacked) | segmented | defaults |
| `mergedOverviewSelectedProviders` | string[] | first 3 active | defaults |
| `mergedOverviewSelectionEditedActiveProviders` | string[] | empty | defaults |
| `confettiOnWeeklyLimitResetsEnabled` | bool | false | defaults |
| `randomBlinkEnabled` | bool | false | defaults |
| `hidePersonalInfo` | bool | false | defaults |
| `providerStorageFootprintsEnabled` | bool | false | defaults |
| `debugDisableKeychainAccess` | bool | false | defaults |
| `debugMenuEnabled` | bool | false | defaults |
| `debugFileLoggingEnabled` | bool | false | defaults |
| `debugLogLevel` | enum (verbose, info, notice, warn, error) | info | defaults |
| `autoUpdateEnabled` | bool | true | defaults |
| `updateChannel` | enum (stable, beta) | stable (beta on prerelease builds) | defaults |
| **`onboardingCompleted`** | bool | false | defaults |
| **`hotkeyBindings`** | map of action id -> chord | `{ "openMenu": "Win+Shift+U" }` | defaults |
| **`windowState.settings`** | object (monitor, x, y, w, h, pane) | absent | windowState.json |
| **`telemetryEnabled`** | bool | false | defaults |
| `providers[<id>].enabled` | bool | per provider default | config |
| `providers[<id>].source` | enum | auto | config |
| `providers[<id>].cookieSource` | enum (auto, manual, off) | auto | config |
| `providers[<id>].cookieHeader` | secure string | empty | config (DPAPI) |
| `providers[<id>].apiKey` | secure string | empty | config (DPAPI) |
| `providers[<id>].region` | enum | per provider default | config |
| `providers[<id>].workspaceID` | string | empty | config |
| `providers[<id>].tokenAccounts` | TokenAccount[] | empty | config (DPAPI per account) |
| `providers[<id>].quotaWarnings` | optional override | inherits global | config |

## 6. Hotkey action ids reference

| Action id | Default chord | Global / local | Rebindable | Notes |
|---|---|---|---|---|
| `openMenu` | Win+Shift+U | global | yes | Registered via `tauri-plugin-global-shortcut`. Fallback `Ctrl+Alt+U` if conflict. |
| `refreshNow` | Ctrl+R | local (popup focused) | yes | Triggers a fresh refresh of the currently visible provider. |
| `quickSwitchProvider1` | Ctrl+1 | local | yes | Selects first enabled provider in tray switcher. |
| `quickSwitchProvider2` | Ctrl+2 | local | yes | Selects second. |
| `quickSwitchProvider3` | Ctrl+3 | local | yes | Selects third. |
| `quickSwitchProvider4` | Ctrl+4 | local | yes | Selects fourth. |
| `quickSwitchProvider5` | Ctrl+5 | local | yes | Selects fifth. |
| `quickSwitchProvider6` | Ctrl+6 | local | yes | Selects sixth. |
| `quickSwitchProvider7` | Ctrl+7 | local | yes | Selects seventh. |
| `quickSwitchProvider8` | Ctrl+8 | local | yes | Selects eighth. |
| `quickSwitchProvider9` | Ctrl+9 | local | yes | Selects ninth. |
| `focusSearch` | Ctrl+F | local | no | Focuses search box in Providers pane and tray switcher. Standard, not rebindable. |
| `quitApp` | (none) | global | yes (opt-in) | Quits the app from anywhere. Unbound by default to avoid foot-guns. |

## 7. IPC event names reference

| Event name | Direction | Payload | Notes |
|---|---|---|---|
| `settings:changed` | core to renderer | `{ key: string, value: any }` | Emitted after every successful mutation. |
| `settings:flushed` | core to renderer | `{ keys: string[] }` | Emitted after a debounced disk write lands. |
| `settings:flush-failed` | core to renderer | `{ keys: string[], error: string }` | Triggers retry toast. |
| `onboarding:start` | core to renderer | `{ reason: "first-run" or "re-run" }` | Tells the popup to open the onboarding flow. |
| `onboarding:step` | renderer to core | `{ step: number, payload: any }` | Records progress. |
| `onboarding:finished` | renderer to core | `{}` | Writes `onboardingCompleted = true`. |
| `hotkey:fired` | core to renderer | `{ action: string }` | Emitted when a registered global chord fires. |
| `hotkey:conflict` | core to renderer | `{ action: string, chord: string }` | Emitted when `register_hotkey` fails. |
| `updater:check-started` | core to renderer | `{}` | About pane shows spinner. |
| `updater:check-finished` | core to renderer | `{ status: "up-to-date" or "update-available" or "error", version?: string, error?: string }` | About pane updates. |
| `tray:repaint-requested` | core to renderer | `{ reason: string }` | Internal; settings event bus uses this to repaint icons after a relevant key changes. |

## 8. File tree introduced by this phase

Concrete paths created or modified in `apps/desktop-tauri/`.

```
apps/desktop-tauri/
  src-tauri/
    Cargo.toml                              modified, adds window-vibrancy, winreg, tauri-plugin-global-shortcut, tauri-plugin-updater
    tauri.conf.json                         modified, adds settings window definition
    src/
      lib.rs                                modified, registers settings_cmds, hotkeys, launch_at_login, onboarding, updater, telemetry
      main.rs                               modified, recognizes --hidden CLI flag
      settings_cmds.rs                      new, get_setting, set_setting, subscribe_settings, set_provider_order
      onboarding.rs                         new, first-run detector, onboarding event emitter
      hotkeys.rs                            new, register_hotkey, unregister_hotkey, list_hotkey_conflicts
      launch_at_login.rs                    new, set_launch_at_login, is_launch_at_login_enabled
      updater.rs                            new, set_update_channel, check_for_updates
      telemetry.rs                          new, ring-buffer telemetry events
      windows/
        settings.rs                         new, open_settings_window, persist window state
    tests/
      hotkeys.rs                            new
      launch_at_login.rs                    new
  src/
    App.tsx                                 modified, routes /settings and /onboarding
    main.tsx                                modified, mounts router
    settings/
      SettingsApp.tsx                       new
      panes/
        General.tsx                         new
        Providers.tsx                       new
        Display.tsx                         new
        Notifications.tsx                   new
        Shortcuts.tsx                       new
        Advanced.tsx                        new
        About.tsx                           new
      components/
        Sidebar.tsx                         new
        TitleBar.tsx                        new
        PreferenceToggleRow.tsx             new
        PickerRow.tsx                       new
        ProviderSidebar.tsx                 new
        ProviderRow.tsx                     new
        ProviderDetail.tsx                  new
        ProviderHeader.tsx                  new
        InfoGrid.tsx                        new
        UsageBlock.tsx                      new
        ErrorCard.tsx                       new
        ProviderSettingsSection.tsx         new
        rows/
          PickerRow.tsx                     new (per-row primitive)
          FieldRow.tsx                      new
          SecureFieldRow.tsx                new
          ActionsRow.tsx                    new
        TokenAccountsRow.tsx                new
        CodexAccountsSection.tsx            new
        QuotaWarningsSection.tsx            new
        OptionsSection.tsx                  new
        OverviewProviderPicker.tsx          new
        ThresholdList.tsx                   new
        ThresholdEditor.tsx                 new
        KeyShortcutRecorder.tsx             new
        DebugLogLevelPicker.tsx             new
        LinkRow.tsx                         new
        InlineError.tsx                     new
        FieldShake.tsx                      new
        SavingIndicator.tsx                 new
        RestoreDefaults.tsx                 new
        SeeAlso.tsx                         new
      data/
        providerCatalog.ts                  new, per-provider row descriptors
        searchIndex.ts                      new, searchable rows
        relatedSettings.ts                  new, see-also map
      lib/
        bridge.ts                           new, SettingsBridge wrapper
        keys.ts                             new, SettingsKey enum
      styles/
        sidebar.css                         new
        panes.css                           new
        rows.css                            new
      __tests__/
        SettingsApp.test.tsx                new
    onboarding/
      OnboardingApp.tsx                     new
      Step1Welcome.tsx                      new
      Step2Providers.tsx                    new
      Step3SignIn.tsx                       new
      Step4Done.tsx                         new
      __tests__/
        OnboardingApp.test.tsx              new
    i18n/
      index.ts                              new
      en.json                               new
      zh-Hans.json                          new
      pt-BR.json                            new
  rust/
    codexbar-core/
      src/
        config.rs                           modified, add debounce flush, onboarding flag
      tests/
        config_settings.rs                  new
```

## 9. Design tokens used by the preferences window

The values below are referenced by the CSS variable system loaded in `apps/desktop-tauri/src/settings/styles/`. They mirror spec 20 section 1 and section 16, normalized to a token table for easy theming.

| Token | Light | Dark | Used for |
|---|---|---|---|
| `--accent` | `#7C5CFF` | `#9C84FF` | Selection bar, focus ring, primary buttons |
| `--accent-hover` | `#6A4AE6` | `#AC94FF` | Primary button hover |
| `--surface-bg` | `#FFFFFF` (over Mica) | `#1B1B1F` (over Mica) | Pane background (semi-transparent over Mica) |
| `--surface-elevated` | `#F5F5F7` | `#2A2A30` | Cards, sidebar elevation |
| `--surface-hover` | `#0000000A` | `#FFFFFF0F` | Row hover fill |
| `--surface-selected` | `#7C5CFF14` | `#9C84FF1F` | Selected sidebar row |
| `--border-subtle` | `#00000014` | `#FFFFFF1A` | Section dividers |
| `--border-strong` | `#0000002E` | `#FFFFFF33` | Field outlines |
| `--border-danger` | `#E5484D` | `#FF6369` | Invalid field outline |
| `--text-primary` | `#1B1B1F` | `#F5F5F7` | Body text |
| `--text-secondary` | `#1B1B1F99` | `#F5F5F7B3` | Subtitle text |
| `--text-tertiary` | `#1B1B1F66` | `#F5F5F780` | Footnote text |
| `--text-success` | `#16A249` | `#3DD68C` | Saving check |
| `--text-warn` | `#C26200` | `#F0A150` | Yellow info banner |
| `--text-error` | `#CC2027` | `#FF6369` | Inline validation error |
| `--radius-card` | `8 px` | `8 px` | Cards, sections |
| `--radius-row` | `6 px` | `6 px` | Hover-highlighted rows |
| `--gap-section` | `24 px` | `24 px` | Vertical gap between sections |
| `--gap-row` | `12 px` | `12 px` | Vertical gap between rows |
| `--switch-track-w` | `36 px` | `36 px` | Toggle track width |
| `--switch-knob-w` | `16 px` | `16 px` | Toggle knob width |
| `--switch-anim-ms` | `180` | `180` | Toggle knob slide duration |
| `--pane-cross-fade-ms` | `200` | `200` | Sidebar pane cross-fade |
| `--saving-fade-in-ms` | `200` | `200` | Saving pill fade-in |
| `--saving-hold-ms` | `1500` | `1500` | Saving check hold |
| `--saving-fade-out-ms` | `400` | `400` | Saving check fade-out |
| `--shake-cycle-ms` | `280` | `280` | Validation error shake |
| `--shake-amplitude-px` | `4` | `4` | Validation error shake X amplitude |

## 10. Microcopy table

All user-visible strings introduced by this phase. Keys match `apps/desktop-tauri/src/i18n/*.json`. Phase 8 ships only the `en` column; zh-Hans and pt-BR translations land via translators in parallel.

| Key | English |
|---|---|
| `tab_general` | General |
| `tab_providers` | Providers |
| `tab_display` | Display |
| `tab_notifications` | Notifications |
| `tab_shortcuts` | Shortcuts |
| `tab_advanced` | Advanced |
| `tab_about` | About |
| `general_section_system` | System |
| `general_section_usage` | Usage |
| `general_section_automation` | Automation |
| `general_language_label` | Language |
| `general_launch_at_login_label` | Launch at sign-in |
| `general_launch_at_login_subtitle` | Open CodexBar4Windows when you sign in. The tray icon shows up; no window opens. |
| `general_refresh_cadence_label` | Refresh cadence |
| `general_refresh_cadence_manual` | Manual |
| `general_status_checks_label` | Check provider status |
| `general_session_quota_notifications_label` | Session quota notifications |
| `general_quota_warning_notifications_label` | Quota warning notifications |
| `general_quit_button` | Quit CodexBar4Windows |
| `general_quit_confirm_title` | Quit CodexBar4Windows? |
| `general_quit_confirm_body` | The tray icon and background fetchers will stop. |
| `display_section_tray` | Tray icon |
| `display_section_menu` | Menu content |
| `display_merge_icons_label` | Merge icons into one tray button |
| `display_switcher_shows_icons_label` | Switcher shows brand icons |
| `display_highest_usage_label` | Auto-pick highest-usage provider |
| `display_brand_icon_with_percent_label` | Show brand icon with percent |
| `display_display_mode_label` | Display mode |
| `display_usage_bars_show_used_label` | Show usage as used |
| `display_quota_markers_label` | Show quota warning markers |
| `display_quota_markers_subtitle` | Turn off to hide tick marks on usage bars across the tray and popup. |
| `display_reset_times_absolute_label` | Show reset time as absolute clock |
| `display_optional_credits_label` | Show credits and extra usage |
| `display_multi_account_layout_label` | Multi-account layout |
| `display_overview_picker_label` | Overview tab providers |
| `display_overview_picker_hint` | Pick up to three providers for the Overview tab. |
| `notifications_enable_toasts_label` | Enable toasts |
| `notifications_thresholds_label` | Thresholds |
| `notifications_thresholds_subtitle` | Get a toast when usage crosses a threshold. |
| `notifications_add_threshold` | Add threshold |
| `notifications_threshold_field_hint` | 1 to 99 |
| `notifications_session_enabled_label` | Session warnings |
| `notifications_weekly_enabled_label` | Weekly warnings |
| `notifications_weekly_celebration_label` | Confetti on weekly reset |
| `notifications_sound_label` | Play sound on warning |
| `shortcuts_open_menu_label` | Open CodexBar4Windows |
| `shortcuts_open_menu_default` | Win+Shift+U |
| `shortcuts_refresh_now_label` | Refresh now |
| `shortcuts_quick_switch_label` | Quick switch provider |
| `shortcuts_recording_placeholder` | Recording. Press the keys, or Esc to cancel. |
| `shortcuts_conflict_text` | Already used by {action}. |
| `shortcuts_app_conflict_text` | In use by another app. Pick another chord. |
| `shortcuts_fallback_banner` | Using fallback chord {chord}. Click to rebind. |
| `shortcuts_reset_link` | Reset to default |
| `shortcuts_disable_link` | Disable |
| `advanced_cli_install_label` | Install CodexBar CLI |
| `advanced_debug_settings_label` | Show debug settings |
| `advanced_random_blink_label` | Random blink animation |
| `advanced_hide_personal_info_label` | Hide personal info in menu |
| `advanced_provider_storage_label` | Show provider storage usage |
| `advanced_disable_secret_access_label` | Disable secret access |
| `advanced_disable_secret_access_subtitle` | Stops CodexBar from reading saved cookies or DPAPI tokens. Paste cookies manually for any provider that needs them. |
| `advanced_file_logging_label` | Enable file logging |
| `advanced_log_level_label` | Logging verbosity |
| `advanced_reveal_log_folder` | Reveal log folder |
| `about_tagline` | Track AI coding usage in your tray. |
| `about_version` | Version {version} (build {build}) |
| `about_built` | Built {date} |
| `about_fork_attribution` | Forked from steipete/CodexBar (MIT). |
| `about_check_for_updates_label` | Check for updates automatically |
| `about_update_channel_label` | Update channel |
| `about_update_channel_stable` | Stable |
| `about_update_channel_beta` | Beta |
| `about_update_channel_stable_desc` | Stable, production-ready releases only. |
| `about_update_channel_beta_desc` | Stable releases plus beta previews. May contain bugs. |
| `about_check_button` | Check for updates |
| `about_donation` | Buy me a coffee |
| `about_changelog_link` | View changelog |
| `about_rerun_onboarding` | Run onboarding again |
| `about_copyright` | Copyright 2026 Peter Steinberger, MIT |
| `onboarding_welcome_title` | Welcome to CodexBar4Windows. |
| `onboarding_welcome_body` | Track your AI coding usage from the tray. Let us set you up in under a minute. |
| `onboarding_welcome_next` | Next |
| `onboarding_providers_title` | Pick your providers. |
| `onboarding_providers_subtitle` | You can change this any time in Settings. |
| `onboarding_signin_title` | Sign in to each provider. |
| `onboarding_signin_subtitle` | Authenticate now or skip and add credentials later. |
| `onboarding_signin_button` | Sign in |
| `onboarding_signin_skip` | Skip for now |
| `onboarding_done_title` | You are set up. |
| `onboarding_done_body` | The tray icon shows your usage. Right-click it for more options. |
| `onboarding_done_open_prefs` | Open Preferences |
| `onboarding_done_button` | Done |
| `welcome_toast_title` | CodexBar4Windows lives in the tray. |
| `welcome_toast_body` | Pin the icon so it stays visible. |
| `welcome_toast_action` | Show me how |
| `saving_pill` | Saving |
| `saved_pill` | Saved |
| `save_failed_toast` | Could not save settings. Retrying. |
| `validation_netscape_cookie` | That looks like a Netscape cookie file. Convert each row to name=value and join them with semicolons. |
| `validation_threshold_range` | Use a value between 1 and 99. |
| `validation_url_scheme` | Use an http or https URL. |
| `restore_defaults_link` | Restore pane defaults |
| `restore_defaults_confirm` | Reset these settings? |

## 11. Wireframe sketches in ASCII

These are deliberately rough so a reviewer can sanity-check the layout before any pixels are drawn.

### 11.1 Settings window, General pane selected

```
+--------------------------------------------------------------------------+
| [icon] CodexBar Preferences                              _   []   X     |  <- 32 px title strip, drag region
+----------------+---------------------------------------------------------+
| [Search    ] | General                                                 |
| > General    |   System                                                |
|   Providers  |     Language          [System          v]               |
|   Display    |     Launch at sign-in [  o ]   (off)                    |
|   Notifications  Usage                                                  |
|   Shortcuts  |     Show cost summary [  o ]   (off)                    |
|   Advanced   |   Automation                                            |
|   About      |     Refresh cadence   [ 5 minutes    v]                 |
|              |     Check provider status      [  o] (on)               |
| ...          |     Session quota notifications [  o] (on)              |
| Quit CodexBar|     Quota warning notifications [ o ] (off)             |
|              |                                                         |
|              |   ... See also: Display, Shortcuts                      |
|              |   Restore pane defaults                                 |
+--------------+---------------------------------------------------------+
```

### 11.2 Providers pane

```
+--------------------------------------------------------------------------+
| [icon] CodexBar Preferences                              _   []   X     |
+----------------+---------------------------------------------------------+
| [Search    ] | [Search providers...]                                    |
|   General    | +-----------------------------+   +---------------------+|
| > Providers  | | :: [C] Claude              o| | [C] Claude            ||
|   Display    | | :: [X] Codex               o| | Updated 4m ago        ||
|   Notifications  | :: [O] OpenRouter         o| | [Refresh] [Enable o]||
|   Shortcuts  | | :: [P] Copilot            o |+---------------------+|
|   Advanced   | | :: [G] Gemini              o| Info                   ||
|   About      | | :: [F] Factory            o | State    Enabled       ||
|              | | :: [I] Cursor              o| Source   OAuth         ||
|              | +-----------------------------+ Version  1.0.18        ||
|              |                                 Usage                   ||
|              |                                 Session     [#####....] ||
|              |                                 Weekly      [##....   ] ||
|              |                                 Settings                ||
|              |                                 Source [Auto    v]      ||
|              |                                 Cookie source [Auto v]  ||
+--------------+---------------------------------------------------------+
```

### 11.3 Onboarding step 2

```
+----------------------------------------------------+
|                                                    |
|        Pick your providers.                        |
|        You can change this any time in Settings.   |
|                                                    |
|        [x] Claude                                  |
|        [x] Codex                                   |
|        [ ] Cursor                                  |
|        [ ] Copilot                                 |
|        [ ] Gemini                                  |
|        [ ] OpenRouter                              |
|        [ ] Factory                                 |
|                                                    |
|                       [Back]   [Next ->]           |
+----------------------------------------------------+
```

## 12. Phase acceptance tests

Run all of these on a clean Windows 11 22H2 VM with no prior CodexBar4Windows install.

1. **Install timing.** Download the latest MSI from the Phase 8 build artifact. Double-click. Click through the installer. Time to first tray icon paint: under 30 seconds.
2. **First-run toast.** Within 5 seconds of the tray icon appearing, a Windows toast appears reading "CodexBar4Windows lives in the tray. Pin the icon so it stays visible." with a "Show me how" action button. Clicking the button opens Windows Taskbar Settings.
3. **Onboarding popup.** The popup auto-opens centered on the primary monitor (not anchored to the tray). It shows the Welcome step.
4. **Provider picker.** Clicking Next moves to the provider picker. Claude and Codex are pre-checked. The user checks Cursor and Copilot.
5. **Per-provider sign-in.** Clicking Next moves to per-provider sign-in. Claude shows a "Sign in with Claude" OAuth button; clicking opens the browser to claude.ai's OAuth consent, returns a token, the step shows a green check. Codex shows a "Sign in with Codex" button; clicking opens the managed-account OAuth flow. Cursor and Copilot use cookie or device-flow respectively.
6. **Done step.** Clicking Done writes `onboardingCompleted = true`, the popup closes, the tray icon starts a refresh, the loading animation runs.
7. **Five-minute test.** From the moment of the MSI double-click to the moment the user sees their Claude usage percent in the tray, total elapsed time is under five minutes. The user did not open the docs.
8. **Preferences pane parity.** Right-click the tray icon, choose Preferences. The settings window opens at 880x640 with Mica background. Sidebar shows General, Providers, Display, Notifications, Shortcuts, Advanced, About. Each pane renders without errors.
9. **General pane live-apply.** Change refresh cadence from 5 min to 1 min. Observe the next refresh fires within 60 seconds. Toggle Launch at sign-in ON; check `reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run`. Switch language to zh-Hans; the entire window re-renders in Chinese.
10. **Providers pane parity.** Reorder providers by drag. The tray switcher reflects the new order within 350 ms. Search "claude" filters to Claude only. Click Claude to see the detail panel. Toggle Claude OFF; the tray icon stops showing Claude usage.
11. **Display pane live-apply.** Toggle Show usage as used; the bars flip direction. Toggle Show quota warning markers OFF; the tick marks disappear from every bar across tray and popup within 350 ms.
12. **Notifications pane.** Set threshold to 80. Force a refresh that returns 79 percent used; a toast appears reading "Claude session at 80% threshold crossed".
13. **Shortcuts pane.** Press the recorder for `openMenu`, press Win+Shift+J, save. The chord rebinds. Pressing Win+Shift+J anywhere toggles the popup. Pressing the old Win+Shift+U does nothing.
14. **Advanced pane.** Toggle Show debug settings ON. Reveal log folder opens Explorer at `%LOCALAPPDATA%\CodexBar4Windows\Logs\`. Disable secret access shows the explanatory toast.
15. **About pane.** Switch update channel from stable to beta; the updater fires a check immediately and the result appears within 5 seconds. Click "Run onboarding again"; the onboarding flow restarts.
16. **Reboot test.** Toggle Launch at sign-in ON. Reboot the VM. Within 60 seconds of sign-in, the tray icon is visible and showing usage. No window opens. No splash appears.
17. **Conflict test.** Open another app that registers Win+Shift+U (e.g. a test PowerShell script using `RegisterHotKey`). Launch CodexBar4Windows. The Shortcuts pane shows the `openMenu` row with a yellow info banner "Win+Shift+U is already in use; using Ctrl+Alt+U as fallback".
18. **Persistence test.** Resize the settings window to 1100x720, switch to Display pane, close. Reopen. The window comes up at 1100x720 on the same monitor with Display selected.
19. **Accessibility test.** Enable Narrator (Ctrl+Win+Enter). Open Preferences. Tab through every control. Every control announces a meaningful name and role. No focus traps. No silent buttons.
20. **i18n test.** Switch system locale to pt-BR. Restart the app. With `appLanguage` set to system, the settings window renders in Portuguese.

## 13. CI gates

The following CI checks must pass before this phase merges to `main`. All checks run on `windows-latest`.

1. `cargo fmt --check` on the workspace.
2. `cargo clippy --workspace --all-targets -- -D warnings`.
3. `cargo test --workspace` with all unit and integration tests passing.
4. `npm install` and `npm test` in `apps/desktop-tauri/` with React Testing Library tests passing.
5. `npm run tauri build` producing a signed MSI artifact (signing keys come from Phase 9; until then the artifact is unsigned).
6. A Playwright smoke test that launches the MSI in headed mode, opens Preferences, navigates each pane, and asserts no console errors.
7. A regression guard test that `quotaWarningMarkersVisible` defaults to true and the Show quota warning markers switch reflects that default.
8. A regression guard test that the seven panes render in the expected order: General, Providers, Display, Notifications, Shortcuts, Advanced, About.
9. Bundle-size check: the built `apps/desktop-tauri/dist/` JS bundle stays under 1.2 MB minified to keep the popup snappy. Fail CI if it exceeds.
10. Accessibility CI: `axe-core` run against the rendered settings panes in Playwright. Zero serious or critical violations.

## 14. Risks

Concrete things that could go wrong, ordered by likelihood times impact.

1. **Mica fallback layering.** Win10 and older Win11 builds without Mica support fall back to Acrylic; further fallback to a solid color is straightforward but the seam where Mica fails silently and the window paints opaque is a common bug. Mitigation: explicit feature detection via `IsWindows11OrGreater`, exhaustive matrix test on Win10 22H2 and Win11 23H2, log the chosen background mode to file.
2. **Hotkey conflict UX.** Win+Shift+U is unusually common; many keyboard remappers and game launchers grab it. If the registration fails silently the user thinks the app is broken. Mitigation: probe registration on app start, surface a yellow banner in Shortcuts pane on failure, fall back to Ctrl+Alt+U, suggest a rebind.
3. **Onboarding OAuth flow flakiness.** Each provider's OAuth flow has its own quirks. A user partway through onboarding with a flaky Cursor cookie import will blame CodexBar4Windows. Mitigation: every sign-in step is independently retryable, skip-able, and resumable; failure messages link to a docs page; the Done step does not require all providers to be signed in.
4. **Live-apply event storm.** Toggling Merge icons OFF synchronously triggers a tray icon teardown and rebuild for every enabled provider; on a machine with 12 enabled providers this is a 200 ms freeze. Mitigation: coalesce rebuilds via a 50 ms idle timer.
5. **Launch at sign-in registry race.** If the user enables Launch at sign-in, then uninstalls without disabling it, the Run entry points at a missing exe. Mitigation: uninstaller removes the entry; on app start the entry is updated to reflect the actual installed path.
6. **i18n key drift.** Adding a new pane or row without adding the corresponding key to all three dictionaries lands as a missing-key fallback to English. Mitigation: a CI step that compares keysets across en.json, zh-Hans.json, pt-BR.json and fails on drift.
7. **Updater channel default flip on prerelease.** A v1.0 release that accidentally contains the string `rc` in the version (`1.0.0-rc.1`) would default new users to beta. Mitigation: explicit `is_prerelease()` test in unit tests covering the parser.
8. **DPAPI roaming profile.** A user on a domain-joined machine with roaming profiles may run the app on a different machine, where DPAPI cannot decrypt the secrets. Mitigation: detect decrypt failure, clear the field, show a friendly "Please paste your cookie again" toast.
9. **Onboarding popup multi-monitor.** Centered on primary monitor is unambiguous on a single monitor, but on a 3-monitor setup the popup may land off-screen if the primary is unplugged. Mitigation: clamp to a visible monitor via `MonitorFromPoint`.
10. **Settings window crash recovery.** A React unhandled exception in a pane could blank the entire window. Mitigation: ErrorBoundary around each pane that renders a "This pane crashed; click to reload" card.

## 15. Time estimate

For one experienced Rust/TS engineer working full-time, with the dependencies from Phases 0 through 7 complete and verified.

| Task block | Time |
|---|---|
| Tasks 1 through 6 (window scaffold, sidebar, persistence bridge) | 2 to 3 days |
| Tasks 7 through 15 (seven panes plus per-provider catalog) | 5 to 7 days |
| Tasks 16 through 18 (launch at sign-in, About pane, updater) | 1 to 2 days |
| Tasks 19 (i18n) | 1 day |
| Tasks 20 through 22 (onboarding flow) | 2 to 3 days |
| Tasks 23 through 27 (live-apply, validation, a11y, window state, telemetry) | 2 to 3 days |
| Tasks 28 through 30 (tests, CI, run-through) | 1 to 2 days |
| Buffer for unknowns | 1 to 2 days |
| **Total** | **15 to 23 working days** |

A two-engineer team can compress to 8 to 12 working days by parallelizing the per-pane work across engineers; the persistence bridge (Task 6) and hotkey subsystem (Task 14) are blocking dependencies that must land first.

## 16. Open questions

These do not block phase start, but should be resolved before phase merge.

1. **Tauri router choice.** `react-router-dom` adds 18 KB to the bundle for two routes (`/popup`, `/settings`, `/onboarding`). Is the bundle budget OK with that, or do we hand-roll a switch on `window.location.pathname`? Recommendation: hand-roll for now; revisit if a third route lands.
2. **Sidebar icons: Segoe Fluent codepoints vs Lucide React.** Segoe Fluent renders perfectly on Win10 and Win11 but is unavailable on browser preview during development. Lucide React adds 12 KB and renders consistently everywhere. Recommendation: ship Segoe Fluent in the title strip (where Windows feel matters most) and use Lucide React for the sidebar icons (consistency across themes).
3. **Onboarding analytics opt-in.** Should the first-run welcome toast include an "anonymous telemetry" opt-in checkbox? Steipete's mac version does not collect any telemetry. Recommendation: leave telemetry off by default; do not surface in onboarding; add a separate Advanced pane toggle in a later phase.
4. **Beta channel signing key.** Do we share signing keys across stable and beta, or use a separate beta-only cert? Phase 9 owns this decision; flagged here so the updater wiring (Task 18) does not paint us into a corner. Recommendation: same cert for both channels at v1.0; revisit if beta gets a wider audience.
5. **Per-action hotkey defaults.** The plan defaults `quickSwitchProvider1` through `9` to Ctrl+1 through Ctrl+9 as local in-popup chords. Should these be global instead, so a user can jump to a provider without first opening the popup? Recommendation: keep local for v1.0 to reduce global hotkey footprint and conflict surface; revisit based on user feedback.
6. **Debug pane scope.** Spec 20 section 8 defines a Debug pane that is only visible when `debugMenuEnabled == true`. Phase 8 scope as agreed ships only the toggle wiring (in Advanced) and the seven user-facing panes. The Debug pane itself is deferred to Phase 9 or a 1.1 follow-up. Confirm this is acceptable.
7. **Onboarding skip.** Should the onboarding popup be skippable on each step ("Skip for now"), or only completable via Done at the end? Recommendation: allow skip from any step (idempotent; re-runnable from About); record skip events for telemetry.
8. **First-run welcome toast localization.** The welcome toast is a Windows toast posted before the user has clicked through to set their language. The toast content is in English. Should we instead localize to the system locale at toast time, or accept English-only for the welcome and localize everything else from step 1 forward? Recommendation: localize the toast to system locale via `GetUserPreferredUILanguages`.
9. **Update channel description copy.** Stable: "Stable, production-ready releases only." Beta: "Stable releases plus beta previews. May contain bugs." Are these acceptable to ship in v1.0, or do we want legal review of the beta disclaimer? Recommendation: ship as is.
10. **Re-run onboarding semantics.** Clicking "Run onboarding again" from About should not wipe existing tokens. But should it pre-check the currently-enabled providers in step 2, or always start from the Claude+Codex defaults? Recommendation: pre-check whatever is currently enabled; the flow becomes a "review and adjust" rather than a fresh start.

## 17. Out of scope

Things that some readers might expect Phase 8 to deliver but that are explicitly deferred.

1. The Debug pane content (spec 20 section 8): probe logs, fetch strategy viewer, error simulation, CLI paths display. Phase 8 wires the `debugMenuEnabled` toggle only.
2. Sparkle-style staged rollouts in the updater. Phase 8 ships a binary stable/beta picker; staged rollouts come with the Phase 9 release pipeline.
3. Auto-detect of provider CLIs and cookies during onboarding. Phase 8's onboarding has the user explicitly pick and sign in; auto-detect (e.g. "Found a Claude session in Chrome cookies; import?") is a 1.1 polish item.
4. Importing settings from steipete/CodexBar on macOS. Cross-platform import is a separate phase; for now a new install is a fresh start.
5. A "What's new" modal on first launch after an update. Deferred to 1.1.
6. Multiple user profiles (e.g. work vs personal) inside one Windows account. The current data model assumes one config per Windows user.
7. Cloud sync of settings. Out of scope for v1.0.
8. A CLI for editing settings (e.g. `codexbar settings set refreshFrequency 1m`). Out of scope for v1.0.
9. A web dashboard for viewing usage. Out of scope; this is a tray app.
10. Voice control or speech input for onboarding. Out of scope.

## 18. Definition of done

Phase 8 is done when every one of these is true:

- All 33 tasks listed in section 4 have landed on `main` as atomic commits with conventional-commit messages, pushed to origin.
- All 20 phase acceptance tests in section 12 pass on a clean Windows 11 22H2 VM and a clean Windows 10 22H2 VM.
- All 10 CI gates in section 13 pass green.
- All 10 open questions in section 16 have been answered in writing, either in a commit message, in this plan via a follow-up edit, or in a GitHub issue.
- The CHANGELOG `## [Unreleased]` entry from Task 30 is in place.
- No GitHub issues tagged `phase-8-blocker` are open.
- The five-minute install-to-first-tray-icon-percent benchmark (test 7 in section 12) has been replicated by at least one engineer other than the author.

When all of the above hold, Phase 8 is closed and Phase 9 (Polish, Packaging, Release v1.0) begins.
