---
summary: "Windows-port spec for the refresh loop, central usage store, per-provider fetch dispatch, pace/historical algorithms, status polling, cost-usage and storage coalescing, session-quota and quota-warning notifications, widget data contract, persistence/cache, and lifecycle/error model."
audience: "Rust / TypeScript engineer porting CodexBar's behavior to Tauri 2 + React + shared Rust crate. No Swift required."
polish_target: "Phantom Wallet / Duolingo — every transition smooth, every error chip dignified, no flicker on tab switch, no surprise stalls on the menu."
---

# 50 — Refresh loop, UsageStore, pace tracking, history

This document is a **behavioral blueprint**. It does not show how the macOS Swift code is structured; it shows what the Windows port must *do* to be observably identical (and where it can do better). All references to file paths are macOS source-of-truth pointers only.

The subsystem has four cooperating pieces:

1. **Refresh loop** — a single ticker that fans out per-provider fetches.
2. **UsageStore** — the canonical reactive state shared by the menu bar / popup / widget.
3. **Pace + history pipeline** — derives "are you ahead/behind, will you run out, when?" from snapshots + long-term samples.
4. **Side channels** — status checks, cost-usage scans, provider-storage scans, session-quota notifications, widget snapshot writer.

---

## 1. RefreshFrequency

User-selectable cadence for the *main* refresh loop. One enum, one persisted value.

| Variant         | seconds | Notes                                       |
|-----------------|---------|---------------------------------------------|
| `Manual`        | none    | Timer disabled. Only user-initiated runs.   |
| `OneMinute`     | 60      | Aggressive; battery cost on cookie-heavy providers. |
| `TwoMinutes`    | 120     | —                                           |
| `FiveMinutes`   | 300     | **Default.**                                |
| `FifteenMinutes`| 900     | —                                           |
| `ThirtyMinutes` | 1800    | —                                           |

### Storage

- Mac: `UserDefaults` key `refreshFrequency`, raw value is the variant name.
- Windows: JSON config (`%APPDATA%\CodexBar\config.json` → `refresh_frequency: "five_minutes"`). See §19.

### Invariants

- Changing this setting **cancels the in-flight ticker** and starts a new one (does *not* cancel an in-flight fetch).
- `Manual` leaves the timer task absent; the menu's "Refresh now" item is the only entry point.
- Other cadences (cost-usage TTL, status, storage scans) are NOT tied to this value; they have independent budgets (§11–13).

---

## 2. The loop

### Tick origin

- **Background Tokio task** (Windows). A single async loop: `sleep(RefreshFrequency) → run_refresh()`.
- The Mac code uses `Task.detached(priority: .utility)`; Windows analogue is `tokio::spawn` on the runtime's blocking-friendly default executor. **Not** a high-priority/UI-thread timer.
- A second independent task drives the **token-cost timer** at a fixed 60-minute TTL (§12).

### Per tick — main refresh

In order, on every tick:

1. Guard: bail if `is_refreshing == true` (single-flight).
2. Snapshot the current set of *display-enabled* and *available* providers.
3. Set `is_refreshing = true`; record `refreshing_providers ⊆ enabled`.
4. **Clear state** for newly disabled or newly unavailable providers (snapshot, error, source label, attempts, status, token snapshot, footprint, failure gates, lastTokenFetchAt).
5. Schedule **provider-storage footprint refresh** (coalesced — §13).
6. **Fan out fetches** (parallel `JoinSet`) — one task per enabled provider for usage + one task per available provider for status.
7. Fan out **credits refresh** (Codex only) in the same task group.
8. Await the group. (Each task ALWAYS completes — no panics escape; see §6.)
9. Outside the group: schedule **token-cost refresh** (single-flight queue — §12).
10. Sync OpenAI web state (`open_ai_web_account_did_change` flag, etc).
11. Evaluate the **OpenAI web refresh policy** (access enabled + cookie source enabled + (force OR not in battery-saver)). If allowed, run the dashboard scrape with a 25 s timeout (post-import: 25 s; retry: 8 s).
12. **Persist widget snapshot** (debounced — §15).
13. Set `is_refreshing = false`; mark `has_completed_initial_refresh = true`.

### What runs on user action only

- `replay_loading_animation` (debug).
- `clear_cost_usage_cache` (Preferences → Advanced).
- Forced token-cost refresh (`force = true` cancels any in-flight sequencer).
- `request_open_ai_dashboard_refresh_if_stale` (called when the menu opens or a settings change implies stale dashboard data).
- Manual "Refresh now" — invokes the same `refresh()` entrypoint with `force_token_usage = true`.
- Provider-level "Force refresh Augment session" — bypasses the loop entirely via `provider_runtimes[augment].perform(.forceSessionRefresh)`.

### What runs on settings change

Implemented as an **observation watcher** on the settings store. Any change to:

`refreshFrequency, statusChecksEnabled, sessionQuotaNotificationsEnabled, quotaWarningNotificationsEnabled, quotaWarningThresholds, quotaWarningSoundEnabled, usageBarsShowUsed, costUsageEnabled, randomBlinkEnabled, configRevision, per-provider observed settings, multiAccountMenuLayout, tokenAccountsByProvider, mergeIcons, selectedMenuProvider, debugLoadingPattern, debugKeepCLISessionsAlive, historicalTrackingEnabled, providerStorageFootprintsEnabled`

triggers (on the main/UI actor):

```
probe_logs.clear();
restart_timer();             // cancels old ticker, starts new
update_provider_runtimes();  // starts/stops per-provider runtimes
refresh_historical_dataset_if_needed().await;
refresh().await;             // immediate fan-out
```

The watcher is *re-armed* at the end of each fire (Swift uses `withObservationTracking`; Windows uses a settings broadcast channel on the Rust side, with a debounce of ~16 ms to coalesce burst writes during preferences UI typing).

---

## 3. Concurrency model

### Threading

| Concern                       | Mac (truth)            | Windows port              |
|------------------------------|-------------------------|---------------------------|
| UsageStore state mutation    | `@MainActor`            | A single `Arc<Mutex<UsageState>>` on the Tauri main runtime, OR a dedicated single-threaded actor (e.g. `tokio::sync::mpsc` + loop) for serialized writes. |
| HTTP/cookie/keychain reads   | `Task.detached` utility | `tokio::spawn` on multi-thread runtime. |
| Filesystem scans (cost, storage) | `Task.detached`     | `tokio::task::spawn_blocking` (Rayon for parallel directory walks). |
| Subprocess (CLI probes)      | `SubprocessRunner`      | `tokio::process::Command`. |
| UI delivery                  | Observation → SwiftUI   | Tauri event `usage://state` (debounced 50 ms) → React store. |

### MainActor hops (Windows equivalents)

Every place the Mac code does `await MainActor.run { … }` is a write to the central store. In the port this is a **single mutex acquisition** wrapping the entire mutation block. The mutex must NEVER be held across an `.await` of I/O — that's the single biggest concurrency smell to avoid.

### Sendable boundaries

- `UsageSnapshot`, `RateWindow`, `ProviderFetchResult`, `ProviderFetchOutcome`, `ProviderStatus`, `CostUsageTokenSnapshot`, `WidgetSnapshot`, `PlanUtilizationHistoryBuckets`, `HistoricalUsageRecord`, `CodexHistoricalDataset` are all immutable `Sendable` value types — these become `#[derive(Clone, Serialize, Deserialize)]` structs in Rust.
- `UsageFetcher`, `ClaudeUsageFetching`, `CostUsageFetcher`, `BrowserDetection`, `HistoricalUsageHistoryStore` are stateful services — `Arc<dyn …>` traits, all internal locks fine-grained.
- Task-locals (`ProviderInteractionContext`, `ProviderRefreshContext`) → on the Rust side use `tokio::task_local!` for `Interaction = {Background | UserInitiated}` and `Phase = {Startup | Regular}`. Propagate explicitly when spawning detached tasks.

### Tauri IPC

- Rust → React: one event `usage://state-changed` carrying a thin patch (provider, kind-of-change, version counter); React queries the full state via `invoke("get_usage_state")`. This avoids serializing the entire store on every change.
- React → Rust: `invoke("refresh_now")`, `invoke("set_setting", { key, value })`, `invoke("toggle_provider", { id, enabled })`, `invoke("force_augment_refresh")`, `invoke("clear_cost_cache")`, `invoke("dump_log", { provider })`.
- Subscriptions: a single event channel with a typed envelope. On disconnect (window closed), the loop keeps running — the Windows app must continue refreshing while the popup is hidden, exactly like the Mac status bar.

---

## 4. UsageStore — the canonical state shape

A single observable struct. **Every** field below must exist so the menu and widget can re-render without re-fetching.

```rust
struct UsageState {
    // --- Per-provider primary state ---
    snapshots: HashMap<UsageProvider, UsageSnapshot>,
    errors: HashMap<UsageProvider, String>,
    last_source_labels: HashMap<UsageProvider, String>,
    last_fetch_attempts: HashMap<UsageProvider, Vec<ProviderFetchAttempt>>,
    versions: HashMap<UsageProvider, String>,           // CLI/web version strings
    statuses: HashMap<UsageProvider, ProviderStatus>,
    probe_logs: HashMap<UsageProvider, String>,         // debug
    provider_storage_footprints: HashMap<UsageProvider, ProviderStorageFootprint>,

    // --- Multi-account fan-out ---
    account_snapshots: HashMap<UsageProvider, Vec<TokenAccountUsageSnapshot>>,
    codex_account_snapshots: Vec<CodexAccountUsageSnapshot>,

    // --- Cost / tokens ---
    token_snapshots: HashMap<UsageProvider, CostUsageTokenSnapshot>,
    token_errors: HashMap<UsageProvider, String>,
    token_refresh_in_flight: HashSet<UsageProvider>,
    last_token_fetch_at: HashMap<UsageProvider, Instant>,

    // --- Codex extras ---
    credits: Option<CreditsSnapshot>,
    last_credits_error: Option<String>,
    last_credits_snapshot: Option<CreditsSnapshot>,            // last-good
    last_credits_snapshot_account_key: Option<String>,
    last_credits_source: CodexCreditsSource,                   // None | Api | DashboardWeb
    credits_failure_streak: u32,

    // --- OpenAI web dashboard ---
    open_ai_dashboard: Option<OpenAIDashboardSnapshot>,
    last_open_ai_dashboard_error: Option<String>,
    open_ai_dashboard_requires_login: bool,
    open_ai_dashboard_cookie_import_status: Option<String>,
    open_ai_dashboard_cookie_import_debug_log: Option<String>,
    open_ai_dashboard_attachment_authorized: bool,
    last_open_ai_dashboard_snapshot: Option<OpenAIDashboardSnapshot>,
    last_open_ai_dashboard_attachment_authorized: bool,
    last_open_ai_dashboard_target_email: Option<String>,
    last_open_ai_dashboard_attempt_at: Option<Instant>,
    last_open_ai_dashboard_cookie_import_attempt_at: Option<Instant>,
    last_open_ai_dashboard_cookie_import_email: Option<String>,

    // --- Refresh-loop bookkeeping ---
    is_refreshing: bool,
    refreshing_providers: HashSet<UsageProvider>,
    has_completed_initial_refresh: bool,
    debug_force_animation: bool,
    path_debug_info: PathDebugSnapshot,

    // --- Failure gates (single-flake suppression) ---
    failure_gates: HashMap<UsageProvider, ConsecutiveFailureGate>,
    token_failure_gates: HashMap<UsageProvider, ConsecutiveFailureGate>,

    // --- Quota notification bookkeeping ---
    last_known_session_remaining: HashMap<UsageProvider, f64>,
    last_known_session_window_source: HashMap<UsageProvider, SessionQuotaWindowSource>,
    quota_warning_state: HashMap<QuotaWarningStateKey, QuotaWarningState>,
    weekly_limit_reset_detector_states: HashMap<String, WeeklyLimitResetDetectorState>,

    // --- Reset backfill memory ---
    last_known_reset_snapshots: HashMap<UsageProvider, UsageSnapshot>,

    // --- Pace / history ---
    historical_pace_revision: u64,                             // bump on dataset change
    codex_historical_dataset: Option<CodexHistoricalDataset>,
    codex_historical_dataset_account_key: Option<String>,
    plan_utilization_history: HashMap<UsageProvider, PlanUtilizationHistoryBuckets>,

    // --- Codex routing guards (multi-account safety) ---
    last_codex_account_scoped_refresh_guard: Option<CodexAccountScopedRefreshGuard>,
    last_known_live_system_codex_email: Option<String>,
    open_ai_web_account_did_change: bool,
}
```

### Observation tokens

The menu and the icon read distinct "tokens" — sets of fields whose change should re-render that surface. Use **two version counters** (`menu_rev`, `icon_rev`) bumped whenever a contributing field changes; React stores subscribe to one or the other to minimize unnecessary renders.

| Surface | Fields that bump its counter                                                                                              |
|---------|---------------------------------------------------------------------------------------------------------------------------|
| Menu    | snapshots, errors, source labels, fetch attempts, account snapshots, token snapshots/errors, refresh-in-flight, credits/dashboard, versions, statuses, probe logs, historical pace rev, storage footprints |
| Icon    | snapshots, errors, credits/dashboard, refresh-in-flight, statuses, historical pace rev                                    |

---

## 5. Per-provider fetch dispatch

### Parallelism

All enabled providers fetch **in parallel** in one task group. There is no per-provider serial ordering. Status checks run alongside usage fetches in the same group.

### Per-provider strategy pipeline (`ProviderFetchPlan`)

Each provider has a **descriptor** that owns:

- A set of allowed `ProviderSourceMode`s: `{auto, web, cli, oauth, api}`.
- A `ProviderFetchPipeline` — a function returning an ordered `Vec<dyn ProviderFetchStrategy>`.

A strategy answers four things:

```rust
trait ProviderFetchStrategy: Send + Sync {
    fn id(&self) -> &str;
    fn kind(&self) -> ProviderFetchKind; // Cli | Web | OAuth | ApiToken | LocalProbe | WebDashboard
    async fn is_available(&self, ctx: &ProviderFetchContext) -> bool;
    async fn fetch(&self, ctx: &ProviderFetchContext) -> Result<ProviderFetchResult, Error>;
    fn should_fallback(&self, error: &Error, ctx: &ProviderFetchContext) -> bool;
}
```

### Pipeline run rules

For each strategy in order:

1. Call `is_available(ctx)`. If false → record an attempt with `was_available=false, error=None`; continue.
2. Available → call `fetch(ctx)`.
   - Success → record success attempt; **return** `Success(result)`.
   - Failure →
     - Record attempt with `error_description = err.to_string()`.
     - If `should_fallback(err)` → continue to next strategy, remember error.
     - Else → return `Failure(err)` immediately with attempts.
3. End of list → return `Failure(last_available_error or NoAvailableStrategy)`.

### Candidate retry runner

Within a single strategy, a list of credential/endpoint candidates is iterated by `ProviderCandidateRetryRunner::run` — try each, fall back **only** if `should_retry(err)` returns true and another candidate remains, otherwise rethrow. Used for e.g. multiple Claude OAuth credential sources (CodexBar cache → ~/.claude/.credentials.json → Claude CLI Keychain).

### Deadlines / timeouts

- **OpenAI dashboard web scrape:** 25 s primary, 8 s retry, 25 s post-cookie-import.
- **Per-provider debug log probe:** 15 s (with `run_with_timeout`).
- **Cost-usage scan:** 10 min hard ceiling (token fetch timeout).
- **Status fetch (Statuspage / Google Workspace):** 10 s per request.
- **Generic provider fetch:** governed by the strategy itself (most providers set their own `URLSession.timeoutInterval`).
- **NO global per-provider deadline** is applied around the whole pipeline — a slow strategy will hold a slot until it finishes or errors. **Windows port should** wrap each pipeline run in `tokio::time::timeout` (recommended 45 s) and treat timeout as a fallback-eligible error. (This is a concurrency smell on Mac.)

### Account override

When `multi_account_menu_layout == Stacked` AND the provider has >1 token account, the loop *replaces* the single fetch with a serial loop over up to 6 accounts (limit constant). Each account becomes its own `ProviderFetchContext` via `TokenAccountOverride`. Codex visible accounts are handled similarly (`refresh_codex_visible_accounts_for_menu`).

---

## 6. Fold step — merging into UsageStore

The fold is **per-provider**, idempotent, last-writer-wins per provider, and preserves last-good data on failure. It runs inside the central mutex.

### Success path

```
let scoped = result.usage.scoped_to(provider);
if provider == Codex && !should_apply_codex_usage_result(expected_guard, scoped) { return; }
let backfilled = scoped.backfilling_reset_times(from: last_known_reset_snapshots[provider]);
handle_quota_warning_transitions(provider, backfilled);
handle_session_quota_transition(provider, backfilled);
last_known_reset_snapshots[provider] = backfilled;
snapshots[provider] = backfilled;
last_source_labels[provider] = result.source_label;
errors[provider] = None;
failure_gates[provider].record_success();
if provider == Codex {
    remember_live_system_codex_email_if_needed(backfilled.account_email);
    seed_codex_account_scoped_refresh_guard(backfilled.account_email);
}
record_plan_utilization_history_sample(provider, backfilled).await;
provider_runtimes[provider]?.provider_did_refresh(ctx, provider);
if provider == Codex { record_codex_historical_sample_if_needed(backfilled); }
```

### Failure path

```
if provider == Codex && !should_apply_codex_scoped_failure(expected_guard) { return; }
let had_prior_data = snapshots[provider].is_some();
let should_surface = failure_gates[provider].should_surface_error(had_prior_data);
if should_surface {
    errors[provider] = Some(err.to_string());
    snapshots.remove(provider);
} else {
    errors[provider] = None;     // swallow single flake
}
provider_runtimes[provider]?.provider_did_fail(ctx, provider, err);
```

### Reset-time backfill

`backfilling_reset_times` keeps the previous `RateWindow.resets_at`/`window_minutes` when the latest fetch dropped those fields but the cached reset is still in the future — prevents the menu from flickering "?" reset chips when a provider's response is partial.

### Single-flake suppression

`ConsecutiveFailureGate` increments a streak on failure; if `had_prior_data && streak == 1` → swallow (don't show an error). On success the gate resets. **Two failures in a row** always surface even if prior data exists.

### Cancellation as a special case (account loops)

When cancellation is detected (`CancellationError`, `URLError(.cancelled)`, message matches `cancelled`/`cancellationerror`), the per-account loop **preserves the prior per-account snapshot** instead of writing a "cancelled" placeholder. This is critical for the polish target — switching tabs mid-flight must NOT replace good cards with sad ones.

---

## 7. Highest-usage auto-selection (merged-icons focus)

When multiple providers are enabled and `merge_icons == false` (single icon mode is auto-pick), the menu bar must show the provider closest to its rate limit.

Algorithm:

```
let mut highest: Option<(Provider, f64)> = None;
for provider in enabled_providers() {
    let snapshot = snapshots[provider]?;
    let window = menu_bar_metric_window(provider, snapshot); // per-user metric pref
    let percent = window.map(|w| w.used_percent).unwrap_or(0.0);
    if should_exclude_from_highest_usage(provider, snapshot, percent) { continue; }
    if highest.is_none() || percent > highest.unwrap().1 {
        highest = Some((provider, percent));
    }
}
```

### Exclusion rules

- A provider whose chosen window is `>= 100%` is excluded.
- **Copilot + automatic preference**: only exclude if *both* primary AND secondary are `>= 100%` (free plans expose only one of them).
- **Cursor + automatic preference**: only exclude if all three (primary, secondary, tertiary) are `>= 100%`.

### Metric resolution

`menu_bar_metric_preference(provider, snapshot)` is a per-provider user setting (Auto/Primary/Secondary/Tertiary/ExtraUsage/Average). Codex has its own "consumer projection" that overrides which window counts (see Codex spec doc).

---

## 8. Pace tracking

The pace pipeline answers: "Given you're X% used after Y% of the window, are you ahead, behind, on track? Will you run out? When?"

### Generic (linear) pace

For any provider that exposes a `RateWindow` with `resets_at` AND `window_minutes`:

```
duration = window_minutes * 60
time_until_reset = resets_at - now
elapsed = clamp(duration - time_until_reset, 0..duration)
expected = clamp(elapsed / duration * 100, 0..100)
actual   = clamp(window.used_percent, 0..100)
delta    = actual - expected
```

Stage thresholds on `|delta|`:

| Range       | Stage           |
|-------------|-----------------|
| 0 .. 2      | OnTrack         |
| 2 .. 6      | Slightly{Ahead\|Behind} |
| 6 .. 12     | {Ahead\|Behind} |
| > 12        | Far{Ahead\|Behind} |

### ETA / runs-out

```
if elapsed > 0 && actual > 0 {
    rate = actual / elapsed                       // % per second
    remaining = max(0, 100 - actual)
    candidate = remaining / rate                  // seconds to 100%
    if candidate >= time_until_reset { will_last_to_reset = true }
    else { eta_seconds = candidate }
} else if elapsed > 0 && actual == 0 {
    will_last_to_reset = true
}
```

### Hidden when early

If `expected_used_percent < 3` (i.e. less than 3% of the window has elapsed) → **return None** (no pace card). This prevents nonsensical "you're 4% behind" right after a reset.

### Codex historical pace (override of linear)

If `provider == Codex && settings.historical_tracking_enabled` AND `codex_historical_dataset_account_key == current_account_key` AND a dataset is loaded, run the **historical evaluator**:

1. Filter dataset to weeks with matching `window_minutes` and `reset_at < normalized_current_reset_at`.
2. Require ≥ **3 complete weeks** (`minimum_complete_weeks_for_historical`).
3. Weight each historical week by `exp(-age_in_weeks / 3.0)` (recency τ = 3 weeks).
4. Build a weighted-median **expected curve** on a 169-point grid (hourly over a week).
5. Blend with linear baseline using `λ = clamp((n_eff - 2) / 6, 0..1)` where `n_eff = (Σw)² / Σw²` — more historical data → more weight on history; few weeks → mostly linear.
6. Enforce monotonicity on the expected curve.
7. `expected_now = interpolate(curve, u_now)` where `u_now = elapsed / duration`.
8. For each historical week, shift its curve by `(actual - week_now)` and find first crossing of 100 → ETA candidate. Weighted-median the candidates.
9. `run_out_probability = (Σw_runout + 0.5) / (Σw + 1)`. Reported only when ≥ **5 weeks** (`minimum_weeks_for_risk`).
10. `will_last_to_reset = run_out_probability < 0.5`. If false but no crossings → fall back to true (curve never crosses 100).

If the historical evaluator returns None (insufficient data) → fall back to linear pace.

### Pace output shape

```rust
struct UsagePace {
    stage: Stage,
    delta_percent: f64,
    expected_used_percent: f64,
    actual_used_percent: f64,
    eta_seconds: Option<f64>,
    will_last_to_reset: bool,
    run_out_probability: Option<f64>, // only when historical with ≥5 weeks
}
```

---

## 9. HistoricalUsagePace + PlanUtilizationHistoryStore

Two stores, two purposes, two file shapes.

### `HistoricalUsageHistoryStore` (Codex weekly pace)

- File: `<AppData>\CodexBar\usage-history.jsonl` (one record per line).
- Schema v1; each line is a JSON `HistoricalUsageRecord`:

```rust
struct HistoricalUsageRecord {
    v: u32,
    provider: UsageProvider,                  // currently always Codex
    window_kind: WindowKind,                  // Secondary only
    source: Source,                           // Live | Backfill
    account_key: Option<String>,              // SHA-256 of canonicalized email
    sampled_at: DateTime,
    used_percent: f64,                        // clamped 0..100
    resets_at: DateTime,                      // bucketed to nearest 60 s
    window_minutes: u32,
}
```

### Write rules

A new live sample is **accepted** if any of the following is true vs. the most recent record for `(provider, window_kind, account_key, window_minutes)`:

- It's the first sample.
- `resets_at` differs (week boundary).
- `sampled_at` is ≥ **30 min** newer (`write_interval`).
- `|used_percent - prior.used_percent|` ≥ **1.0** (`write_delta_threshold`).

Otherwise the sample is dropped. Retention: prune anything older than **56 days** before each write.

### Backfill from OpenAI dashboard

When the OpenAI dashboard returns daily credit breakdowns, the store synthesizes up to **8 prior weeks** of `source: Backfill` samples at 15 evenly-spaced fractions of each week (`(0..=14).map(|i| i / 14.0)`), as long as:

- Reference window has a valid `resets_at` and `window_minutes`.
- Calibration window has `used_percent >= 1.0`.
- Calibration credits > 0.001.
- Coverage of the daily breakdown actually spans the reference window (±16 h tolerance).
- The target week is not already "complete" (≥6 samples with start- and end-of-week coverage within 24 h).

### Dataset reconstruction

`build_dataset(account_key)` groups records by `(resets_at, window_minutes)`, requires each week to be "complete" (≥6 samples, with coverage at both ends), then reconstructs a 169-point monotone-non-decreasing curve per week (anchored at u=0:0 and u=1:end_value, interpolated linearly between observed monotone points). Weeks are sorted by `resets_at` ascending. Dataset is None if any constraint fails.

### Account-scoping intricacies

Codex history is keyed on a canonicalized email hash. The store supports legacy unscoped (account_key=None), legacy email-hash, opaque-scoped, and current canonical formats. `CodexHistoryOwnership` decides which scoped + unscoped records belong to the active continuity (single-account streak, no adjacent-multi-account-veto). This is **critical** to port correctly because mis-scoping leaks usage between accounts.

### `PlanUtilizationHistoryStore` (per-provider hourly buckets)

- Directory: `<AppData>\CodexBar\com.steipete.codexbar\history\`.
- File-per-provider: `<provider>.json` (e.g. `codex.json`, `claude.json`).
- Document schema v1:

```jsonc
{
  "version": 1,
  "preferredAccountKey": "abc…",         // optional sticky default
  "unscoped": [ /* histories with no accountKey */ ],
  "accounts": {
    "<accountKey>": [ /* PlanUtilizationSeriesHistory */ ]
  }
}
```

- Each `PlanUtilizationSeriesHistory` is `{ name, windowMinutes, entries: [{capturedAt, usedPercent, resetsAt?}] }`.
- Series names: `session | weekly | opus`.

### Plan-utilization write rules

- Minimum interval per hour bucket: **1 hour** (samples within the same hour are merged to a single canonical entry per "reset segment").
- Cap: **17,520 entries** per series (24 × 730 ≈ 2 years).
- Sample acceptance:
  - For Codex, sample only the lanes returned by `codex_consumer_projection.plan_utilization_lanes`.
  - For Claude, sample `primary→session`, `secondary→weekly`, `tertiary→opus`.
  - For others, sample windows whose `window_minutes == 10080` as `weekly`.
- Merging an hour bucket: split by reset boundary (tolerance 2 min) → keep the peak entry of each segment → if a new reset segment appears, retain the previous segment's peak as a historical anchor (returns up to 2 entries per hour).

### Persistence

A dedicated `PlanUtilizationHistoryPersistenceCoordinator` actor coalesces writes — last-write-wins, single writer at a time, written off the UI thread with atomic file replacement.

---

## 10. Stale / error states

### Per-provider error timeline

| Event                                  | Effect                                                                                  |
|----------------------------------------|-----------------------------------------------------------------------------------------|
| Fetch starts                           | `refreshing_providers.insert(provider)`. UI shows a refreshing card if no snapshot yet. |
| First failure, no prior data           | Surface `errors[provider]`. Snapshot stays empty. Icon dims for this lane.              |
| First failure, had prior data          | **Swallow.** Failure gate streak → 1. Icon stays normal. Last snapshot remains.        |
| Second consecutive failure             | Surface `errors[provider]`. Snapshot cleared. Icon dims.                                |
| Success after failures                 | Failure gate reset. Error cleared. New snapshot wins.                                  |
| Cancellation in per-account loop       | Preserve prior per-account snapshot; no error chip.                                     |

### Per-provider timeout

`runWithTimeout(seconds, op)` returns `"Probe timed out after <N>s"` if the op exceeds the budget. This is currently used **only** for debug-log probes (15 s). The main fetch pipeline does NOT wrap each strategy in a timeout — the Mac code leans on each fetcher's own URLSession timeout. **Windows port should add** a per-pipeline-step soft timeout of ~45 s; treat as fallback-eligible.

### "Stale" definition

`store.is_stale` is true whenever ANY enabled provider has a non-nil `errors[provider]`. Drives the dimmed icon overlay.

### Dim-icon trigger

- `errors[primary_provider].is_some()` → dim that lane's lane-icon (combined mode) or the whole icon (single mode).
- `statuses[provider].indicator != .none` → status overlay dot.
- `is_refreshing && snapshots[provider].is_none() && errors[provider].is_none()` → animated loading dot (uses `should_show_refreshing_menu_card`).

---

## 11. Status polling — separate from usage polling

Status checks ride **inside the same refresh task group** as usage (one task per available provider). They are not on a separate cadence on Mac, but they have these critical properties:

- Only run when `settings.statusChecksEnabled == true`.
- Skip if provider has no `status_page_url` and no `status_workspace_product_id`.
- 10-second `URLRequest.timeoutInterval`.
- **Sources:**
  - Statuspage.io: `<baseURL>/api/v2/status.json` for OpenAI / Claude / Cursor / Factory / Copilot.
  - Google Workspace incidents: `https://www.google.com/appsstatus/dashboard/incidents.json` for Gemini / Antigravity (filtered by product ID).
- **Failure behavior:** if a status fetch fails AND we already have a `statuses[provider]` → **keep the previous status** (avoid flapping). If there was no prior status → set to `{indicator: unknown, description: error}`.

### Indicator → icon overlay

```
none      → no overlay
minor     → small yellow dot
major     → orange dot
critical  → red dot
maintenance → blue/gray wrench
unknown   → faint dot (only if we have no prior)
```

### Windows note

The Windows port should be free to put status checks on their own slower cadence (e.g. every 5 min regardless of `RefreshFrequency`), since Statuspage doesn't change second-to-second. But the initial port should keep parity.

---

## 12. Cost-usage refresh coalescing

A second, slower clock for the local cost-usage scanner (`CostUsageFetcher`).

### Cadence

- Independent timer task at **60-minute TTL** (`token_fetch_ttl`).
- On each main refresh tick, `schedule_token_refresh(force=false)` is called *after* the main task group.
- A sequencer task is **single-flight**: if one is already running, `force=false` is a no-op. `force=true` cancels the running sequencer and starts a new one.
- The sequencer iterates `enabled_providers_for_background_work()` and calls `refresh_token_usage(provider, force)` serially. (Serial, not parallel — disk-heavy work; avoid I/O storms.)

### Per-provider rules

- Only Codex, Claude, Vertex AI fetch token usage. Others get cleared.
- `settings.cost_usage_enabled` must be true.
- `is_enabled(provider)` must be true.
- Single-flight per provider (`token_refresh_in_flight`).
- TTL check: if `!force && now - last_token_fetch_at[provider] < 60 min` → skip.
- Wraps in `withThrowingTaskGroup` with a **10-minute hard ceiling** (`token_fetch_timeout`). Whichever finishes first wins; the other is cancelled.

### Result handling

- Empty `daily` → set `token_errors[provider] = no_data_message`; clear snapshot; record success on the gate (this isn't a "failure").
- Non-empty → store snapshot, clear error, record success, persist widget snapshot with reason `"token-usage"`.
- Cancellation → return silently (caller will reschedule).
- Other error → consult failure gate (same rules as §6); surface or swallow.

### Cache directory

`<UserCache>\CodexBar\cost-usage\` — Mac: `~/Library/Caches/CodexBar/cost-usage/{claude-v2.json, pi-sessions-v1.json, …}`. Windows analogue: `%LOCALAPPDATA%\CodexBar\cost-usage\`.

---

## 13. Provider-storage scan coalescing

Scans disk to compute how much space each provider's local data uses.

### Opt-in

Gated by `settings.provider_storage_footprints_enabled`. When disabled → `provider_storage_footprints.clear()` and tasks cancelled.

### Cadence

- Automatic cadence: **5 minutes** (`automatic_storage_refresh_interval`). Multiple in-tick schedule calls with the same `signature` are coalesced.
- Each main refresh tick calls `schedule_storage_footprint_refresh(for: display_enabled_providers, force=false)`.
- The "Force refresh" path uses `force=true` to bypass both the TTL and the in-flight check.

### Signature

A deterministic string built from `(provider, candidate_paths joined by unit-separators)`. Same signature within 5 min → skip. Different signature (e.g. new managed Codex account added) → run.

### Cancellation safety

Uses a `storage_refresh_generation` counter; results are only applied when the generation hasn't moved (cancellation-safe).

### Scanning

Off-main work (`Task.detached` → `spawn_blocking` in Rust). For each provider, dedupe by candidate-path-set (a single path-set may map to multiple providers — scan once, replicate the result).

---

## 14. Session-quota notifications

### Two distinct notification families

1. **Session depleted / restored** — fires on transitions through 0% remaining.
2. **Quota warnings** — fires on crossing user-configured thresholds (e.g. 80%, 50%, 20%).

### Session transition

Session window resolution:

- Primary takes priority if `primary.window_minutes ≤ 6h` (or unspecified — treat as session).
- For Copilot, fall back to secondary when primary missing (free plans hide one lane).

Algorithm:

```
let current = session_window.remaining_percent;
let prev = last_known_session_remaining[provider];
let current_source = session_window.source; // Primary | CopilotSecondaryFallback

// Source change resets memory without firing.
if prev_source != current_source { update_memory; return; }

if !settings.session_quota_notifications_enabled { return; }

if prev.is_none() {
    if is_depleted(current) { post(Depleted, provider); } // startup-depleted only
    update_memory; return;
}

match transition(prev, current) {
    Depleted  => post notification "X session depleted",
    Restored  => post notification "X session restored",
    None      => no-op,
}
update_memory;
```

`is_depleted(x) == x <= 0.0001`.

### Quota warnings

Tracked per `(provider, window=session|weekly)`. State: `{ last_remaining: Option<f64>, fired_thresholds: HashSet<i32> }`.

Algorithm on each new snapshot:

1. If feature disabled or window absent → wipe state.
2. Resolve `thresholds = settings.resolved_quota_warning_thresholds(provider, window)` (sanitized to active positives, sorted ascending).
3. Clear any fired thresholds where `current_remaining > threshold` (so they can re-arm after a reset).
4. Find the smallest "crossed" threshold: an eligible threshold (≤ current remaining AND not already fired) where either prev was > threshold OR prev is None.
5. If crossed, mark *all thresholds ≥ crossed* as fired (don't multi-fire on a single drop through several thresholds), then post a warning notification with `(provider, window, threshold, current_remaining, sound_enabled)`.

### Deduplication

- Toast id prefix: `session-<provider>-<transition>` (depleted/restored) or `quota-warning-<provider>-<window>-<threshold>` — same id within OS coalescing window means the OS will suppress duplicates.
- Internal: `fired_thresholds` set prevents re-firing the same threshold until the window remaining rises back above it.
- Windows: use `winrt::Windows::UI::Notifications` with the same tag-based dedup, OR Tauri's notification plugin with a custom in-memory de-dup map.

### Snooze

Not exposed as a setting. The user can globally disable via Settings, or set sound off. The OS notification center provides snooze.

### Weekly limit reset celebration (confetti)

When the weekly lane crosses **down through 1%** (was above, now below), post an in-app `WeeklyLimitResetEvent` via NotificationCenter (Mac) / a Tauri event (Windows). Triggers a confetti animation in the popup. Detector state persists in user defaults under `weeklyLimitResetDetectorStates`.

---

## 15. Widget snapshot — data contract

**The Windows port has no widgets.** This section is the *data contract* that the popup app and any future third-party integration (e.g. a Rainmeter skin, a Stream Deck plugin) should read.

### File location

- Mac: shared App Group container, `widget-snapshot.json`.
- Windows: `%PROGRAMDATA%\CodexBar\widget-snapshot.json` (machine-wide read; written by the user-session app). If `%PROGRAMDATA%` is undesirable (writability), use `%LOCALAPPDATA%\CodexBar\widget-snapshot.json` — document the path either way.

### Schema

```jsonc
{
  "entries": [ /* ProviderEntry, one per provider with a snapshot */ ],
  "enabledProviders": [ "codex", "claude", … ],
  "generatedAt": "ISO-8601"
}

// ProviderEntry
{
  "provider": "codex",
  "updatedAt": "ISO-8601",
  "primary":   { /* RateWindow or null */ },
  "secondary": { /* RateWindow or null */ },
  "tertiary":  { /* RateWindow or null */ },
  "usageRows": [ { "id": "primary", "title": "Session", "percentLeft": 73.2 }, … ],
  "creditsRemaining": 12.34,
  "codeReviewRemainingPercent": 50.0,
  "tokenUsage": {
    "sessionCostUSD": 0.42, "sessionTokens": 12345,
    "last30DaysCostUSD": 19.80, "last30DaysTokens": 4_500_000
  },
  "dailyUsage": [ { "dayKey": "2026-05-01", "totalTokens": 12345, "costUSD": 0.42 }, … ]
}
```

### Write rules

- Written via debounced single-flight task. The current write chains via `_ = await previousTask?.result` — i.e. wait for previous to finish, then write the next. The Windows port should use a `tokio::sync::watch` channel + a single writer task that takes the latest value when it wakes up.
- Atomic write (`tempfile + rename`).
- Triggered by: `refresh` completion, `token-usage` updates, account changes, settings changes that affect what's enabled.
- Mac additionally calls `WidgetCenter.reloadAllTimelines()` after each write — **no analogue on Windows**.

---

## 16. Cache & persistence — file map

| Path (Windows)                                                       | Format        | Owner                                       | Retention      |
|----------------------------------------------------------------------|---------------|---------------------------------------------|----------------|
| `%APPDATA%\CodexBar\config.json`                                     | JSON          | Settings, token-accounts, provider config   | forever        |
| `%APPDATA%\CodexBar\usage-history.jsonl`                             | JSON-lines    | HistoricalUsageHistoryStore (Codex pace)    | 56 days rolling|
| `%APPDATA%\CodexBar\com.steipete.codexbar\history\<provider>.json`   | JSON document | PlanUtilizationHistoryStore (per-provider)  | ~2 years cap   |
| `%LOCALAPPDATA%\CodexBar\cost-usage\claude-v2.json`                  | JSON          | CostUsageFetcher (Claude native)            | rebuilt on TTL |
| `%LOCALAPPDATA%\CodexBar\cost-usage\pi-sessions-v1.json`             | JSON          | CostUsageFetcher (pi sessions)              | rebuilt on TTL |
| `%LOCALAPPDATA%\CodexBar\widget-snapshot.json` (or `%PROGRAMDATA%`)  | JSON          | Widget snapshot writer                      | overwritten    |
| `%APPDATA%\CodexBar\probe-logs\codexbar-<provider>-probe.txt`        | plain text    | Debug-only, user clicks "Save log"          | manual         |
| `%LOCALAPPDATA%\CodexBar\Logs\codexbar.<rotation>.log`               | JSON-lines    | FileLogHandler                              | rotate by size |

Settings additionally use a key-value layer mirroring `UserDefaults`. Recommend the `confy` or `directories-next` + `serde_json` combo on Windows for the config file; a lightweight `state.json` (the equivalent of `UserDefaults`) holds:

- `refresh_frequency`, `status_checks_enabled`, `cost_usage_enabled`, `quota_warning_thresholds`, `merge_icons`, `selected_menu_provider`, `multi_account_menu_layout`, `provider_storage_footprints_enabled`, `historical_tracking_enabled`, etc.
- `weekly_limit_reset_detector_states` (JSON map serialized).

---

## 17. Lifecycle events

| Event                  | Behavior                                                                                                                  |
|------------------------|---------------------------------------------------------------------------------------------------------------------------|
| **App launch**         | Load `plan_utilization_history` from disk (per-provider files). Load `weekly_limit_reset_detector_states`. Detect versions in parallel (off-main). Schedule `refresh_historical_dataset_if_needed()`. Kick off first `refresh()`. Start main timer and token timer. |
| **Wake from sleep**    | The Mac code doesn't have an explicit sleep handler — the next ticker iteration naturally catches up. Windows: subscribe to `WM_POWERBROADCAST` (`PBT_APMRESUMESUSPEND`) and call `refresh()` immediately on resume. |
| **Settings change**    | See §2. Restart timer, re-evaluate runtimes, refresh historical, refresh.                                                  |
| **Manual refresh**     | `refresh(force_token_usage: true)` — bypasses cost-usage TTL and OpenAI web throttle.                                       |
| **Provider toggle**    | Triggers settings change. Newly disabled providers have their state cleared on next refresh. Newly enabled providers are fetched immediately. |
| **Account add/remove** | If multi-account stacked is on, the per-account loop runs next refresh; if not, the account selection changes drive a `prepare_refresh_state` correction and a follow-up `refresh`. Codex account changes additionally invalidate the historical dataset cache key. |
| **Codex account switch** | `last_codex_account_scoped_refresh_guard` seeded; in-flight Codex fetches that don't match the new guard are discarded on completion (prevents stale snapshots from clobbering the new account). |
| **Cookies imported**   | Bump `open_ai_web_account_did_change`. Next refresh allows dashboard fetch with `did_import_cookies=true` (25 s timeout).   |

---

## 18. Error model

### Typed errors (Rust)

```rust
enum ProviderFetchError {
    NoAvailableStrategy(UsageProvider),
    StrategyFailed { strategy_id: String, source: Box<dyn std::error::Error + Send + Sync> },
    Timeout { strategy_id: String, seconds: u32 },
    Cancelled,
}

enum CostUsageError {
    TimedOut { seconds: u32 },
    Io(io::Error),
    Decode(String),
    NoData,                                  // not surfaced as error — becomes token_errors no-data text
}
```

### Surfacing rules summary

| Error                                          | UI surface              |
|------------------------------------------------|-------------------------|
| Network/HTTP failure on usage, no prior data   | Error chip in menu      |
| Same, with prior snapshot                      | Swallowed (1st flake)   |
| Same, with prior snapshot, 2nd in a row        | Error chip              |
| Cancellation in per-account loop               | No error, preserve prior |
| Cost-usage empty                               | "No data" chip          |
| Cost-usage error                               | Failure-gate path same as usage |
| Status fetch error                             | Keep prior status; if no prior, show `unknown` with description |
| OpenAI dashboard requires login                | "Sign in" prompt; sticky `open_ai_dashboard_requires_login=true` |

### Logging discipline

Log categories: `providers`, `tokenCost`, `sessionQuota`, `sessionQuotaNotifications`, `quotaWarningNotifications`, `openAIWeb`, `confetti`, etc. (see `LogCategories`).

| Level | When to use                                                            |
|-------|------------------------------------------------------------------------|
| trace | per-request detail, body sizes, header presence (no values)            |
| debug | gate decisions ("OpenAI web refresh gate"), source switches, scheduling |
| info  | successful fetches with summary; quota notifications enqueued          |
| warn  | swallowed flakes that hit second consecutive failure                   |
| error | persisted-failure error; encode/decode failures of cached state        |

Never log credential values, cookies, or full tokens. The `LogRedactor` middleware on Mac strips known secret shapes — replicate this in the Rust `tracing` layer (`Layer` impl).

---

## 19. Mac → Windows mapping

| macOS construct                          | Windows port                                                                                                |
|------------------------------------------|-------------------------------------------------------------------------------------------------------------|
| `@MainActor @Observable UsageStore`      | `Arc<UsageState>` behind a serializing actor (mpsc loop) OR a `parking_lot::RwLock` if mutations stay short. |
| `Combine` publishers                     | Tauri event bus (`AppHandle::emit_all`) + a small `tokio::sync::broadcast` for internal subscribers.        |
| `UserDefaults`                           | `state.json` (settings KV) and `config.json` (user-editable structured config) under `%APPDATA%\CodexBar\`. |
| `URLSession.shared`                      | `reqwest::Client` with a long-lived pool; per-request `.timeout(...)`.                                       |
| `Task.detached(priority: .utility)`      | `tokio::spawn` on default runtime; `spawn_blocking` for fs/proc.                                            |
| `withTaskGroup`                          | `tokio::task::JoinSet`.                                                                                     |
| `withThrowingTaskGroup` race-vs-timeout  | `tokio::time::timeout(d, future)`.                                                                          |
| `TaskLocal` (`ProviderInteractionContext`) | `tokio::task_local!`.                                                                                     |
| `WidgetCenter.reloadAllTimelines()`      | none.                                                                                                       |
| `NSWorkspace.shared.open(url)`           | `tauri::api::shell::open` or `start <path>`.                                                                |
| `NSSound("Glass" / "Ping")`              | `winrt::Windows::Media::Audio` or `rodio` with bundled .wav.                                                |
| `UNUserNotificationCenter`               | Tauri `notification` plugin / `winrt::Windows::UI::Notifications`.                                          |
| `BrowserDetection` + cookie file readers | Re-implement against Windows browser cookie stores (Chromium: `%LOCALAPPDATA%\Google\Chrome\User Data\Default\Network\Cookies`, etc.). |
| `Bundle.main.bundleIdentifier`           | hardcoded `app_id = "com.codexbar.windows"` (or whatever the installer uses).                               |
| `ApplicationSupportDirectory`            | `%APPDATA%\CodexBar`.                                                                                       |
| `CachesDirectory`                        | `%LOCALAPPDATA%\CodexBar\cache`.                                                                            |
| `AppGroup`                               | not applicable (no widgets); use `%PROGRAMDATA%\CodexBar\` if a shared inter-process drop is needed.        |
| SwiftUI views with `@Bindable usageStore`| React + Zustand store, hydrated from `invoke("get_usage_state")` and patched on `usage://state-changed`.   |
| Mac status item                          | Tauri tray icon + popover window. Icon redraw on `icon_rev` bump.                                          |

### State broadcast on the Rust side

```rust
enum StatePatch {
    Provider(UsageProvider, ProviderPatch),
    Credits, Dashboard, Storage(UsageProvider),
    IsRefreshing(bool), Status(UsageProvider),
    PaceRev,
}
```

Tauri emits a typed event per patch (debounced 50 ms). React's Zustand store has selectors keyed on patch fields → React Compiler / `useSyncExternalStore` ensures minimal re-renders.

---

## State diagram — main refresh loop

```text
                         (app launch)
                              │
                              ▼
                  ┌──────────────────────┐
            ┌────►│        Idle          │◄────────────────┐
            │     │ (timer task awake,   │                 │
            │     │  no fetch running)   │                 │
            │     └─────────┬────────────┘                 │
            │               │ tick OR user "Refresh now"   │
            │               │ OR settings change           │
            │               ▼                              │
            │     ┌──────────────────────┐                 │
            │     │   Acquire Refresh    │                 │
            │     │  (set is_refreshing) │                 │
            │     │  drop if already on  │                 │
            │     └─────────┬────────────┘                 │
            │               │                              │
            │               ▼                              │
            │     ┌──────────────────────┐                 │
            │     │  Pre-fold cleanup    │  removes        │
            │     │  (clear disabled /   │  stale slices   │
            │     │   unavailable)       │                 │
            │     └─────────┬────────────┘                 │
            │               ▼                              │
            │     ┌──────────────────────┐                 │
            │     │  Schedule storage    │ coalesced       │
            │     │  footprint refresh   │ (5 min)         │
            │     └─────────┬────────────┘                 │
            │               ▼                              │
            │     ┌──────────────────────────────────────┐ │
            │     │       JoinSet: parallel fan-out      │ │
            │     │  ┌────────────┐  ┌──────────────┐    │ │
            │     │  │ Usage(P_i) │  │ Status(P_i)  │    │ │
            │     │  └─────┬──────┘  └──────┬───────┘    │ │
            │     │        ▼                ▼            │ │
            │     │  ┌────────────────────────────┐      │ │
            │     │  │  ProviderFetchPipeline:    │      │ │
            │     │  │  for strategy in ordered:  │      │ │
            │     │  │    avail? fetch? fallback? │      │ │
            │     │  └────┬───────────────┬───────┘      │ │
            │     │       │ success       │ failure      │ │
            │     │       ▼               ▼              │ │
            │     │  ┌────────────┐  ┌──────────────┐    │ │
            │     │  │ Fold OK    │  │ Fold ERR     │    │ │
            │     │  │ - snapshot │  │ - failureGate│    │ │
            │     │  │ - reset bf │  │ - swallow or │    │ │
            │     │  │ - quotaNote│  │   surface    │    │ │
            │     │  │ - history  │  └──────────────┘    │ │
            │     │  └────────────┘                      │ │
            │     │  ┌──────────────┐                    │ │
            │     │  │ Credits(Codex)│  (single task)    │ │
            │     │  └──────────────┘                    │ │
            │     └─────────┬────────────────────────────┘ │
            │               ▼                              │
            │     ┌──────────────────────┐                 │
            │     │  Schedule TokenCost  │  single-flight  │
            │     │  sequencer (60 min)  │  (force cancels)│
            │     └─────────┬────────────┘                 │
            │               ▼                              │
            │     ┌──────────────────────┐                 │
            │     │  OpenAI web policy?  │ yes → scrape    │
            │     │  (access + cookies   │ 25 s primary    │
            │     │   + battery saver)   │                 │
            │     └─────────┬────────────┘                 │
            │               ▼                              │
            │     ┌──────────────────────┐                 │
            │     │  Persist widget      │  debounced      │
            │     │  snapshot            │                 │
            │     └─────────┬────────────┘                 │
            │               ▼                              │
            │     ┌──────────────────────┐                 │
            └─────┤   Release Refresh    ├─────────────────┘
                  │ (is_refreshing=false,│
                  │  emit usage://state) │
                  └──────────────────────┘
```

Side-channel loops (run independently of the diagram above):

```text
TokenCost timer ──tick(60 min)──► schedule_token_refresh(force=false)
                                      │ (no-op if sequencer running)
                                      ▼
                              serial per provider:
                                Codex → Claude → Vertex
                                  10 min ceiling each

Storage scan  ──tick(5 min)──► schedule(...) ──signature?─┐
                                                          ▼
                                            spawn_blocking scan
                                            apply iff generation match
```

---

## 20. Acceptance checklist

A Windows reviewer should verify each of the following with manual tests + automated harness:

### Refresh loop
- [ ] Default cadence is 5 min; changing to 1 min causes the next fetch to happen within `(prev_interval, 60s)`.
- [ ] Setting cadence to "Manual" stops the timer; "Refresh now" still works.
- [ ] Two rapid setting changes coalesce — only one refresh runs.
- [ ] `refresh()` is single-flight; clicking "Refresh now" while one is running is a no-op (no double-fire).
- [ ] Wake-from-sleep triggers a refresh within 5 s.

### Fold + errors
- [ ] First fetch failure with prior snapshot does NOT surface; UI stays calm.
- [ ] Second failure DOES surface; icon dims.
- [ ] Recovery clears the error and resets the streak.
- [ ] Cancellation in the account loop does NOT replace good cards with cancellation chips.
- [ ] Reset-time backfill keeps "resets in 3d 4h" visible even when a partial fetch drops resets_at.

### Highest usage
- [ ] When two providers are enabled, the icon shows the highest non-saturated one.
- [ ] Copilot at 100% session but ≠100% weekly is NOT excluded (in automatic mode).
- [ ] Cursor at 100% on all three lanes IS excluded.

### Pace
- [ ] No pace card shown before `expected ≥ 3%` of the window.
- [ ] Stage label flips Slightly→Ahead at delta=6.
- [ ] "Runs out in …" only appears when ETA falls inside the reset window.
- [ ] Codex with ≥3 weeks of history uses historical pace; <3 weeks falls back to linear.
- [ ] Codex with ≥5 weeks reports a `runOutProbability`; with 3–4 weeks does not.
- [ ] After dashboard scrape, ≤8 prior weeks of backfill records appear in `usage-history.jsonl`.

### History persistence
- [ ] Killing the app mid-refresh and relaunching restores: plan utilization history (per provider), Codex weekly history, weekly-reset detector state, last widget snapshot.
- [ ] Account switch on Codex correctly scopes the visible dataset; the previous account's history is preserved on disk but not shown.
- [ ] Retention prunes `usage-history.jsonl` entries older than 56 days on next write.

### Cost-usage
- [ ] First "Refresh now" forces a cost-usage scan even within TTL.
- [ ] Regular refresh ticks within the TTL do NOT re-scan.
- [ ] Cancelling and forcing again cancels the previous sequencer.
- [ ] Empty results render as a "No data" chip, not as an error.

### Storage scan
- [ ] Toggling "Show provider storage usage" off clears all footprints immediately.
- [ ] Toggling on triggers a scan within one refresh cycle.
- [ ] Same signature within 5 min is a no-op.

### Status checks
- [ ] Disabling status checks clears `statuses` and stops fetching.
- [ ] A failing Statuspage request does NOT remove the previously-known status.
- [ ] Workspace incident parser picks the most severe active incident.

### Notifications
- [ ] Crossing from 1% to 0% remaining fires "depleted" once.
- [ ] Crossing back fires "restored" once.
- [ ] User-configured 80% threshold fires once on crossing; doesn't re-fire until remaining rises back above 80%.
- [ ] Disabling sound suppresses the chime but not the toast.

### Widget snapshot
- [ ] After every refresh, `widget-snapshot.json` is rewritten atomically.
- [ ] `entries[].usageRows` are filtered to those with a non-null percent.
- [ ] `enabledProviders` reflects the current enabled+ordered list.

### Concurrency hygiene
- [ ] No `.await` while holding the state mutex (audit via tracing spans).
- [ ] All `tokio::spawn`-ed tasks are joined or stored as `JoinHandle` for cancellation.
- [ ] Cancellation propagates: closing the app cleanly cancels the timer, the token timer, the storage task, and the OpenAI dashboard refresh task.
- [ ] No panic crashes the runtime — fetch errors must be `Result`-typed all the way.

### IPC
- [ ] React receives `usage://state-changed` patches within 100 ms of state mutation.
- [ ] Popup window opening from cold tray click renders within 150 ms (state is read from the in-memory store, NOT recomputed).
- [ ] Closing the popup does NOT pause the refresh loop.

---

## Appendix A — provider runtime hooks

Some providers attach a long-lived `ProviderRuntime` (e.g. Augment session keepalive). The hooks are:

| Hook                          | When                                                                                  |
|-------------------------------|---------------------------------------------------------------------------------------|
| `start(ctx)`                  | When the provider becomes enabled (or at launch).                                     |
| `stop(ctx)`                   | When the provider becomes disabled.                                                   |
| `settings_did_change(ctx)`    | After observed settings change.                                                       |
| `provider_did_refresh(ctx, p)`| After a successful fold; provider's snapshot is up-to-date.                           |
| `provider_did_fail(ctx, p, e)`| After a fold-surfaced failure.                                                        |
| `perform(action, ctx)`        | User-initiated runtime action (e.g. `force_session_refresh` for Augment).             |

The runtimes are owned by `UsageStore.provider_runtimes` and are NOT serialized to disk.

---

## Appendix B — concurrency smells called out for the port

1. **No per-strategy timeout**: a Claude OAuth + slow keychain probe can stall the whole `JoinSet` until the inner URLSession timeout (often 60 s). Add a `tokio::time::timeout(45 s, strategy.fetch(ctx))`.
2. **Cost-usage scan is sequential**: Codex → Claude → Vertex. For Windows the disk read cost is similar; sequential is fine, but the per-step 10-min ceiling allows a single slow scan to delay the others significantly. Consider lowering ceiling to 60–90 s and surfacing as `TimedOut`.
3. **`prepareRefreshState` mutates settings during refresh**: `persistResolvedCodexActiveSourceCorrectionIfNeeded()` writes back to UserDefaults inside the loop. Port should keep this off the hot path or move it to the settings save coordinator.
4. **`probe_logs.clear()` on every settings change**: drops cached debug logs even for unrelated settings. Acceptable for parity, but consider scoping the clear to relevant providers.
5. **Widget-snapshot write chaining via `_ = await previousTask?.result`**: creates an unbounded chain of tasks if writes are faster than disk. Port should use a `watch` channel + single consumer.
6. **Two failure-gate maps**: `failure_gates` (usage) and `token_failure_gates` (cost). Same struct, same rules, separate maps. Acceptable, but worth documenting.
7. **`backfilling_reset_times` runs on MainActor**: a tiny computation, but if Mac extends it later the port should keep it pure-functional.
8. **OpenAI dashboard refresh has its own multiplier (`5 ×`)** on top of `RefreshFrequency`. This can stretch to 30 min × 5 = 2.5 h between scrapes on the slowest cadence — fine for a heavy webview operation.
9. **`@unchecked Sendable` on `ProviderFetchOutcome`** to allow `Result<…, Error>` across actors. In Rust, `Box<dyn Error + Send + Sync>` handles this naturally — no unchecked needed.
10. **Codex account-scoped refresh guard**: a string token compared on each fetch return. Mismatch → silently discard. Port should preserve this; race-y account switches will otherwise leak data between accounts.

---

## Appendix C — public surface to expose over Tauri

Minimum commands the React side needs to call:

| Command                       | Args                                     | Returns                       |
|-------------------------------|------------------------------------------|-------------------------------|
| `get_usage_state`             | —                                        | full `UsageState` snapshot    |
| `get_usage_state_diff`        | `since_rev: u64`                         | patch list since version      |
| `refresh_now`                 | `{ force_token_usage: bool }`            | ()                            |
| `set_setting`                 | `{ key, value }`                         | new settings snapshot         |
| `toggle_provider`             | `{ provider, enabled }`                  | ()                            |
| `reorder_providers`           | `{ ordered: Vec<UsageProvider> }`        | ()                            |
| `clear_cost_cache`            | —                                        | `Option<String>` (error msg)  |
| `dump_log`                    | `{ provider }`                           | `String` path                 |
| `set_codex_visible_account`   | `{ id }`                                 | ()                            |
| `force_augment_refresh`       | —                                        | ()                            |
| `open_url`                    | `{ url }`                                | ()                            |

Events emitted by the Rust side:

| Event                          | Payload                                       |
|--------------------------------|-----------------------------------------------|
| `usage://state-changed`        | `StatePatch` (above)                          |
| `usage://refresh-began`        | `{ providers: Vec<UsageProvider> }`           |
| `usage://refresh-ended`        | `{ duration_ms: u32 }`                        |
| `usage://session-quota`        | `{ provider, transition }`                    |
| `usage://quota-warning`        | `{ provider, window, threshold, remaining }`  |
| `usage://weekly-reset`         | `{ provider, account_label }`  → confetti     |
| `usage://error`                | `{ provider, message, surfaced: bool }`       |

---

## Appendix D — quick reference of magic numbers

| Constant                                              | Value         | Source                                              |
|-------------------------------------------------------|---------------|-----------------------------------------------------|
| Default refresh cadence                               | 5 min         | `RefreshFrequency.fiveMinutes`                      |
| Cost-usage TTL                                        | 60 min        | `UsageStore.tokenFetchTTL`                          |
| Cost-usage hard ceiling                               | 10 min        | `UsageStore.tokenFetchTimeout`                      |
| Status fetch timeout                                  | 10 s          | `fetchStatus`/`fetchWorkspaceStatus`                |
| Per-provider debug-log probe timeout                  | 15 s          | `runWithTimeout`                                    |
| OpenAI dashboard fetch — primary / retry / post-cookie| 25 s / 8 s / 25 s | `UsageStore+OpenAIWeb`                          |
| OpenAI web refresh interval                           | 5× refresh cadence, min 120 s | `openAIWebRefreshIntervalSeconds`     |
| Storage scan cadence                                  | 5 min         | `automaticStorageRefreshInterval`                   |
| Pace minimum elapsed before showing                   | 3% of window  | `minimumPaceExpectedPercent`                        |
| Stage threshold breakpoints                           | 2 / 6 / 12 %  | `UsagePace.stage(for:)`                             |
| Historical evaluator min weeks                        | 3             | `minimumCompleteWeeksForHistorical`                 |
| Historical evaluator min weeks for risk %             | 5             | `minimumWeeksForRisk`                               |
| Recency τ (weeks)                                     | 3             | `recencyTauWeeks`                                   |
| History retention                                     | 56 days       | `retentionDays`                                     |
| History write interval                                | 30 min        | `writeInterval`                                     |
| History write delta                                   | 1 %           | `writeDeltaThreshold`                               |
| History grid points                                   | 169           | `CodexHistoricalDataset.gridPointCount`             |
| Backfill week cap                                     | 8             | `backfillWindowCapWeeks`                            |
| Backfill calibration min %                            | 1 %           | `backfillCalibrationMinimumUsedPercent`             |
| Plan-utilization minimum hour bucket                  | 60 min        | `planUtilizationMinSampleIntervalSeconds`           |
| Plan-utilization reset-equivalence tolerance          | 2 min         | `planUtilizationResetEquivalenceToleranceSeconds`   |
| Plan-utilization max samples per series               | 17 520        | `planUtilizationMaxSamples`                         |
| Token-account menu snapshot limit                     | 6             | `tokenAccountMenuSnapshotLimit`                     |
| Session depletion threshold                           | 0.0001 %      | `SessionQuotaNotificationLogic.depletedThreshold`   |
| Weekly limit reset threshold                          | 1 %           | `weeklyLimitResetThreshold`                         |

---

## Appendix E — what "Phantom / Duolingo polish" means here

The data layer alone won't get you there. These behaviors are what separate this port from a bog-standard "refresh button" app:

1. **Never flicker on tab switch.** Cancellation must preserve prior data (see §6). If a card was visible 50 ms ago, it stays visible.
2. **Single-flake immunity.** One bad fetch never breaks the menu (see `ConsecutiveFailureGate`). The user sees calm; only persistent failure earns a chip.
3. **Reset countdown always present.** `backfilling_reset_times` keeps the "resets in …" text steady even when a partial fetch drops the field.
4. **Loading states are micro.** `should_show_refreshing_menu_card` only shows a loading card when there is *neither* snapshot *nor* error. Most ticks the user sees no spinner.
5. **Auto-pick the urgent provider.** Highest-usage promotion (§7) means the user's eye is drawn to the lane closest to its limit without them choosing.
6. **Pace is honest.** Hide pace below 3% elapsed; switch to historical when there's enough data; report run-out probability only when statistically meaningful.
7. **Weekly reset = confetti.** The detector watches for the 1%→0% transition and emits a celebration event. The polished port wires this to a particle animation in the React popup.
8. **No surprise stalls.** Cost scans and storage scans are explicit, coalesced, off the hot path. Menu opens instantly.
9. **Notifications dedupe.** Same threshold doesn't fire twice in one cycle; restoration is distinct from depletion; sound is optional.
10. **Persistence survives crashes.** All deriveds (`historical_pace_revision`, `weekly_limit_reset_detector_states`, plan utilization) reload from disk on launch. The user never starts from scratch.

If a behavior in this document conflicts with what feels right, the polished move is almost always: *keep the last-good data, swallow the first flake, defer the heavy work, and emit one well-typed event so React can decide how to animate it.*
