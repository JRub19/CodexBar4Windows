---
title: "Phase 7 plan, Cost Scan, Status Feeds, and Notifications"
phase: 7
status: "Plan, ready for execution"
audience: "Rust and TypeScript engineers on the Windows port"
estimated_calendar_time: "16 to 22 working days, single engineer, fully focused"
predecessors: "Phases 0 through 6, all v1 providers shipping live data"
successors: "Phase 8, Preferences UI polish, onboarding, hotkeys"
source_specs:
  - "docs/windows/spec/70-cost-scanning.md"
  - "docs/windows/spec/55-status-incidents.md"
  - "docs/windows/spec/80-feel-and-polish.md"
---

# Phase 7, Cost Scan, Status Feeds, and Notifications

This is the phase that turns CodexBar4Windows from a usage display into an ambient assistant. The
three subsystems land together because they share infrastructure (the refresh tick, the popup state
store, the settings panel, the tray icon overlay, the toast plumbing). Shipping them piecemeal would
require throwaway scaffolding three times.

After this phase a user with Codex and Claude installed sees: real dollar numbers for the last 30
days that match the macOS reference to the cent, a tiny red dot on the tray when status.openai.com
reports an incident, and a system toast at 80 percent session quota with an OS sound that survives a
muted notification setting.

## Why this phase, why now

By the end of Phase 6 we have seven providers shipping live usage data. The popup card shows
percentages, weekly windows, pace text. What it cannot do yet:

1. Show cost in dollars. Users keep asking, the macOS reference has done this for a year, and the
   data is sitting in JSONL files on disk waiting to be aggregated.
2. Surface incidents. Today if status.openai.com is on fire the user sees the same green bars they
   always see, then yells at the app when their refresh fails. The fix is a 30 line poller and a 5
   by 5 pixel overlay.
3. Notify on threshold crossings. The whole point of a quota meter is the warning at 80 percent, not
   the pretty bar. Without this, the user has to keep glancing at the tray. With this, the tray can
   recede into the background and the toast does the work.

The three pieces are also the last big native surfaces left before Phase 8 polish. Cost scanning
exercises file IO at scale, status polling exercises HTTP fan out, notifications exercise the OS
notification plumbing including AUMID, custom protocol, and sound. After Phase 7 the rest of the app
is React tweaking.

## Dependencies and assumptions

Confirmed in place from prior phases:

- Tauri 2 shell with frameless transparent popup window.
- Shared Rust workspace with a `codexbar_core` crate holding the refresh loop, provider trait,
  settings store, and IPC events to the React side.
- `reqwest` with rustls TLS, `tokio` runtime, `chrono`, `serde_json`, `walkdir`, `tracing`.
- `tauri-plugin-store` for persistent settings, `tauri-plugin-shell` for opening URLs in the default
  browser.
- Settings store with the boolean knobs `tokenCostUsageEnabled`, `providerStorageFootprintsEnabled`,
  `statusChecksEnabled` already declared but unread.
- Per provider descriptor metadata including `statusPageURL`, `statusLinkURL`, `productId` for
  Google Workspace providers.
- AUMID registered at install time (`com.codexbar.codexbar4windows`).

Net new in Phase 7:

- `codexbar_cost` crate, parsers and pricing and aggregator.
- `codexbar_status` crate, two feed parsers and the poller.
- `codexbar_notify` module inside `codexbar_core`, threshold ledger and toast builder.
- React routes for the cost history chart, the per provider status pill, the storage footprint
  card.
- CLI subcommand `codexbar4windows cost`.

External assumptions:

- The macOS reference fixture set has been captured into `tests/fixtures/cost/` and is available as
  input. Phase 6 already shipped sample sessions, this phase adds three more for forks, subagents,
  and pi mixed streams.
- `models.dev` is reachable from CI for the integration test that exercises the 24 hour TTL refresh.
  CI uses a frozen mock served by `httpmock` to keep test runs hermetic.

## Deliverables, three subsystems

### A. Cost usage scanning subsystem

Reads local JSONL transcripts written by the Codex CLI, the Claude Code CLI, and pi agents, applies
the documented pricing tables, and emits a 30 day daily snapshot. Output is consumed by the popup
card cost chart, the menu bar projection text, the CLI, and the optional storage footprint card.

Key contract surfaces:

1. `codexbar_cost::TokenSnapshot` matches the wire form in spec 70 section 7.7. The React side
   subscribes to a Tauri event `cost-snapshot-updated` and renders the chart from this struct.
2. `codexbar_cost::scan(provider, force)` is the only entry point. It is idempotent, coalesces
   concurrent callers, respects a 60 second floor between scans unless `force` is true, runs on
   `tokio::task::spawn_blocking`.
3. Cache files live under `%LOCALAPPDATA%\CodexBar4Windows\cache\cost-usage\`. Filenames match spec
   70 section 8.1 exactly, including the `vN` suffix. Atomic replace via tempfile then rename.
4. CLI `codexbar4windows cost --provider codex|claude|both [--json] [--refresh]` returns the same
   JSON the menu uses. Exit code 0 if all providers parsed at least one file, non zero if a
   selected provider returned an error.

Out of scope for Phase 7, documented in spec 70 but deferred:

- Cross load test against the Swift cache writer. Nice to have, not required for parity, and adds a
  macOS dependency to CI. Capture as a Phase 8 optional task.
- Codex consumer plan projection text refresh. Already shipping live in Phase 5, the only Phase 7
  touch is a small bug fix to clamp the projected monthly cost smoothing constant when the rolling
  window has fewer than 7 entries.

### B. Status feeds and incident overlay subsystem

Polls eight public status endpoints on the existing usage refresh tick, aggregates to a single tray
overlay glyph using a first match wins rule keyed on user provider order, and exposes per provider
pills in the popup.

Key contract surfaces:

1. `codexbar_status::Poller::tick(client, providers)` returns a map of `ProviderId` to
   `StatusSnapshot` and shares the `reqwest::Client` with the usage layer.
2. `StatusSnapshot` is serialized to the React side as the documented six variant lowercase enum.
   The IPC event is `status-snapshot-updated`, debounced 100 ms.
3. Tray icon overlay is composited in Rust using `tiny-skia` on top of the base PNG before it is
   handed to `tauri::tray::TrayIconBuilder::set_icon`. The overlay draws a 5 by 5 pixel circle at
   the top right corner using `GetSysColor(COLOR_WINDOWTEXT)` as the fill.
4. Per provider pill in the popup card is rendered in React, listens to the IPC event, and uses the
   color palette from spec 55 section 8 including light, dark, and high contrast variants.

Link only providers (Alibaba, DeepSeek, Kiro, Mistral, OpenRouter, Perplexity, Vertex AI) get a
plain menu link that opens the status URL in the default browser. They never produce a snapshot and
never contribute to the tray overlay.

### C. Notifications subsystem

Wires `tauri-plugin-notification`, registers the custom URI scheme `codexbar://`, and implements
the threshold ledger described in spec 80 section 11. Adds a sound layer using `rodio` that plays
the OS chime before the toast posts so that muting notifications does not silence the warning.

Key contract surfaces:

1. `codexbar_notify::Posts::session_quota_threshold(provider, window, threshold, remaining)` is the
   single entry point used by the refresh loop after every snapshot update.
2. The threshold ledger lives in memory at runtime and is persisted to a small JSON file under
   `%LOCALAPPDATA%\CodexBar4Windows\state\quota-thresholds.json` so a restart does not re fire the
   80 percent toast on a still depleted account.
3. Sound playback is feature flagged at compile time so non audio CI runners still link. The
   feature is on by default in release builds.
4. The weekly reset celebration receives a separate handler that picks one of two flavors based on
   the setting `weeklyResetCelebrationStyle`, default `mini`. The full flavor uses the same canvas
   confetti as the popup but in a transparent click through overlay window.

## Atomic commit tasks

Each task lands as a single commit. Each commit follows conventional commit format. After each
commit, push to `main`. If CI fails, fix forward. Tasks are grouped by subsystem but the order
within each group is the build order, the reader can interleave A and B and C freely with the noted
hard dependencies.

### Group A, cost usage scanning

#### A1. Scaffold `codexbar_cost` crate

Files:

- `crates/codexbar_cost/Cargo.toml`, new.
- `crates/codexbar_cost/src/lib.rs`, new, exposes `pub mod parser`, `pub mod pricing`, `pub mod
  aggregator`, `pub mod cache`, `pub mod scan`, and the public `TokenSnapshot`, `DailyEntry`,
  `ModelBreakdown` types.
- `Cargo.toml` workspace members, add the new crate.

Acceptance check:

- `cargo build -p codexbar_cost` succeeds with only the stub types.
- `cargo doc -p codexbar_cost --no-deps` lists `TokenSnapshot` and `DailyEntry` in the public API.

Draft commit:

```
feat(cost): scaffold codexbar_cost crate with public snapshot types
```

#### A2. Hardcoded pricing tables

Files:

- `crates/codexbar_cost/src/pricing/codex.rs`, new, with the 20 model table from spec 70 section
  6.3.
- `crates/codexbar_cost/src/pricing/claude.rs`, new, with the 13 model table from spec 70 section
  6.4 including the 200 000 token threshold columns for Sonnet variants.
- `crates/codexbar_cost/src/pricing/mod.rs`, new, defining `PricingEntry`, `TieredAxis`, and the
  `lookup(provider, raw_model_id)` function with the normalization rules from spec 70 section 6.2.
- `crates/codexbar_cost/tests/pricing.rs`, new, table driven test asserting every documented model
  has a row and the Sonnet tier kicks in at exactly 200 000 tokens.

Acceptance check:

- `cargo test -p codexbar_cost pricing` passes with 20 plus 13 plus 4 cases (raw, with date suffix,
  with `openai/` prefix, with `anthropic.` prefix).
- Manual sanity, `pricing::lookup("claude", "claude-sonnet-4-5")` returns 0.000003 input per token
  and 0.000006 input per token above 200 000.

Draft commit:

```
feat(cost): hardcode codex and claude pricing tables with tiered claude
```

#### A3. models.dev fallback fetcher

Files:

- `crates/codexbar_cost/src/pricing/models_dev.rs`, new, fetches `https://models.dev/api.json` with
  a 20 second timeout, divides per million figures by 1 000 000, writes the cache file to
  `%LOCALAPPDATA%\CodexBar4Windows\cache\model-pricing\models-dev-v1.json`, returns a struct that
  shadows the hardcoded table.
- `crates/codexbar_cost/src/pricing/mod.rs`, extend `lookup` to consult the catalog snapshot first
  if the cache file mtime is under 24 hours, then fall back to the hardcoded table.
- `crates/codexbar_cost/tests/models_dev_fixture.rs`, new, parses a frozen JSON in
  `tests/fixtures/models-dev-2026-05.json` and asserts that the Sonnet 200k tier values match spec
  70.

Acceptance check:

- Test passes against the fixture.
- Manual run with network on writes the cache file and a second run within 24 hours skips the HTTP
  fetch (assert via `tracing` log at debug level).

Draft commit:

```
feat(cost): fetch models.dev catalog with 24h ttl and tiered overrides
```

#### A4. Path resolver for Codex, Claude, pi roots

Files:

- `crates/codexbar_cost/src/paths.rs`, new, with `codex_roots(env)`, `claude_roots(env)`,
  `pi_roots(env)` that follow the Windows mappings in spec 70 section 1.
- Handle `CLAUDE_CONFIG_DIR` comma split, append `\projects` if the segment does not already end
  in `projects`.
- Skip missing roots silently, never error on a missing variable.

Acceptance check:

- Unit test under `crates/codexbar_cost/tests/paths.rs` covers: only `USERPROFILE` set, both
  `CODEX_HOME` and `USERPROFILE` set, `CLAUDE_CONFIG_DIR` with two comma values, one ending in
  `projects`, the other not.

Draft commit:

```
feat(cost): resolve codex claude pi root paths on windows with env overrides
```

#### A5. JSONL line filter and parser, Claude

Files:

- `crates/codexbar_cost/src/parser/claude.rs`, new. Implements the byte filter from spec 70 section
  2.1, the 512 KiB line cap, the required field check, the optional dedup fields, the local time
  day key derivation, and the Vertex AI detection from section 2.5.
- `crates/codexbar_cost/tests/parser_claude.rs`, new, with fixtures from
  `tests/fixtures/cost/claude/sample-session.jsonl`. Cases: parent session, subagent path, Vertex
  row by `_vrtx_` id, Vertex row by `@` in model name, line that hits the 512 KiB cap.

Acceptance check:

- All five fixture cases produce the expected `ClaudeUsageRow` shape.
- A row with all four token counts zero is dropped.
- A line longer than 512 KiB is dropped intact, not truncated.

Draft commit:

```
feat(cost): parse claude jsonl assistant rows with byte filter and vertex detection
```

#### A6. JSONL line filter and parser, Codex

Files:

- `crates/codexbar_cost/src/parser/codex.rs`, new. Parses `session_meta`, `turn_context`, and
  `event_msg` with `payload.type == "token_count"` lines. Implements the delta computation rules
  from spec 70 section 4.5 including the `cached_clamp = min(deltaCached, deltaInput)` safeguard.
- The model fallback chain from section 4.6 and the normalization rules from section 6.2.
- `crates/codexbar_cost/tests/parser_codex.rs`, new. Fixtures: an active session, a session with a
  fork (`forked_from_id`), a session that emits only `last_token_usage`, a session that mixes both
  shapes mid stream.

Acceptance check:

- All four fixture cases produce token deltas matching the macOS reference output captured into
  `tests/fixtures/cost/codex/expected-deltas.json`.
- The fork case correctly subtracts the parent totals at or before `forkTimestamp` from the child
  deltas.

Draft commit:

```
feat(cost): parse codex event_msg token_count lines with fork inheritance
```

#### A7. JSONL line filter and parser, pi sessions

Files:

- `crates/codexbar_cost/src/parser/pi.rs`, new. Tracks the rolling `PiModelContext`, resolves
  identity per assistant turn per spec 70 section 5.2, accepts the many key shapes for usage from
  section 5.3, derives the day bucket in local time.
- Computes cost per message using `pricing::lookup` so the 200 000 tier applies per message.
- Records `costSampleCount` and `usageSampleCount` so the aggregator can decide whether to use the
  cached cost sum or recompute.
- `crates/codexbar_cost/tests/parser_pi.rs`, new. Fixtures: a pi session that crosses local
  midnight, a session with three model changes, a session with `tokens` only and no per axis
  fields.

Acceptance check:

- Two day spanning fixture produces two day key entries with the expected split.
- A model change to a non Claude non Codex provider clears the context and following rows are
  dropped.

Draft commit:

```
feat(cost): parse pi agent sessions with rolling model context and per message pricing
```

#### A8. Dedup machinery

Files:

- `crates/codexbar_cost/src/dedup.rs`, new. Implements three layers from spec 70 section 3: in
  file streaming chunk dedup keyed on `messageId:requestId`, cross file canonical key
  `sessionId:messageId:requestId` with the parent over subagent tie break, Codex `session_id` plus
  Windows `FileIdInfo` file identity.
- Uses `windows-sys` for `GetFileInformationByHandleEx` with the `FileIdInfo` info class.
- `crates/codexbar_cost/tests/dedup.rs`, new. Cases: same row in parent and subagent path, two
  Codex files with the same `session_id`, a hardlinked Codex file pair.

Acceptance check:

- The parent over subagent tie break keeps the parent path entry exclusively, by file path.
- Two paths pointing to the same NTFS file by hardlink are detected as one logical file.

Draft commit:

```
feat(cost): dedup sessions across files using fileidinfo and sessionid
```

#### A9. Aggregator and 30 day window

Files:

- `crates/codexbar_cost/src/aggregator.rs`, new. Day keyed grouping per local calendar day,
  packed tuples per provider per spec 70 section 7.3 and 7.4, `since = now - 29 days`, `scan*` keys
  widen by one day for timezone slop.
- Per day Claude tuple `[input, cacheRead, cacheCreate, output, costNanos, sampleCount,
  pricedSampleCount]`.
- Per day Codex tuple `[input, cached, output]`, cost always recomputed at report time.
- `merged([claude, pi])` matches spec 70 section 7.6.
- `crates/codexbar_cost/tests/aggregator.rs`, new. A multi day fixture exercises the rolling window,
  merge rules, and the "no cost reported" edge case.

Acceptance check:

- A 31 day fixture renders only 30 days plus today.
- A model with all rows priced cleanly uses the cached cost sum; a fixture with one unpriced row
  forces a recompute.

Draft commit:

```
feat(cost): aggregate daily buckets with 30 day rolling window and merged provider sums
```

#### A10. Cache, atomic write, incremental scan

Files:

- `crates/codexbar_cost/src/cache.rs`, new. Persists the JSON shapes from spec 70 section 8 with
  the exact filenames `codex-v4.json`, `claude-v2.json`, `vertexai-v2.json`, `pi-sessions-v2.json`.
- Atomic replace: write to `<dir>\.tmp-<uuid>.json`, then `std::fs::rename` with a Windows
  `MoveFileExW` `MOVEFILE_REPLACE_EXISTING` fallback path via `windows-sys` for the case where the
  destination is locked by a reader.
- Incremental scan when `size > cached.size and cached.parsedBytes <= size and cached.parsedBytes >
  0`, parse from `startOffset` to EOF, rekey streaming chunks across scans.
- Cache version invalidation: `version != 1` for Codex and Claude, `version != artifactVersion` for
  pi, force a fresh start.

Acceptance check:

- Round trip test writes and re reads each cache shape.
- Append 100 KiB to a fixture Claude session, run the scan twice, assert the second run reads only
  the appended bytes (measured via a custom `Read` wrapper that counts bytes consumed).
- Concurrent write test: two threads call `write_atomic` 100 times each, the file is always valid
  JSON.

Draft commit:

```
feat(cost): cache jsonl scans with atomic replace and incremental tail parsing
```

#### A11. Scan coordinator, per step ceiling

Files:

- `crates/codexbar_cost/src/scan.rs`, new. Orchestrates: load cache, refresh models.dev catalog (best
  effort), enumerate roots, walk directories, parse changed files, write cache, build snapshot.
- Per step ceiling: 60 to 90 seconds wall clock per provider, enforced via `tokio::time::timeout`
  on the directory walk and on the per file parse loop. The total cost scan stage stays under 3
  minutes across Codex, Claude, Vertex AI, pi sessions.
- Per provider mutex via a `tokio::sync::Mutex<HashSet<ProviderId>>`, coalesces concurrent callers
  to the same future via `tokio::sync::broadcast` or `futures::shared`.
- `refreshMinIntervalSeconds = 60`, skip a scan within 60 seconds of the last completion unless
  `force` is true.
- `crates/codexbar_cost/tests/scan_timeout.rs`, new. A test that injects a 91 second sleep into the
  parser asserts the scan returns within 92 seconds with a partial result.

Acceptance check:

- Cold scan of the fixture set finishes under 5 seconds on a developer SSD.
- Warm scan with no changes finishes under 50 ms.
- A simulated 95 second parser stall returns a partial snapshot with the timeout reason recorded.

Draft commit:

```
feat(cost): orchestrate scan with per step ceiling and per provider coalescing
```

Note on the Mac vs Windows ceiling: spec 50 calls out smell number 2, the Mac 10 minute per step
timeout. Windows uses 60 to 90 seconds. This is a deliberate divergence. Document in code comments
on `scan.rs` so a future reader does not bump it to match Mac out of misplaced parity.

#### A12. Codex consumer projection bug fix

Files:

- `crates/codexbar_core/src/projection/codex.rs`, edit. Clamp the smoothing constant when the
  rolling window has fewer than seven entries: `smoothing = max(1.0 / max(1, n), 1.0 / 7.0)`. The
  pre Phase 7 code divides by zero when a fresh install has zero days of data.
- `crates/codexbar_core/tests/projection.rs`, new test exercising the fresh install case.

Acceptance check:

- Test passes, no panic on zero entries, projected monthly cost is the rolling 7 day sum scaled to
  a month with a minimum of $0.00.

Draft commit:

```
fix(projection): clamp codex monthly cost smoothing on fresh install
```

#### A13. Provider storage footprint scanner

Files:

- `crates/codexbar_cost/src/footprint.rs`, new. Implements the candidate path table from spec 70
  section 11.1, skips symlinks, counts only regular files, populates `unreadablePaths` on access
  denied. Honors the 5 minute throttle and the signature based coalescing from section 11.5.
- React side, new component `src/components/StorageFootprintCard.tsx` listening to the IPC event
  `storage-footprint-updated`. Renders the per component breakdown sorted by size descending. A
  button opens the directory in Explorer via `tauri::api::shell::open` (no delete button).
- `crates/codexbar_cost/tests/footprint.rs`, new, against a synthetic tree on disk: a directory with
  a known regular file size, a symlink that points to a 10 GB sparse file (asserts the symlink is
  not followed and not counted).

Acceptance check:

- The reported total matches the expected byte count from `dir /a /s` for the synthetic tree.
- The symlink case correctly reports the link as zero bytes.
- The card renders a button labeled `Open folder` for each component, never a delete button.

Draft commit:

```
feat(footprint): scan provider storage paths with symlink skip and explorer link
```

#### A14. CLI surface, `cost` subcommand

Files:

- `crates/codexbar_cli/src/cost.rs`, new. Mirrors the Mac surface: `--provider codex|claude|both`,
  `--json`, `--pretty`, `--no-color`, `--refresh`, `--log-level`, exit codes per spec 70 section
  14.4.
- JSON wire format matches spec 70 section 14.3 exactly, including the field renames (`totalCost`
  not `costUSD`, `cost` not `costUSD` inside model breakdowns).
- Text output uses bold cyan for the per provider header when stdout is a TTY and `--no-color` is
  unset.
- `crates/codexbar_cli/tests/cost_cli.rs`, new. Snapshot tests against the fixture cost JSON.

Acceptance check:

- `codexbar4windows cost --provider both --json --pretty --refresh` against the Phase 7 fixture
  matches the macOS reference JSON to the cent, except for the `updatedAt` field.
- Exit code is 0 on success, non zero with `--provider gemini` (unsupported provider) plus stderr
  warning.

Draft commit:

```
feat(cli): add cost subcommand with json text outputs matching mac wire format
```

#### A15. Wire cost snapshot to popup card

Files:

- `crates/codexbar_core/src/tick.rs`, edit. After every usage tick, if `tokenCostUsageEnabled` is
  on, call `codexbar_cost::scan` for each enabled provider with `force = false`. Emit the
  `cost-snapshot-updated` Tauri event with the merged snapshot.
- React `src/components/CostHistoryChart.tsx`, new. Renders the daily bars per spec 70 section 9:
  one bar per day at `y = costUSD`, yellow peak cap at 5 percent of peak height, axis ticks for
  first and last date only, hover tooltip with up to four model rows, footer total.
- React `src/components/CostSummaryRow.tsx`, new, shows `Today` and `Last 30 days` lines in the
  provider card.

Acceptance check:

- Popup card renders the chart with the fixture data.
- Hover tooltip shows the right model breakdown sorted by cost descending.
- Toggling `tokenCostUsageEnabled` off hides the chart and stops the scans on the next tick.

Draft commit:

```
feat(popup): render cost history chart from snapshot events
```

#### A16. Auto enable probe

Files:

- `crates/codexbar_core/src/settings/cost_probe.rs`, new. On first run, if `tokenCostUsageEnabled`
  is unset, enumerate the union of Claude project roots and Codex sessions root and Codex archived
  root for the first `.jsonl`. If any match, set the setting to `true`. Otherwise leave unset, UI
  stays off.
- Run the probe exactly once per install via a sentinel file in
  `%LOCALAPPDATA%\CodexBar4Windows\state\cost-probe.done`.

Acceptance check:

- Fresh install with sample JSONL on disk turns the setting on.
- Fresh install with no JSONL on disk leaves the setting at its default of off.

Draft commit:

```
feat(cost): auto enable cost scan on first run when jsonl logs exist
```

### Group B, status feeds and incident overlay

#### B1. Scaffold `codexbar_status` crate

Files:

- `crates/codexbar_status/Cargo.toml`, new, depends on `reqwest`, `serde`, `serde_json`, `chrono`,
  `tracing`.
- `crates/codexbar_status/src/lib.rs`, new, exposes `StatusSeverity`, `StatusSnapshot`, `StatusSource`,
  the `StatusFeed` trait, and the `Poller` struct.
- The severity enum derives `Serialize` and `Deserialize` with lowercase rename: `none`, `minor`,
  `major`, `critical`, `maintenance`, `unknown`.

Acceptance check:

- `cargo build -p codexbar_status` succeeds.
- A round trip `serde_json::to_string(&Severity::Major)` returns the string `"major"`.

Draft commit:

```
feat(status): scaffold codexbar_status crate with severity enum and traits
```

#### B2. Statuspage parser

Files:

- `crates/codexbar_status/src/feeds/statuspage.rs`, new. `StatuspageFeed { provider, base_url }`,
  appends `/api/v2/status.json` tolerantly to bases with or without trailing slash, parses with the
  field mapping from spec 55 section 2.4.
- Unknown `indicator` strings map to `Severity::Unknown` per spec 55 section 11.
- `crates/codexbar_status/tests/statuspage.rs`, new. Fixtures: OpenAI nominal `none`, OpenAI
  `partial_outage`, Cursor with `page.updated_at` missing, GitHub with an empty `indicator` string.

Acceptance check:

- Each fixture maps to the expected severity and title.
- Empty indicator yields `Severity::Unknown` with `title` preserved when present.

Draft commit:

```
feat(status): parse statuspage v2 status.json with six severity mapping
```

#### B3. Google Workspace incidents parser

Files:

- `crates/codexbar_status/src/feeds/workspace.rs`, new. `WorkspaceFeed { provider, product_id }`,
  fetches `https://www.google.com/appsstatus/dashboard/incidents.json` once per tick, filters by
  product id, prefers `currently_affected_products` over `affected_products`, picks the most severe
  active incident per spec 55 section 2.4 mapping table.
- Strip markdown bullets and `[label](url)` link syntax to plain `label` in the description.
- `crates/codexbar_status/tests/workspace.rs`, new. Fixtures: empty array (steady state, all
  workspace providers map to `none`), a multi incident response with `SERVICE_DISRUPTION` and
  `SERVICE_OUTAGE` (outage wins), a resolved incident (`end != null`, filtered out).

Acceptance check:

- All three fixtures produce the documented severities.
- Description text never contains `*` bullets or markdown link brackets.

Draft commit:

```
feat(status): parse workspace incidents with most severe wins and markdown strip
```

#### B4. Workspace request deduplication

Files:

- `crates/codexbar_status/src/poller.rs`, new. Holds an in flight cache `Option<(Instant,
  Vec<WorkspaceIncident>)>` with a 30 second TTL so that Gemini and Antigravity both see the same
  fetch within a tick.
- `crates/codexbar_status/tests/workspace_dedupe.rs`, new. Asserts that polling both providers in
  the same tick issues a single HTTP request via a counting mock client.

Acceptance check:

- Mock client records exactly one HTTP call per tick when both Workspace providers are enabled.

Draft commit:

```
feat(status): dedupe shared workspace fetch across gemini and antigravity
```

#### B5. Poller, integration with the usage refresh tick

Files:

- `crates/codexbar_status/src/poller.rs`, extend. `Poller::tick(&self, client, providers)` runs
  feeds concurrently in a `tokio::task::JoinSet`, with a 10 second per request timeout, no retry.
  On error with a prior snapshot, keep the prior; on error with no prior snapshot, surface
  `Severity::Unknown` with the error string as title.
- `crates/codexbar_core/src/tick.rs`, edit. Add a `StatusPoller` to the refresh loop, fire on every
  tick when `statusChecksEnabled` is on. Emit IPC event `status-snapshot-updated`.
- `crates/codexbar_status/tests/poller.rs`, new. Cases: feed unreachable then recovers, feed
  returns garbage JSON, toggle flips off mid fetch.

Acceptance check:

- Feed unreachable with prior snapshot keeps the prior snapshot, no IPC event fired.
- Feed unreachable with no prior snapshot fires one event with `Severity::Unknown`.
- Toggle off mid fetch: result discarded, store not mutated.

Draft commit:

```
feat(status): poll feeds on usage tick with sticky prior on transient errors
```

#### B6. Tray icon overlay composition

Files:

- `crates/codexbar_core/src/icon/overlay.rs`, new. After the base icon bitmap is rendered, if the
  aggregated severity has an issue per spec 55 section 6, composite a 5 by 5 pixel dot at the top
  right corner (offset from edges by 1 pixel inset) using `tiny-skia` and `GetSysColor(COLOR_WINDOWTEXT)`
  as the fill via `windows-sys`. The shape varies per spec 80 section 3: 4 by 4 filled circle for
  minor or maintenance, a 2 by 6 pill plus 2 by 2 dot below for major, critical, or unknown.
- Snap to the DPI scaled pixel grid. Pre render at the five tray icon sizes (16, 20, 24, 32, 40 px).
- Aggregation rule: first match wins in the user configured provider order, never severity based.
- `crates/codexbar_core/tests/icon_overlay.rs`, new. Renders the overlay at each DPI scale and
  compares against golden PNGs in `tests/fixtures/icons/`.

Acceptance check:

- Golden PNG comparison passes at 100, 125, 150, 175, 200 percent DPI.
- A minor on the top provider beats a critical on a lower ordered provider; the rendered glyph
  matches the minor shape.

Draft commit:

```
feat(tray): composite status overlay with first match wins aggregation
```

#### B7. Per provider status pill in popup

Files:

- React `src/components/StatusPill.tsx`, new. Renders the chip per spec 55 section 5.2, with the
  severity dot, the truncated label, and the relative freshness from `dayjs.fromNow()`. Uses the
  light, dark, and high contrast color tokens from spec 55 section 8.
- Click opens the vendor status URL in the default browser via `tauri-plugin-shell`. Tooltip shows
  the full label and the vendor `updated_at` on a second line.
- React `src/components/PreferencesProviderSidebar.tsx`, edit, add the 6 by 6 dot next to each
  provider when `statusChecksEnabled` is true.

Acceptance check:

- Pill is hidden when severity is `none`.
- Click opens the vendor URL.
- Narrator reads the severity label correctly; `aria-hidden="true"` on the colored swatch.

Draft commit:

```
feat(popup): render status pill chip and preferences sidebar dot
```

#### B8. Link only menu entries

Files:

- React `src/components/MenuLinks.tsx`, edit. For providers in the link only set (Alibaba,
  DeepSeek, Kiro AWS, Mistral, OpenRouter, Perplexity, Vertex AI GCP), render a `Status Page`
  menu item that opens the URL from `provider.statusLinkURL`. No polling, no snapshot.
- Sanity check that none of the link only providers have a `statusPageURL` field set in metadata,
  which would otherwise pull them into the poller.

Acceptance check:

- The seven link only providers each show a working menu link.
- None of them produce a `StatusSnapshot` at runtime (asserted via a debug log filter).

Draft commit:

```
feat(menu): add status page link items for link only providers
```

#### B9. Settings UI for status checks

Files:

- React `src/routes/SettingsAdvanced.tsx`, edit. Add the toggle `Check provider status` defaulting
  on, persisted to `statusChecksEnabled`.
- Add a tooltip `Status badge follows your provider order.`
- When the toggle flips on, fire a one shot fetch immediately for every feed rather than waiting up
  to a full refresh interval, per spec 55 section 14 open question 5.

Acceptance check:

- Toggle persists across app restart.
- Flipping the toggle on triggers an immediate fetch (asserted via a debug event in the test
  harness).
- Flipping the toggle off clears the tray overlay within one tick.

Draft commit:

```
feat(settings): add check provider status toggle with eager fetch on enable
```

### Group C, notifications

#### C1. Wire `tauri-plugin-notification` with AUMID

Files:

- `src-tauri/tauri.conf.json`, edit. Add `tauri-plugin-notification` to the plugin list, declare
  the AUMID `com.codexbar.codexbar4windows` (must match what the installer registers).
- `src-tauri/installer/codexbar4windows.wxs`, edit. Confirm the AUMID is set on the shortcut so the
  toast attribution surfaces the CodexBar4Windows icon and name.
- `crates/codexbar_core/src/notify/mod.rs`, new module skeleton with `Posts` struct.

Acceptance check:

- A smoke test toast posted from `cargo run --bin tauri-dev -- --post-test-toast` displays with the
  CodexBar4Windows app icon in the Action Center.
- The AUMID in the toast XML matches the AUMID registered by the installer.

Draft commit:

```
feat(notify): wire tauri plugin notification with aumid
```

#### C2. Threshold ledger and dedup

Files:

- `crates/codexbar_core/src/notify/thresholds.rs`, new. Implements the rules from spec 80 section
  11: per provider per window ledger of fired thresholds. Crossing a threshold fires once and marks
  it as fired plus all higher thresholds. Falling below a threshold re arms it only after the
  remaining percent climbs back above it. The ledger is a `HashMap<(ProviderId, WindowKind),
  HashSet<u8>>`.
- Persisted to `%LOCALAPPDATA%\CodexBar4Windows\state\quota-thresholds.json` on every change.
- `crates/codexbar_core/tests/thresholds.rs`, new. Cases: cross 80, drop to 60, drop to 40, recover
  to 50 then 70, recover to 95. Asserts the exact firing sequence.

Acceptance check:

- The firing sequence matches the macOS reference test output captured into
  `tests/fixtures/notify/threshold-crossings.json`.
- Restart preserves the ledger so an 80 percent toast does not re fire on a still depleted account.

Draft commit:

```
feat(notify): track threshold crossings per provider per window with persistence
```

#### C3. Toast builder, click protocol, action buttons

Files:

- `crates/codexbar_core/src/notify/builder.rs`, new. Builds the toast XML per spec 80 section 11:
  `ToastGeneric` template with `appLogoOverride` set to a 96 by 96 PNG of the dynamic icon, the
  `launch` attribute pointing at `codexbar://open?provider=<id>`, action buttons `Open CodexBar`
  and `Snooze 1h`.
- The custom URI scheme `codexbar://` is registered on app install. The Tauri shell forwards the
  launch URI to the running instance via a single instance lock; if no instance is running, the
  protocol handler launches the app and passes the URI on the command line.
- Group keys: `codexbar-quota-<provider>` for quota toasts, `codexbar-celebrate-<isoWeek>` for
  celebrations, so newer toasts replace older.
- `crates/codexbar_core/tests/toast_xml.rs`, new. Snapshot test of the generated XML against
  `tests/fixtures/notify/toast-quota-80.xml`.

Acceptance check:

- Snapshot test passes.
- Clicking the toast opens the popup at the provider's card.
- The Snooze button marks the threshold as snoozed until now plus 1 hour, re arms even if the
  remaining percent has not climbed back above the threshold.

Draft commit:

```
feat(notify): build toast xml with launch protocol and snooze action
```

#### C4. Sound layer with `rodio`

Files:

- `crates/codexbar_core/src/notify/sound.rs`, new. Wraps `rodio::OutputStream` and plays the
  Windows IM chime via `windows::Media::SystemMediaTransportControls` based file path resolution.
  Spec 80 maps `NSSound("Glass")` to `ms-winsoundevent:Notification.IM`. The chime is played from
  Rust on a dedicated sink before the toast posts.
- The toast XML sets `<audio silent="true"/>` so the OS does not also play the toast sound. This
  preserves the spec 80 rule: muting notifications does not silence the quota warning unless the
  master toggle is off.
- Compile time feature `audio` defaults on. When off, the call is a no op.
- The master toggle `quotaWarningSoundEnabled` in settings gates the chime call.
- `crates/codexbar_core/tests/sound.rs`, new. With the `audio` feature off, no panic, no syscall.
  With the feature on but `quotaWarningSoundEnabled = false`, no syscall.

Acceptance check:

- On a developer machine, the chime plays before the toast appears.
- Muting Windows notification sounds in OS settings does not silence the chime.
- Muting the in app `quotaWarningSoundEnabled` does silence the chime; the toast is silent too.

Draft commit:

```
feat(notify): play os chime via rodio before silent toast
```

#### C5. Threshold settings UI

Files:

- React `src/routes/SettingsNotifications.tsx`, new. Renders the threshold list per provider per
  window. Default thresholds: 80, 50, 20, 10 percent. The user can add, remove, reorder.
- Master toggle `Play notification sound`, default on. Toggle `Show incident alerts`, default off.
  Toggle `Toast on celebration`, default on. Toggle `Confetti style` with values `Mini`, `Full`,
  `Off`, default `Mini`.
- Persisted under `notificationThresholds.<provider>.<window>` and the four toggles.

Acceptance check:

- Thresholds persist across restart.
- Removing the 80 percent threshold means the toast does not fire at 80, only at the next lower
  configured threshold.

Draft commit:

```
feat(settings): notification thresholds and sound toggles ui
```

#### C6. Session depleted, session restored, weekly reset

Files:

- `crates/codexbar_core/src/notify/lifecycle.rs`, new. Watches the usage snapshot for two
  transitions: positive to zero remaining (depleted), zero to positive remaining (restored). Fires
  the corresponding toast with the dedup key prefix from spec 80 section 11. Watches for weekly
  window reset events, fires the celebration handler if the user had at least one percent
  utilization in the past 24 hours.
- `crates/codexbar_core/tests/lifecycle.rs`, new. State machine tests for the three transitions.

Acceptance check:

- A simulated session crossing from 5 percent to 0 fires the depleted toast once, not on every
  refresh.
- A simulated weekly reset on a fully utilized account fires the celebration; on an unused account
  it does not.

Draft commit:

```
feat(notify): post session depleted restored and weekly reset toasts
```

#### C7. Weekly reset celebration, Mini and Full flavors

Files:

- `crates/codexbar_core/src/celebration.rs`, new. Mini flavor: schedule a 1500 ms tray icon morph
  using the `unbraid` pattern from spec 80 section 2 (already shipped in Phase 4 as part of the
  loading animations), drive it from the existing 30 Hz tick. Then post the celebration toast.
- Full flavor: spawn a transparent click through overlay window using Tauri's window APIs with the
  flags `WS_EX_TRANSPARENT | WS_EX_LAYERED`, render the canvas confetti from spec 80 section 4 for
  5 seconds, then close. Per spec 80 the staggered 60 ms launches and the palette algorithm match
  the Mac numbers exactly.
- React `src/components/PopupConfetti.tsx`, new. If the popup is open at celebration time, render
  a 1.2 second canvas confetti burst anchored to the provider card header. Hand rolled particle
  system, no external library, less than 2 kB of code.
- A test in `crates/codexbar_core/tests/celebration.rs` covers the Mini flavor scheduling.

Acceptance check:

- Mini flavor: tray icon visibly morphs for 1.5 seconds, toast appears, popup confetti fires if
  open.
- Full flavor: overlay window is click through (mouse clicks pass through to the underlying app),
  no focus stolen.
- Reduced motion: both flavors suppress visual elements; the toast still posts.

Draft commit:

```
feat(notify): weekly reset celebration with mini tray morph and full overlay
```

#### C8. Incident alert toast (optional)

Files:

- `crates/codexbar_core/src/notify/incident.rs`, new. When the global toggle `incidentAlertsEnabled`
  is on (default off), fire a toast on the transition from `none` to `critical` for any user
  favorited provider. Use the dedup key `incident-<provider>` so resolved then reopened incidents
  re fire only after a recovery to `none`.

Acceptance check:

- Toast fires on transition from `none` to `critical` only.
- Toast does not fire on `none` to `minor` or on `minor` to `critical`.
- Default off, no toast unless the user opted in.

Draft commit:

```
feat(notify): optional incident alert toast on critical transition
```

#### C9. Wire all notifications into the refresh tick

Files:

- `crates/codexbar_core/src/tick.rs`, edit. After each refresh, call `notify::lifecycle::tick` and
  `notify::thresholds::tick` with the latest snapshots. Both functions are idempotent: calling
  them with the same snapshot twice fires no extra toasts.
- `crates/codexbar_core/tests/tick_notify.rs`, new. End to end test: a fabricated snapshot stream
  triggers exactly the expected toast set.

Acceptance check:

- End to end test passes.
- A snapshot replayed twice in a row produces no duplicate toasts.

Draft commit:

```
feat(tick): drive notifications from refresh loop with idempotent dispatch
```

## Phase acceptance tests

These are the gates that must pass before the phase merges to `main` as complete. They are run
manually for the first end to end pass and then folded into the CI suite where possible.

### AT1. Cost numbers match Mac to the cent

Procedure:

1. Capture a fixture set on macOS by running `codexbar cost --provider both --json --pretty
   --refresh` on a developer Mac with a known JSONL tree. Save to
   `tests/fixtures/cost/mac-reference-2026-05-12.json`.
2. Copy the same JSONL trees into the documented Windows paths on a Windows test machine.
3. Run `codexbar4windows cost --provider both --json --pretty --refresh`.
4. Diff the two JSON outputs. Required equal: `last30DaysCostUSD` to the cent, `last30DaysTokens`
   exact, per day `costUSD`, `totalTokens`, sorted `modelsUsed`, sorted `modelBreakdowns` by
   `costUSD` descending. Allowed different: `updatedAt`.

Pass when: diff is empty modulo `updatedAt`.

### AT2. Status feeds badge the tray icon correctly

Procedure:

1. Configure the test machine with two providers, Codex first and Claude second in the user order.
2. Force `status.openai.com` to return `indicator: minor` via the mock URL in the test harness.
3. Force `status.claude.com` to return `indicator: critical`.
4. Observe the tray icon overlay.

Pass when: the overlay is the minor glyph (4 by 4 dot), not the critical glyph, because Codex is
first in user order and the first match wins rule applies.

Then reverse the user order so Claude is first, observe the overlay switches to the critical glyph.

### AT3. Toast fires on threshold and survives muted notifications

Procedure:

1. Set the default thresholds: 80, 50, 20, 10 percent. `quotaWarningSoundEnabled = true`.
2. Mute Windows notification sounds via `Settings > System > Notifications`.
3. Simulate a snapshot where the remaining percent drops from 85 to 79.

Pass when: a toast appears in the Action Center, the OS chime plays despite the Windows mute,
clicking the toast opens the popup centered on the primary monitor.

Then drop to 49, 19, 9, and confirm one toast per crossing, no spam, lower thresholds re arm after
recovery to 75.

### AT4. Fixture round trip for parsers

Procedure: run `cargo test -p codexbar_cost --release` and `cargo test -p codexbar_status --release`.

Pass when: all fixture cases from A5, A6, A7, A8, A9, B2, B3, B4 pass.

### AT5. Storage footprint never deletes

Procedure: code review and a runtime check. Search the entire `codexbar_cost::footprint` module
for any of: `std::fs::remove_file`, `std::fs::remove_dir`, `std::fs::remove_dir_all`, `DeleteFile`.

Pass when: zero matches. The card UI is also searched for any element with `onClick` that calls a
delete IPC. Zero matches.

### AT6. Reduced motion suppresses confetti and morph

Procedure: enable Windows reduced motion via `Settings > Accessibility > Visual effects > Animation
effects` off. Trigger a weekly reset celebration via the debug menu.

Pass when: the tray icon does not morph, the toast still posts, the popup confetti does not fire.

## CI gates

The Phase 7 PR merges only if all of these pass in the CI pipeline:

1. `cargo build --workspace --all-features` clean.
2. `cargo test --workspace --all-features` clean, including the fixture suite under
   `tests/fixtures/`.
3. `cargo clippy --workspace --all-features -- -D warnings` clean.
4. `cargo fmt --check` clean.
5. New Rust crates pass `cargo deny check` for license and supply chain.
6. The fixture under `tests/fixtures/cost/` has not been modified accidentally: a checksum check in
   the test harness compares the directory hash against a recorded value.
7. The React side passes `pnpm typecheck`, `pnpm lint`, `pnpm test`.
8. A new dedicated job `windows-toast-smoke` boots a Windows GitHub Actions runner, posts a test
   toast via a CLI subcommand, asserts the toast appears in the Action Center via a PowerShell
   probe. This is the only Windows job needed for the phase; the rest run on the cross platform
   matrix.

Pre commit hook reminder: the repo has no em dashes and no single dashes in prose policy. A simple
`scripts/lint-prose.ps1` script scans staged `.md` files for ` -- ` and ` , `, raises a warning. The
script is informational, not blocking.

## Risks

1. macOS reference fixture skew. If the Mac reference output changes between fixture capture and
   Phase 7 ship, AT1 will fail with a stale expected file. Mitigation: pin the Mac reference at a
   recorded git SHA of the macOS app, document the SHA in the fixture file header. If the Mac
   reference must update mid phase, recapture and bump the fixture version.

2. `models.dev` schema drift. The catalog has changed shape twice in the last year. If a field
   rename lands during Phase 7, the fallback layer silently misprices. Mitigation: the integration
   test pins a known good snapshot at `tests/fixtures/models-dev-2026-05.json`. The live fetch is
   best effort, the hardcoded table is the source of truth. The fetch only adds entries, never
   overrides hardcoded entries on a column mismatch.

3. Windows file identity edges. `FileIdInfo` on networked drives (SMB, OneDrive) returns volume
   serial values that can collide. Mitigation: when `GetFileInformationByHandleEx` returns an error
   or a suspicious zero volume serial, fall back to the canonicalized absolute path as the dedup
   key. Log at warn level so the user sees it once.

4. Statuspage rate limits. With eight Statuspage feeds polling every five minutes, we are at 96
   requests per hour. Atlassian's published limit is way above this, but a custom Statuspage tenant
   might rate limit aggressively. Mitigation: the sticky prior on transient errors covers a 60
   minute outage cleanly. If we see 429 from a specific tenant, back off to 15 minute polling for
   that tenant only.

5. Toast attribution mismatch. If the installer registers a different AUMID than the runtime, toasts
   show a default Windows icon. This is the most common Phase 7 ship blocker. Mitigation: the
   `windows-toast-smoke` CI job asserts the toast attribution. The AUMID string is defined in
   exactly one place, `crates/codexbar_core/src/notify/aumid.rs`, and both the installer and the
   plugin read from that single source.

6. `rodio` pulls in `cpal` with ALSA on Linux. Mitigation: the `audio` feature is off for the
   Linux build, on for the Windows build, gated in `Cargo.toml` via target specific dependencies.

7. Per step timeout pessimism. 60 to 90 seconds may be too tight on slow disks with huge pi
   sessions. Mitigation: instrument the scan with `tracing` spans, surface a `Cost scan slow`
   warning in the Debug panel if median scan exceeds 45 seconds for two consecutive runs.

8. Notification permission denial. Mitigation: on the first refresh after launch, attempt a silent
   permission probe; if denied surface a non blocking banner in the popup with a link to the
   Windows notifications settings page.

9. JSONL line endings on Windows. The Codex CLI sometimes writes `\r\n`. Mitigation: the parser
   strips a trailing `\r` before JSON parsing; fixture `crlf-session.jsonl` covers mixed endings.

10. Storage footprint scanning on huge `node_modules` neighbors can take minutes. Mitigation:
    enforce a 10 second per directory tree budget on the footprint scan, partial results carry a
    `Scan timed out` annotation on the affected component.

## Time estimate

| Group         | Tasks | Engineer days |
|---------------|-------|---------------|
| A, cost scan  | 16    | 8 to 10       |
| B, status     | 9     | 3 to 4        |
| C, notify     | 9     | 5 to 6        |
| Phase tests   |       | 1             |
| Risk buffer   |       | 2 to 3        |
| Total         | 34    | 16 to 22 days |

Critical path: A1, A2, A4, A5, A6, A9, A10, A11. The cost subsystem dominates the schedule because
the dedup and incremental scan rules are the most subtle code in the phase.

## Open questions

1. Should the cost chart default to a 30 day rolling window or a calendar month? Mac uses rolling
   30, this plan follows Mac. A future user may ask for calendar month; defer until requested.
2. Should we ship a `codexbar4windows cost --watch` flag that prints a live updating table? Mac has
   no equivalent, deferred to Phase 8 polish.
3. Should the status badge consider any provider with a reachable status feed, or only providers
   the user has enabled? This plan follows Mac: only enabled providers contribute. Document in the
   tooltip on the toggle.
4. Should the threshold dedup ledger be exposed in the Debug panel for inspection? Recommended yes,
   tracked as a Phase 8 polish item, not blocking.
5. Should the snooze action accept custom durations (15m, 1h, 4h)? Mac has no snooze at all, this
   plan ships 1 hour fixed. If user feedback asks for more, add a chooser in Phase 8.
6. Should we add a `Try a sample notification` button in `SettingsNotifications.tsx` so the user
   can verify their Windows configuration without waiting for a real quota event? Recommended yes,
   trivially small. Tracked under Group C if time permits, else moved to Phase 8.
7. What is the right behavior when both `tokenCostUsageEnabled` is off and the user clicks a cost
   button in the popup? This plan: show a one line empty state with a link that toggles the
   setting. Confirm with design before shipping.
8. Should the storage footprint card link out to the macOS reference's deeper component breakdown,
   or stay at the top level summary? This plan: top level summary plus the first level children
   list, no third level drill down. Document the divergence from Mac in the spec.

## Appendix A, file map (condensed)

Each task above lists its own files. The summary by crate:

```
crates/codexbar_cost/      A1 to A14, A16. Public types, parsers, pricing, aggregator,
                           cache, scan, footprint, plus fixture tests.
crates/codexbar_status/    B1 to B5. Severity enum, two feed parsers, the poller, tests.
crates/codexbar_core/      A12 projection fix, A15 popup wiring, A16 probe, B5 tick wiring,
                           B6 tray overlay, C1 aumid, C2 thresholds, C3 builder, C4 sound,
                           C6 lifecycle, C7 celebration, C8 incident, C9 tick wiring.
crates/codexbar_cli/       A14 cost subcommand and its snapshot test.
src-tauri/                 C1 tauri.conf.json and installer wxs edits.
src/ (React)               A13 footprint card, A15 cost chart and summary, B7 status pill
                           and sidebar dot, B8 link only menu, B9 status toggle,
                           C5 notification thresholds settings, C7 popup confetti.
tests/fixtures/            Cost JSONL fixtures (claude sample, claude crlf, codex fork,
                           codex expected deltas, pi multi day), mac reference JSON for AT1,
                           five overlay golden PNGs, models-dev snapshot, threshold ledger
                           JSON, toast XML snapshot.
```

## Appendix B, command reference

```
cargo test -p codexbar_cost
cargo test -p codexbar_status
cargo test -p codexbar_core notify
cargo run --bin codexbar4windows -- cost --provider both --json --pretty --refresh
pnpm dev | pnpm typecheck | pnpm lint | pnpm test
scripts/refresh-fixtures.ps1
```

The `refresh-fixtures.ps1` script wraps the Mac reference capture flow so AT1 can be rebuilt with
one command on a developer Mac. End of Phase 7 plan.
