# 70 — Cost-Usage Scanning Subsystem (Windows Port Blueprint)

**Source of truth:** macOS `Sources/CodexBarCore/CostUsageFetcher.swift`,
`Vendored/CostUsage/*`, `PiSessionCostScanner.swift`, `ProviderStorageFootprint.swift`,
`CLI` cost surface.
**Goal:** Re-implement, in a shared Rust crate consumed by a Tauri 2 + React shell,
a local JSONL log scanner that yields **the same daily cost numbers to the cent**
as macOS CodexBar for Codex + Claude (+ Vertex AI Claude fallback) over a rolling
30-day window.

This document is a self-contained spec for a Rust/TS engineer who does not read Swift.
It documents *file formats, parsing rules, dedup, aggregation, caching, pricing tables* —
not Swift code.

---

## 1. Inputs (root directories scanned)

The scanner walks three families of JSONL trees, each independent and each with
multiple roots. **Every root is opt-in by existence**: a missing root is silently
ignored. No root is ever written to by this subsystem.

### 1.1 Codex session logs

| Variable | Default (Mac) | Windows mapping |
|---|---|---|
| `CODEX_HOME` env | unset | unset |
| Live root | `$CODEX_HOME/sessions` else `~/.codex/sessions` | `%CODEX_HOME%\sessions` else `%USERPROFILE%\.codex\sessions` |
| Archive root | sibling `~/.codex/archived_sessions` | `%USERPROFILE%\.codex\archived_sessions` |

**Layout inside the root** (any one of these — the scanner tries all three for
robustness across Codex CLI versions):

- **Date-partitioned**: `<root>/YYYY/MM/DD/*.jsonl`
- **Flat**: `<root>/*.jsonl` (filename may contain a `YYYY-MM-DD` substring
  used as an opportunistic day-prefilter)
- **Recursive (forced rescan only)**: any depth `<root>/**/*.jsonl`

**Listing strategy** for an incremental refresh: union of partitioned ∪ flat scan,
plus a "cold-cache lookback" that enumerates the whole tree once when the cache is
empty or roots changed (`listCodexRecentlyModifiedFiles` filtered by file mtime ≥
`since`-day start).

### 1.2 Claude Code session logs

| Variable | Mac default | Windows mapping |
|---|---|---|
| `CLAUDE_CONFIG_DIR` env | unset | unset (read identically) |
| Root 1 | `$CLAUDE_CONFIG_DIR` split on `,`, each `<x>/projects` (or `<x>` if it already ends in `projects`) | identical, only with `%CLAUDE_CONFIG_DIR%` and backslash separators |
| Fallback 1 | `~/.config/claude/projects` | `%USERPROFILE%\.config\claude\projects` |
| Fallback 2 | `~/.claude/projects` | `%USERPROFILE%\.claude\projects` |

**Layout**: arbitrary depth. Globbed as `<root>/**/*.jsonl`. Path semantic:
a path component literally named `subagents` marks a row as `subagent`,
otherwise `parent` (this is a dedup tie-breaker — see §3.4).

### 1.3 pi (Practical Intelligence) session logs — sibling format

| Mac | Windows |
|---|---|
| `~/.pi/agent/sessions/**/*.jsonl` | `%USERPROFILE%\.pi\agent\sessions\**\*.jsonl` |

Filename convention parsed for opportunistic prefilter:
`^(\d{4}-\d{2}-\d{2})T(\d{2})-(\d{2})-(\d{2})-(\d{3})Z_…\.jsonl` — the leading
`YYYY-MM-DDTHH-MM-SS-mmmZ` is the session-start UTC timestamp.

A file is included in scan iff `localMidnight(mtime) ≥ scanSince` **or**
`localMidnight(filenameStart) ≥ scanSince`.

### 1.4 Glob walking on Windows

Use the `walkdir` crate with `follow_links(false)`. Hidden-file skip + package-skip
on Mac translate to: skip entries whose filename starts with `.`. There are no Mac
bundles on Windows, so `skipsPackageDescendants` has no equivalent — ignore.

---

## 2. JSONL parsing — Claude Code

### 2.1 Line filter (cheap byte check before JSON parse)

A line is parsed only if the raw bytes contain **both** substrings (literal,
case-sensitive):

```text
"type":"assistant"
"usage"
```

Lines that fail this filter — or that exceed 512 KiB — are skipped without
JSON parsing. `prefixBytes` equals `maxLineBytes` so the *tail* of a long line
(where `usage` may live after a huge `tool_use_result`) is preserved.

### 2.2 Required fields

After `serde_json::from_slice`, the line is kept iff:

| Path | Type | Required |
|---|---|---|
| `type` | string `"assistant"` | yes |
| `timestamp` | string ISO-8601 (e.g. `2026-05-12T14:33:21.123Z`) | yes |
| `message.model` | string | yes |
| `message.usage` | object | yes |
| `message.usage.input_tokens` | non-negative int | defaults 0 |
| `message.usage.output_tokens` | non-negative int | defaults 0 |
| `message.usage.cache_read_input_tokens` | non-negative int | defaults 0 |
| `message.usage.cache_creation_input_tokens` | non-negative int | defaults 0 |

If all four token counts are zero, the line is discarded.

### 2.3 Optional fields used for dedup / attribution

| Path | Use |
|---|---|
| `message.id` | dedup key part 1 |
| `requestId` | dedup key part 2 |
| `sessionId` / `session_id` / `metadata.sessionId` / `message.metadata.sessionId` | canonical dedup across files |
| `isSidechain` | bool, dedup tie-breaker |

### 2.4 Day-key derivation

Convert `timestamp` ISO-8601 to a local-time day key `YYYY-MM-DD`. The fast path
parses the leading 19 chars + trailing `Z`/`±HH:MM` offset by hand
(no regex); fallback `chrono::DateTime::parse_from_rfc3339`. Then format the
local-time calendar date using the host's TimeZone.

### 2.5 Vertex AI detection (used to filter `claude` vs `vertexai`)

A row is "Vertex AI" iff **any** of:

1. `message.id` contains `_vrtx_` (e.g. `msg_vrtx_0154…`).
2. `requestId` contains `_vrtx_` (e.g. `req_vrtx_011…`).
3. `message.model` matches `^claude-.*@.*` (Vertex uses `@version`, Anthropic uses `-version`).
4. Any nested object key contains `vertex` or `gcp` (case-insensitive).
5. A scalar value under any of the keys
   `provider`, `platform`, `backend`, `api_provider`, `apiprovider`, `api_type`,
   `apitype`, `source`, `vendor`, `client` contains the literal `vertex`
   (case-insensitive).

Provider filter modes:

| Mode | Keep rows | Used for |
|---|---|---|
| `all` | all | combined |
| `vertexAIOnly` | only matches | `vertexai` provider |
| `excludeVertexAI` | only non-matches | `claude` provider |

If `vertexai` selection yields zero rows and `allowVertexClaudeFallback` is on,
re-run with `all` (callers opt in for empty-state fallback).

### 2.6 Edge cases

- Truncated lines (exceeded `maxLineBytes`) are silently dropped.
- Unknown model → row is still counted in token totals, cost contribution = nil.
- A streaming chunk that re-emits cumulative usage **replaces** prior chunks
  (see §3).
- `tool_use` / `tool_result` lines: skipped because they lack `usage`.

---

## 3. Deduplication

The dedup story is the subtlest part of the system. There are three layers.

### 3.1 In-file (streaming chunk) dedup

Within a single `.jsonl`, the assistant CLI may emit multiple lines with the
same `(message.id, requestId)` pair as a response streams in. **Usage in these
chunks is cumulative**: the final chunk holds the true totals. The scanner keeps
the last seen chunk per `messageId:requestId` key:

```rust
let key = format!("{}:{}", message_id, request_id);
keyed_rows.insert(key, row);   // overwrite
```

Rows missing either id are kept verbatim as "unkeyed" rows (older Claude CLI logs).

### 3.2 In-file scan output

`keyed_rows` is sorted by key (deterministic), then concatenated with `unkeyed_rows`.
This list (`Vec<ClaudeUsageRow>`) is what the per-file cache stores.

### 3.3 Cross-file dedup (canonical row key)

When **rebuilding** the day-aggregate from all cached files, a canonical key
`sessionId:messageId:requestId` deduplicates across files (some Claude flows
write the same assistant turn into a parent log *and* a subagent log).

Tie-break (`claudeRowWins`):

1. Prefer **non-sidechain** over sidechain.
2. Prefer `parent` pathRole over `subagent`.
3. Else lexicographic by file path.

Rows missing any of the three id parts are *not* deduped across files
(they all contribute).

### 3.4 Codex dedup

Codex sessions are identified by `session_id` (parsed from the first `session_meta`
line). If two files share a `session_id`, the second copy is dropped. Two
additional safeguards:

- `seenFileIds` — by OS file-resource identifier (inode-like). Windows: use
  `winapi::GetFileInformationByHandleEx` → `FileIdInfo` (volume serial + 128-bit
  file id) as the dedup key.
- Codex **forks**: when a `session_meta` carries `forked_from_id`, the scanner
  walks the parent file's `event_msg/token_count` snapshots and subtracts the
  parent's totals at-or-before the fork timestamp from the child's deltas.

---

## 4. JSONL parsing — Codex sessions

### 4.1 Line types of interest

Cheap byte filter: line must contain one of `"type":"event_msg"`,
`"type":"turn_context"`, or `"type":"session_meta"`. `event_msg` lines must also
contain `"token_count"`.

### 4.2 `session_meta` (first line)

```json
{"type":"session_meta","timestamp":"…","payload":{
  "session_id":"…","forked_from_id":"…","timestamp":"…"}}
```

Fields read (with fallbacks):

| Logical | First match wins |
|---|---|
| sessionId | `payload.session_id` → `payload.sessionId` → `payload.id` → top-level `session_id` / `sessionId` / `id` |
| forkedFromId | `payload.forked_from_id` → `payload.forkedFromId` → `payload.parent_session_id` → `payload.parentSessionId` |
| forkTimestamp | `payload.timestamp` → top-level `timestamp` |

### 4.3 `turn_context`

Sets the *current model* for subsequent `event_msg/token_count` lines:
`payload.model` else `payload.info.model`.

### 4.4 `event_msg` with `payload.type == "token_count"`

```json
{"type":"event_msg","timestamp":"…","payload":{
   "type":"token_count",
   "info":{
     "model":"gpt-5.1-codex",
     "total_token_usage":{"input_tokens":N,"cached_input_tokens":M,"output_tokens":K},
     "last_token_usage":  {"input_tokens":dN,"cached_input_tokens":dM,"output_tokens":dK}
   }}}
```

Either `total_token_usage` (preferred) or `last_token_usage` is present.
`cached_input_tokens` may be named `cache_read_input_tokens` — accept both.

### 4.5 Delta computation

Maintain `previousTotals = (input, cached, output)` across lines.

- **With `total_token_usage`**: `delta = max(0, total - previousTotals)` per axis,
  with `total` first reduced by `inheritedTotals` (fork subtraction). `previousTotals = total`.
  `remainingInheritedTotals` is cleared once any total snapshot is seen.
- **With `last_token_usage` only**: `delta = max(0, last - remainingInheritedTotals)`
  per axis (consume inheritance), then `previousTotals += delta`.

After the delta:

```text
cached_clamp = min(deltaCached, deltaInput)
add(dayKey, model, input=deltaInput, cached=cached_clamp, output=deltaOutput)
```

`cached_clamp` prevents a misbehaving server from charging more cached input
tokens than total input tokens.

### 4.6 Model fallback chain

`currentModel` (from `turn_context`) → `info.model` → `info.model_name` →
`payload.model` → `obj.model` → literal `"gpt-5"`.

After lookup the name is normalized (§6.2): strip `openai/` prefix; strip a
trailing `-YYYY-MM-DD` if the bare name matches a known model.

---

## 5. JSONL parsing — pi (Practical Intelligence) sessions

A pi session is a heterogeneous stream of `type="message" / role="assistant"`
turns from multiple providers. The scanner attributes each assistant turn to
either Claude (`anthropic`) or Codex (`openai-codex`) and *only* those.

### 5.1 Relevant line types

- `{"type":"model_change","provider":"...","modelId":"..."}` — updates a
  rolling `currentModelContext` (PiModelContext). Other providers are ignored
  (yields `nil`, so the context is cleared).
- `{"type":"message","message":{"role":"assistant","provider":"…","model":"…",
  "timestamp":"…","usage":{…}}}` — produces a usage row.

### 5.2 Identity resolution per assistant turn

1. Read explicit provider text (preferred `message.provider`, else `obj.provider`).
2. Map provider:
   - `"anthropic"` → `claude`
   - `"openai-codex"` → `codex`
   - else: row dropped.
3. Read explicit model (`message.model` → `obj.model` → `message.modelId` → `obj.modelId`).
4. If explicit provider+model present → use them.
5. If only explicit provider matches the fallback context's provider, use
   fallback model.
6. If only explicit model present, inherit fallback's provider.
7. If neither, use full fallback context.
8. If explicit provider text exists but maps to no known provider → drop.

### 5.3 Usage object — accept many key shapes

| Logical | Accepted keys |
|---|---|
| input | `input` / `inputTokens` / `input_tokens` / `promptTokens` / `prompt_tokens` |
| cache read | `cacheRead` / `cacheReadTokens` / `cache_read` / `cache_read_tokens` / `cacheReadInputTokens` / `cache_read_input_tokens` |
| cache write | `cacheWrite` / `cacheWriteTokens` / `cache_write` / `cache_write_tokens` / `cacheCreationTokens` / `cache_creation_tokens` / `cacheCreationInputTokens` / `cache_creation_input_tokens` |
| output | `output` / `outputTokens` / `output_tokens` / `completionTokens` / `completion_tokens` |
| total | `totalTokens` / `total_tokens` / `tokenCount` / `token_count` / `tokens` |

`totalTokens = max(directTotal, input+cacheRead+cacheWrite+output)`.

### 5.4 Timestamp & day bucket

`message.timestamp` then `obj.timestamp`. Accept ISO-8601 string, numeric seconds,
or numeric milliseconds (> 1e12 → divide by 1000). Bucket by **local-time**
calendar day. Because a single pi session may span days/models, one file
contributes to multiple `(provider, dayKey, model)` cells.

### 5.5 Cost is computed per-message and frozen

Unlike Claude where cost is recomputed from token aggregates at report time,
pi rows compute cost **per message** using the per-token rates (Claude tiered
thresholds apply per-message), then sum as `costNanos = round(usd * 1e9)`.
This preserves Claude's 200k-token threshold (long-context pricing) — see §6.4.

The aggregator records `costSampleCount` and `usageSampleCount`; the report
prefers the cached per-message sum when all rows priced cleanly
(`costSampleCount == usageSampleCount`), else recomputes from aggregates as a
fallback.

---

## 6. Pricing tables

### 6.1 Sources, units, fallback

| Source | Form | TTL | Fallback |
|---|---|---|---|
| Hardcoded table (built into binary) | per-token USD | none | always present |
| `models.dev` catalog | per-1M-tokens USD, fetched from `https://models.dev/api.json` | 24h | falls back to hardcoded table on miss |

Cache file (Mac): `~/Library/Caches/CodexBar/model-pricing/models-dev-v1.json`.
**Windows**: `%LOCALAPPDATA%\CodexBar\cache\model-pricing\models-dev-v1.json`.

`models.dev` per-1M values are divided by `1_000_000` at load time → per-token rate.
When `cost.context_over_200k` is present, those are used as the
`>200k` tier with `thresholdTokens = 200_000`.

**Lookup order** for `(providerId, modelId)`:

1. `models.dev` catalog — try the raw id, then variant candidates: strip
   `openai/`, strip `anthropic.`, take tail after last `.` if it starts with
   `claude-`, split on `@`, strip trailing `-YYYY-MM-DD`, strip trailing
   `-YYYYMMDD`, strip trailing `-vN:M`.
2. Hardcoded table (below).
3. Otherwise → cost is `nil` (token totals still reported; cost column empty).

Currency: **USD** throughout. No FX. CLI/UI formats with `UsageFormatter.usdString`.

### 6.2 Model name normalization

**Codex** (`normalizeCodexModel`):

- Strip leading `openai/`.
- If trailing `-YYYY-MM-DD` and the base is in the table → use base.

**Claude** (`normalizeClaudeModel`):

- Strip leading `anthropic.`.
- If the string contains `claude-` and has a `.` separator, keep only the tail
  beginning with `claude-`.
- Strip trailing `-vN:M`.
- If trailing `-YYYYMMDD` and the base is in the table → use base.

### 6.3 Hardcoded **Codex** pricing (USD per token)

| Model | input | output | cacheRead | label |
|---|---:|---:|---:|---|
| gpt-5 | 1.25e-6 | 1e-5 | 1.25e-7 | — |
| gpt-5-codex | 1.25e-6 | 1e-5 | 1.25e-7 | — |
| gpt-5-mini | 2.5e-7 | 2e-6 | 2.5e-8 | — |
| gpt-5-nano | 5e-8 | 4e-7 | 5e-9 | — |
| gpt-5-pro | 1.5e-5 | 1.2e-4 | — | — |
| gpt-5.1 | 1.25e-6 | 1e-5 | 1.25e-7 | — |
| gpt-5.1-codex | 1.25e-6 | 1e-5 | 1.25e-7 | — |
| gpt-5.1-codex-max | 1.25e-6 | 1e-5 | 1.25e-7 | — |
| gpt-5.1-codex-mini | 2.5e-7 | 2e-6 | 2.5e-8 | — |
| gpt-5.2 | 1.75e-6 | 1.4e-5 | 1.75e-7 | — |
| gpt-5.2-codex | 1.75e-6 | 1.4e-5 | 1.75e-7 | — |
| gpt-5.2-pro | 2.1e-5 | 1.68e-4 | — | — |
| gpt-5.3-codex | 1.75e-6 | 1.4e-5 | 1.75e-7 | — |
| gpt-5.3-codex-spark | 0 | 0 | 0 | "Research Preview" |
| gpt-5.4 | 2.5e-6 | 1.5e-5 | 2.5e-7 | — |
| gpt-5.4-mini | 7.5e-7 | 4.5e-6 | 7.5e-8 | — |
| gpt-5.4-nano | 2e-7 | 1.25e-6 | 2e-8 | — |
| gpt-5.4-pro | 3e-5 | 1.8e-4 | — | — |
| gpt-5.5 | 5e-6 | 3e-5 | 5e-7 | — |
| gpt-5.5-pro | 3e-5 | 1.8e-4 | — | — |

Cost math:

```rust
let cached = clamp(cachedInputTokens, 0, inputTokens);
let nonCached = inputTokens - cached;
let cachedRate = pricing.cacheReadInputCostPerToken.unwrap_or(pricing.inputCostPerToken);
cost = nonCached * pricing.input + cached * cachedRate + outputTokens * pricing.output;
```

### 6.4 Hardcoded **Claude** pricing (USD per token)

| Model | input | output | cacheCreate | cacheRead | threshold | input>thr | output>thr | cacheCreate>thr | cacheRead>thr |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| claude-haiku-4-5-20251001 | 1e-6 | 5e-6 | 1.25e-6 | 1e-7 | — | — | — | — | — |
| claude-haiku-4-5 | 1e-6 | 5e-6 | 1.25e-6 | 1e-7 | — | — | — | — | — |
| claude-opus-4-5-20251101 | 5e-6 | 2.5e-5 | 6.25e-6 | 5e-7 | — | — | — | — | — |
| claude-opus-4-5 | 5e-6 | 2.5e-5 | 6.25e-6 | 5e-7 | — | — | — | — | — |
| claude-opus-4-6-20260205 | 5e-6 | 2.5e-5 | 6.25e-6 | 5e-7 | — | — | — | — | — |
| claude-opus-4-6 | 5e-6 | 2.5e-5 | 6.25e-6 | 5e-7 | — | — | — | — | — |
| claude-opus-4-7 | 5e-6 | 2.5e-5 | 6.25e-6 | 5e-7 | — | — | — | — | — |
| claude-sonnet-4-5 | 3e-6 | 1.5e-5 | 3.75e-6 | 3e-7 | 200000 | 6e-6 | 2.25e-5 | 7.5e-6 | 6e-7 |
| claude-sonnet-4-6 | 3e-6 | 1.5e-5 | 3.75e-6 | 3e-7 | 200000 | 6e-6 | 2.25e-5 | 7.5e-6 | 6e-7 |
| claude-sonnet-4-5-20250929 | 3e-6 | 1.5e-5 | 3.75e-6 | 3e-7 | 200000 | 6e-6 | 2.25e-5 | 7.5e-6 | 6e-7 |
| claude-opus-4-20250514 | 1.5e-5 | 7.5e-5 | 1.875e-5 | 1.5e-6 | — | — | — | — | — |
| claude-opus-4-1 | 1.5e-5 | 7.5e-5 | 1.875e-5 | 1.5e-6 | — | — | — | — | — |
| claude-sonnet-4-20250514 | 3e-6 | 1.5e-5 | 3.75e-6 | 3e-7 | 200000 | 6e-6 | 2.25e-5 | 7.5e-6 | 6e-7 |

Tiered cost math — applied **independently per axis**, where N is the axis token
count:

```rust
fn tiered(n: u64, base: f64, above: Option<f64>, threshold: Option<u64>) -> f64 {
    match (threshold, above) {
        (Some(t), Some(a)) => {
            let below = n.min(t) as f64;
            let over  = n.saturating_sub(t) as f64;
            below * base + over * a
        }
        _ => (n as f64) * base,
    }
}
```

Total cost = tiered(input) + tiered(cacheRead) + tiered(cacheCreate) + tiered(output).

Note: pi-session cost is computed per-message (so the threshold applies per
message). Claude cost from JSONL aggregates is computed once at report time
from daily totals (the threshold *is* applied to daily aggregates — this is a
known approximation that matters only above ≥200k tokens per day per axis).

### 6.5 Unknown model handling

- Token totals: always counted.
- Per-model breakdown entry: present with `costUSD = null`.
- Daily totals: nil-coalesce; missing values do not propagate as zero.
- Summary `totalCostUSD`: only set if **at least one** row priced cleanly.

---

## 7. Aggregation

### 7.1 Day-keyed grouping

The unit of aggregation is the local-calendar-day string `YYYY-MM-DD`. Keys are
formed from each event's local-time year/month/day.

### 7.2 Rolling 30-day window

```text
until  = now
since  = now - 29 days   // inclusive (29 days back + today = 30 days)
sinceKey = local YYYY-MM-DD of since
untilKey = local YYYY-MM-DD of until
scanSinceKey = sinceKey - 1 day  // scan widens by ±1 day for TZ slop
scanUntilKey = untilKey + 1 day
```

The cache stores `scan*` keys (wider). The report restricts entries to
`sinceKey..=untilKey`.

### 7.3 Per-day aggregate (Claude)

Internal packed tuple per `(dayKey, model)`:

```text
[input, cacheRead, cacheCreate, output, costNanos, sampleCount, pricedSampleCount]
```

`costNanos` is `round(usd * 1e9)` (Int storage to avoid f64 drift). When
`pricedSampleCount == sampleCount` and `sampleCount > 0`, the report uses the
cached cost; otherwise it recomputes from the token totals + current pricing.

### 7.4 Per-day aggregate (Codex)

Packed tuple per `(dayKey, model)`:

```text
[input, cached, output]
```

Cost is **always** recomputed from token totals at report time
(Codex has no tiered pricing).

### 7.5 Daily report entry shape

```ts
type DailyEntry = {
  date: "YYYY-MM-DD",
  inputTokens?: number,
  outputTokens?: number,
  cacheReadTokens?: number,
  cacheCreationTokens?: number,
  totalTokens?: number,         // input+cacheRead+cacheCreate+output
  costUSD?: number,
  modelsUsed?: string[],        // sorted asc
  modelBreakdowns?: {
    modelName: string,
    costUSD?: number,
    totalTokens?: number,
  }[]                           // sorted by costUSD desc, then tokens desc, then name desc
};
```

### 7.6 Merging Claude + pi reports

`CostUsageDailyReport.merged([claudeReport, piReport])`:

- Group by `date` key.
- Sum every token axis.
- Sum costUSD only across entries that *report* a costUSD; if at least one entry
  has cost, summed cost is present.
- Union `modelsUsed` (sorted).
- Merge `modelBreakdowns` by `modelName` — same sum rules.
- Recompute summary across the merged entries.

### 7.7 Token snapshot (UI consumer shape)

```ts
type CostUsageTokenSnapshot = {
  sessionTokens?: number,           // most recent day's totalTokens
  sessionCostUSD?: number,
  last30DaysTokens?: number,        // summary.totalTokens else sum of daily
  last30DaysCostUSD?: number,
  daily: DailyEntry[],
  updatedAt: string,                // ISO-8601
};
```

"Most recent day" = max `parsedDate(date)`, ties broken by costUSD asc then
totalTokens asc then date string asc (stable).

---

## 8. Cache

### 8.1 Files

| File (Mac path) | Purpose | Schema version |
|---|---|---|
| `~/Library/Caches/CodexBar/cost-usage/codex-v4.json` | Codex daily aggregate + per-file rows | 4 |
| `~/Library/Caches/CodexBar/cost-usage/claude-v2.json` | Claude daily aggregate + per-file rows | 2 |
| `~/Library/Caches/CodexBar/cost-usage/vertexai-v2.json` | Vertex AI (Claude API filter) | 2 |
| `~/Library/Caches/CodexBar/cost-usage/pi-sessions-v1.json` | pi packed contributions | 2 (artifactVersion) |
| `~/Library/Caches/CodexBar/model-pricing/models-dev-v1.json` | models.dev catalog snapshot | 1 |

**Windows mapping**: `%LOCALAPPDATA%\CodexBar\cache\cost-usage\<provider>-vN.json`
and `…\model-pricing\models-dev-v1.json`. Same filenames, same JSON.

Note: `artifactVersion` for pi cache is **2** even though the filename token says
`v2`. The Codex/Claude `version` field inside the JSON is `1` (the file schema
version) and the file-name N is the *artifact* version (bumped to invalidate the
cache on shape changes).

### 8.2 Codex/Claude cache schema

```jsonc
{
  "version": 1,
  "lastScanUnixMs": 1715500000000,
  "files": {
    "<absolute path>": {
      "mtimeUnixMs": 1715000000000,
      "size": 1234567,
      "parsedBytes": 1234500,       // for incremental scan
      "days": { "2026-05-12": { "gpt-5.1-codex": [in, cached, out] } },
      "lastModel": "gpt-5.1-codex",
      "lastTotals": { "input": N, "cached": M, "output": K },
      "sessionId": "…",
      "forkedFromId": "…",
      "claudeRows": [/* ClaudeUsageRow[] — claude/vertexai caches only */]
    }
  },
  "days": {
    "2026-05-12": { "claude-sonnet-4-5": [in, cacheRead, cacheCreate, out, costNanos, samples, priced] }
  },
  "roots": { "<root path>": 0 }       // Codex only; fingerprint to detect root-set change
}
```

### 8.3 pi-sessions cache schema

```jsonc
{
  "version": 2,
  "lastScanUnixMs": …,
  "scanSinceKey": "YYYY-MM-DD",       // last scan window
  "scanUntilKey": "YYYY-MM-DD",
  "daysByProvider": {
    "claude": { "2026-05-12": { "claude-sonnet-4-5": <PiPackedUsage> } },
    "codex":  { … }
  },
  "files": {
    "<path>": {
      "mtimeUnixMs": …, "size": …, "parsedBytes": …,
      "lastModelContext": { "providerRawValue": "claude", "modelName": "…" },
      "contributions": { "<provider>": { "<dayKey>": { "<model>": <PiPackedUsage> } } }
    }
  }
}
```

`PiPackedUsage`:

```ts
{
  inputTokens: number, cacheReadTokens: number, cacheWriteTokens: number,
  outputTokens: number, totalTokens: number,
  costNanos: number /* i64 */, costSampleCount: number, usageSampleCount?: number
}
```

### 8.4 Invalidation rule

Per-file: `(mtimeUnixMs, size)` tuple. **No hash** — mtime+size is the contract.

- If both match cached → skip parse entirely.
- If `size > cached.size && cached.parsedBytes ≤ size && cached.parsedBytes > 0`
  → **incremental** scan from `startOffset = cached.parsedBytes` to EOF, merging
  rows into the cached row list (rekey by `messageId:requestId` so streaming
  chunks coalesce across scans).
- Else → full re-parse, with old contributions backed out
  (`applyFileDays(..., sign: -1)`) before re-applying.

Cache file is **versioned by filename** (`-vN.json`). If the loaded JSON's
`version != 1` (Codex/Claude) or `decoded.version != artifactVersion` (pi) →
discard cache and start fresh.

Cross-cache invalidation:

- `cache.roots` fingerprint mismatch (Codex) → cold-cache lookback.
- Window expansion (pi scan range wider than cached `scanSinceKey/scanUntilKey`)
  → force rescan.
- `forceRescan` (CLI `--refresh`) → clear cache.

### 8.5 Atomic write

Write to `<dir>/.tmp-<uuid>.json` then `replaceItemAt` (Windows: `MoveFileExW`
with `MOVEFILE_REPLACE_EXISTING`, or Rust `std::fs::rename` after `remove_file`
fallback). Never partial-write the canonical file.

---

## 9. Cost history chart (UI consumer)

`CostHistoryChartMenuView` reads `daily: DailyEntry[]` + an optional
`totalCostUSD` and:

- Filters entries with `costUSD >= 0` and a parseable date.
- Sorts ascending by date.
- Renders one bar per day at `y = costUSD`.
- Caps the peak bar with a yellow accent (rectangle of height `peakCost * 0.05`).
- Axis ticks: first + last date only, `Mar 12`-style formatting.
- Hover tooltip:

  ```text
  <Month Day>: <USD>[ · <tokens> tokens]
  <model₁ display name>
    <model₁ cost · model₁ tokens>
  …up to 4 rows, sorted by cost desc
  ```

- Footer: `Total (30d): $X.YZ`.

Cross-platform note: the chart is rendered in React on Windows. The Rust
back-end emits the exact same `DailyEntry[]` plus the snapshot fields.

---

## 10. Cost projection (Codex consumer)

The Codex *consumer* plan (paid ChatGPT plan; not cost-usage) has a separate
projection in `CodexConsumerProjection`. It does NOT use local JSONL data.
Inputs: live `UsageSnapshot` (rate windows: 5h "session" / 7d "weekly"),
live `CreditsSnapshot`, live `OpenAIDashboardSnapshot`. Outputs:

- `visibleRateLanes` — which of (session, weekly) to surface.
- `planUtilizationLanes` — semantic order session → weekly.
- `creditsProjection.remaining` — credits remaining, surfaced as menu-bar
  fallback only when **any** rate lane is fully exhausted (`remainingPercent ≤ 0`)
  or no rate lanes exist.
- `userFacingErrors` — passes the raw error string through `CodexUIErrorMapper`
  which rewrites known patterns (e.g. `token_expired` → "Codex session expired.
  Sign in again.").

There is **no monthly-cost projection from JSONL data** in this codebase
(no smoothing, no extrapolation). The "projection" surface is purely
plan-utilization + credits.

---

## 11. Provider storage footprint (sibling feature)

This subsystem reports byte sizes of provider-owned local directories.
**It must never delete anything** — surfacing data only.

### 11.1 Candidate paths

| Provider | Mac | Windows |
|---|---|---|
| codex | `CodexHomeScope.ambientHomeURL(env, fileManager).path` (resolves `CODEX_HOME` or `~/.codex`) + managed account home paths | `%CODEX_HOME%` else `%USERPROFILE%\.codex` + managed account homes |
| claude | `~/.claude`, `~/.config/claude`, `~/Library/Application Support/CodexBar/ClaudeProbe` | `%USERPROFILE%\.claude`, `%USERPROFILE%\.config\claude`, `%APPDATA%\CodexBar\ClaudeProbe` |
| gemini | `~/.gemini`, `~/.config/gemini` | `%USERPROFILE%\.gemini`, `%USERPROFILE%\.config\gemini` |
| opencode | `~/.config/opencode` | `%USERPROFILE%\.config\opencode` |
| copilot | `~/.config/github-copilot` | `%USERPROFILE%\.config\github-copilot` |

Each candidate is standardized (resolve `~`, canonicalize) and deduped.

### 11.2 Output

```ts
type ProviderStorageFootprint = {
  provider: UsageProvider,
  totalBytes: number,
  paths: string[],            // existing roots
  missingPaths: string[],
  unreadablePaths: string[],
  components: { id: string, path: string, totalBytes: number }[],
  updatedAt: string
};
```

`components` = first-level children of each existing root with their recursive
byte sizes, sorted by size desc.

### 11.3 Scan rules

- Use a `WalkDir`-like enumerator.
- Skip symlinks entirely (don't follow, don't count).
- Add only regular file sizes.
- Unreadable entries → push path to `unreadablePaths`, continue.
- Cancelable: bail out cleanly if a cancel signal fires (Windows: token).

### 11.4 Recommendations

`ProviderStorageRecommendation` maps named component dirs to manual-cleanup
copy. **It only labels** — does not act.

Examples (Claude): `projects`, `file-history`, `plans`, `debug`, `paste-cache`,
`image-cache`, `session-env`, `shell-snapshots`, `todos`.
Examples (Codex): `sessions`, `archived_sessions`, `cache`, `logs`,
`logs_*.sqlite`, `file-history`, `paste-cache`, `image-cache`, `shell-snapshots`,
`tmp`, `temp`, `.tmp`.

Risk level: always `manualCleanup` (informational). UI surfaces a button that
opens the directory in Explorer — **never** a delete button.

### 11.5 Throttling

`UsageStore+ProviderStorage` enforces:

- Automatic refresh interval: 5 minutes.
- Coalesced by *signature* = sorted `(provider, paths)` joined with unit
  separators `\u{1f}` between paths and `\u{1e}` between providers.
- New request with same signature & a refresh in-flight → no-op.
- Same signature & last refresh within 5min & not `force=true` → no-op.

### 11.6 Opt-in flag

`SettingsStore.providerStorageFootprintsEnabled`. When off, the store clears
all footprints. Default is whatever was last persisted; the auto-default
trigger is keyed off "are there any token-cost JSONL sources on disk"
(see §12.2).

---

## 12. Concurrency / throttling

### 12.1 Cost scans

- Triggered by `CostUsageFetcher.loadTokenSnapshot(provider, now, forceRefresh)`.
- Scans run **off the main thread** (Tauri: `tauri::async_runtime::spawn_blocking`).
- Per-provider mutex (in-flight set: `tokenRefreshInFlight`). A second request
  for the same provider while in-flight → coalesced (return the same future).
- `refreshMinIntervalSeconds = 60`. A scan within 60s of the last completion is
  skipped unless `forceRefresh`.
- `models.dev` refresh runs **before** the scan, awaited, best-effort
  (failure → keep last good cache).
- `forceRescan` (CLI `--refresh` or explicit user action): clears in-memory
  cache state and reparses every file.

### 12.2 Auto-enable

If `tokenCostUsageEnabled` setting has never been set, do a one-shot probe:
enumerate the union of Claude project roots + Codex sessions root + Codex archived
root for the first `.jsonl`. If any → set the setting to `true`. Otherwise leave
unset (UI off).

### 12.3 Cancellation

Honor cancellation tokens at every directory enumeration boundary and after every
file scan. Never partially mutate the cache: in-flight contributions are applied
only after the file fully parses.

---

## 13. Performance

### 13.1 Targets

| Workload | Target |
|---|---|
| Cold scan, 30 days, ~100 Claude sessions (~200 MB total) | < 5 s on an SSD |
| Warm scan, 30 days, no changes | < 50 ms (only directory enumeration + mtime/size compare) |
| Single file incremental (1 MB appended) | < 10 ms |

### 13.2 IO budget

- Read in 256 KiB chunks (`handle.read(upToCount: 256 * 1024)`).
- Per-line cap **Claude** 512 KiB; **Codex** 256 KiB; **pi** 16 MiB
  (pi files can carry large tool outputs interleaved with assistant turns).
- A line that exceeds its cap is dropped intact — *do not* truncate-then-parse.

### 13.3 File-size order of magnitude

Observed:

- Typical Claude `.jsonl` session: 200 KiB – 8 MiB.
- Long Codex session: 50 KiB – 2 MiB.
- pi sessions: routinely 5 MiB – 50 MiB (multi-day, multi-provider transcripts).

### 13.4 Memory

The scanner never holds an entire file in memory; it streams line-by-line. The
canonical state for the 30-day report fits in low single-digit MiB
(per-day × per-model rows).

---

## 14. CLI surfaces

### 14.1 Command

```text
codexbar cost [--provider codex|claude|both] [--format text|json]
              [--json] [--json-only] [--pretty] [--no-color]
              [--refresh] [--log-level …] [--verbose] [-v]
              [--json-output]
```

`--provider both` (default) is the alias for "all supported"; non-Claude/non-Codex
providers in selection get a stderr warning and are skipped.

### 14.2 Text output (per provider)

```text
<Provider Display Name> Cost (local)
Today: $X.YZ · 12.3M tokens
Last 30 days: $X.YZ · 123M tokens
```

The header is bold cyan when stdout is a TTY and `--no-color` is unset.

### 14.3 JSON output

```jsonc
[
  {
    "provider": "claude",
    "source": "local",
    "updatedAt": "2026-05-12T14:33:21.123Z",
    "sessionTokens": 12345,
    "sessionCostUSD": 0.27,
    "last30DaysTokens": 1234567,
    "last30DaysCostUSD": 12.34,
    "daily": [
      { "date": "2026-05-12",
        "inputTokens": …, "outputTokens": …, "cacheReadTokens": …,
        "cacheCreationTokens": …, "totalTokens": …,
        "totalCost": 0.27,
        "modelsUsed": ["claude-sonnet-4-5"],
        "modelBreakdowns": [
          {"modelName":"claude-sonnet-4-5","cost":0.27,"totalTokens":12345}
        ]
      }
    ],
    "totals": {
      "inputTokens": …, "outputTokens": …, "cacheReadTokens": …,
      "cacheCreationTokens": …, "totalTokens": …, "totalCost": 12.34
    },
    "error": null
  }
]
```

Field renames for the JSON wire format:

| Daily entry field (internal) | JSON key |
|---|---|
| costUSD | `totalCost` |
| ModelBreakdown.costUSD | `cost` |
| Totals.totalInputTokens | `inputTokens` |
| Totals.totalOutputTokens | `outputTokens` |
| Totals.totalCostUSD | `totalCost` |

### 14.4 Exit codes

- 0: all providers succeeded.
- non-zero: a provider returned an error; the JSON entry's `error` is populated
  with the mapped CLI error payload. Text mode prints to stderr.

---

## 15. Mac → Windows mapping summary

| Concern | Mac | Windows |
|---|---|---|
| Claude project roots | `$CLAUDE_CONFIG_DIR` ∥ `~/.config/claude/projects` ∥ `~/.claude/projects` | `%CLAUDE_CONFIG_DIR%` ∥ `%USERPROFILE%\.config\claude\projects` ∥ `%USERPROFILE%\.claude\projects` |
| Codex sessions | `$CODEX_HOME/sessions` ∥ `~/.codex/sessions` + sibling `archived_sessions` | `%CODEX_HOME%\sessions` ∥ `%USERPROFILE%\.codex\sessions` + sibling `archived_sessions` |
| pi sessions | `~/.pi/agent/sessions` | `%USERPROFILE%\.pi\agent\sessions` |
| Cache root | `~/Library/Caches/CodexBar/cost-usage` | `%LOCALAPPDATA%\CodexBar\cache\cost-usage` |
| models.dev cache | `~/Library/Caches/CodexBar/model-pricing/models-dev-v1.json` | `%LOCALAPPDATA%\CodexBar\cache\model-pricing\models-dev-v1.json` |
| File walking | Foundation `FileManager.enumerator` | Rust `walkdir` (no follow_links, manual hidden-skip) |
| JSONL parse | `JSONSerialization.jsonObject` | `serde_json::from_slice` line-by-line |
| Atomic replace | `replaceItemAt` | `std::fs::rename` (Windows: `MoveFileExW`) |
| File identity | `URLResourceKey.fileResourceIdentifier` | `GetFileInformationByHandleEx` `FileIdInfo` |
| Path role detection | path contains `/subagents/` | path contains `\subagents\` **or** `/subagents/` |
| Local timezone | `Calendar.current` | `chrono::Local` |
| ISO timestamp | `ISO8601DateFormatter` | `chrono::DateTime::parse_from_rfc3339` |
| HTTP fetch (models.dev) | `URLSession` | `reqwest` (`rustls` TLS), 20s timeout |

JSONL files **are byte-for-byte identical** between Mac and Windows Claude /
Codex CLI installs (the CLIs write the same content; Windows uses `\r\n` line
endings sometimes — the scanner must tolerate trailing `\r` on lines, strip
it before JSON parse).

---

## 16. Acceptance checklist

To prove parity with macOS CodexBar:

1. **Fixture dataset** — capture a snapshot of:
   - `~/.codex/sessions/**/*.jsonl` (1 active + 1 forked + 1 archived).
   - `~/.claude/projects/**/*.jsonl` (parent + subagent + Vertex AI row + a
     streaming-chunk session with cumulative usage).
   - `~/.pi/agent/sessions/**/*.jsonl` (mixed anthropic + openai-codex turns,
     spanning two local days, including model_change lines).
   - A frozen `models.dev` catalog JSON.
2. **Mac baseline** — run `codexbar cost --provider both --json --pretty
   --refresh` on macOS, save the output.
3. **Windows port** — run the same command on the same fixtures (copy the
   JSONL trees verbatim into the Windows paths from §15) with the same frozen
   clock and `models.dev` cache.
4. **Diff** — JSON outputs must match exactly except `updatedAt` timestamps.
   Required equal:
   - `last30DaysCostUSD` (to the cent — *but* per §6.4 caveat about per-day
     vs per-message threshold pricing; document any reproducible delta).
   - `last30DaysTokens` (exact).
   - Per-day `costUSD`, `totalTokens`, sorted `modelsUsed`, sorted
     `modelBreakdowns`.
5. **Dedup edge cases**:
   - Same Claude session log present in both a Claude project root and a
     subagent path → counted exactly once, parent path wins.
   - Codex fork: child + parent both in scan → parent totals subtracted up to
     `forkTimestamp`; child totals reflect only post-fork tokens.
   - pi session re-scanned after appending 100 KiB → incremental scan parses
     only the appended bytes; per-day totals unchanged for earlier days.
6. **Cache shape** — written cache files round-trip cleanly via the Rust impl
   and (separately) the Swift impl (cross-load test, optional but recommended).
7. **Storage footprint** — recursive byte counts match `du -sb` on each
   candidate path (within file-system granularity).
8. **Provider safety** — fuzz `--provider` with every UsageProvider enum value
   except `codex`/`claude` and assert stderr warning + exit code mapping.

---

## Summary

Inputs the Rust port must read (every existence is opt-in):

- Codex: `%CODEX_HOME%\sessions` or `%USERPROFILE%\.codex\sessions`,
  plus sibling `archived_sessions`.
- Claude: `%CLAUDE_CONFIG_DIR%` (comma-split, each `\projects`),
  plus `%USERPROFILE%\.config\claude\projects` and `%USERPROFILE%\.claude\projects`.
- pi: `%USERPROFILE%\.pi\agent\sessions`.
- Cache: `%LOCALAPPDATA%\CodexBar\cache\cost-usage\<provider>-vN.json`
  and `%LOCALAPPDATA%\CodexBar\cache\model-pricing\models-dev-v1.json`.

Pricing tables: **20 Codex models** × 4 columns (input, output, cacheRead,
displayLabel) + **13 Claude models** × 9 columns (input, output, cacheCreate,
cacheRead, threshold, and the four >200k tier rates). Currency USD; per-token
units; tiered above 200k on Sonnet variants; fallback to models.dev catalog
(≤24h TTL) when a model is missing from the hardcoded tables.
