# Claude Provider — Windows Port Specification

> **Audience:** Rust + TypeScript engineer building the Windows menu-bar app (Tauri 2 + React + shared Rust crate). Re-implement Claude usage tracking with Phantom-wallet/Duolingo-level polish. **Do not read or port Swift code** — this document encodes every behavior, header, endpoint, scope, parse rule, and edge case you need.
>
> **Out of scope:** macOS-only paths (Safari binarycookies, Keychain, Security.framework). Where the macOS implementation does something Windows can't, this doc states *what the behavior must accomplish* and the Windows mechanism that replaces it.

---

## 0. Provider profile at a glance

| Field | Value |
| --- | --- |
| Display name | `Claude` |
| Session label | `Session` |
| Weekly label | `Weekly` |
| Opus-slot label | `Sonnet` *(legacy name; field surfaces whichever model-specific window the API returns)* |
| `supportsOpus` | `true` |
| `supportsCredits` | `false` (Claude is rate-limit-first, not credit-first) |
| `defaultEnabled` | `false` |
| `isPrimaryProvider` | `true` |
| `usesAccountFallback` | `false` |
| CLI binary name | `claude` (Windows: `claude.cmd` / `claude.exe` from npm shim) |
| Dashboard URL | `https://console.anthropic.com/settings/billing` |
| Subscription dashboard URL | `https://claude.ai/settings/usage` |
| Status page | `https://status.claude.com/` |
| Brand color (RGB) | `204, 124, 94` (warm clay/terracotta) |
| Cookie domain | `claude.ai` |

There are **three** runtime data paths plus **one** local-disk cost scanner:

1. **OAuth API** (`/api/oauth/usage` on `api.anthropic.com`) — preferred when the user has run `claude login` (or our in-app login flow).
2. **Web API** (`claude.ai/api/*`) — uses browser session cookies; only path that returns rich account email + plan billing fields.
3. **CLI PTY** (`claude` + `/usage` slash command) — fallback when neither OAuth nor cookies work; runs the actual `claude` TUI inside a pseudo-terminal and scrapes the rendered usage panel.
4. **Cost-usage scanner** — reads local JSONL session logs to compute token spend and dollar cost; runs alongside (not instead of) the live data paths.

---

## 1. Auto-pipeline (source selection)

### 1.1 Runtime concept

The app exposes two **runtimes**:

| Runtime | What it is | Default `auto` pipeline |
| --- | --- | --- |
| `app` | The menu-bar app (interactive, can prompt for credentials) | `oauth → cli → web` |
| `cli` | The headless `codexbar` CLI helper (no UI, must avoid prompts) | `web → cli` |

The user-facing Source picker has four options: `Auto / OAuth API / Web API (cookies) / CLI (PTY)`. Explicit picks bypass fallback and surface the source's concrete error.

### 1.2 The planner

A single function (Windows-side name suggestion: `claude_resolve_plan`) takes:

```rust
struct PlanInput {
    runtime: Runtime,                // App | Cli
    selected: ClaudeSource,          // Auto | OAuth | Web | Cli
    web_extras_enabled: bool,        // app-runtime + cli-source only
    has_web_session: bool,           // do we have a `sessionKey` cookie?
    has_cli: bool,                   // is `claude` on PATH (or pinned path)?
    has_oauth_credentials: bool,     // resolved silently, never prompts
}
```

…and returns an ordered list of `(source, reason, plausibly_available)`. Execution iterates the plan; on each step's failure the next is tried *only* if the planner produced it (auto mode); explicit modes have a single-element plan and the error propagates.

### 1.3 Fallback decision matrix

| Runtime | Mode | Ordered strategies | Fallback rule |
| --- | --- | --- | --- |
| `app` | `auto` | `oauth → cli → web` | OAuth → next on any failure. CLI → web only if web is actually available. Web is terminal. |
| `app` | `oauth` | `oauth` | None — surface error. |
| `app` | `cli` | `cli` | None. |
| `app` | `web` | `web` | None. |
| `cli` | `auto` | `web → cli` | Web → CLI on failure. CLI is terminal. |
| `cli` | any explicit | that one strategy | None. |

### 1.4 "Plausibly available" gates (per source)

Used to filter the plan *before* execution, so the menu doesn't try a guaranteed-fail source.

| Source | "Available" means |
| --- | --- |
| **OAuth** | env `CODEXBAR_CLAUDE_OAUTH_TOKEN` is non-empty, **or** a non-interactive credential read succeeds and returns a token whose `scopes` contains `user:profile` and is not expired. Expired but refreshable counts when the owner is `claudeCLI` or `codexbar` (with a refresh token). |
| **Web** | Manual cookie header parses to `sessionKey=sk-ant-...`, **or** at least one supported browser yields a `sessionKey` cookie for `claude.ai`. |
| **CLI** | A `claude` binary can be resolved (env override `CLAUDE_CLI_PATH`, then PATH walk). |

### 1.5 Consolidation TODO (port hygiene)

The macOS code has **two** `.auto` decision sites that disagree:

1. `ClaudeProviderDescriptor.resolveStrategies` — pipeline order `oauth → cli → web` (app).
2. `ClaudeUsageFetcher.loadLatestUsage(.auto)` — direct path used by debug probes, currently `oauth → web → cli`.

**Windows port:** collapse to one planner. The pipeline order is correct (`oauth → cli → web` in app, `web → cli` in CLI). Remove the second decision site. Single source of truth.

---

## 2. OAuth API path

### 2.1 Credential resolution order (silent first)

The store has a strict priority chain. **Each step is silent unless explicitly allowed to prompt.**

| Priority | Source | Owner tag | Notes |
| --- | --- | --- | --- |
| 1 | `CODEXBAR_CLAUDE_OAUTH_TOKEN` env var | `environment` | Optional companion: `CODEXBAR_CLAUDE_OAUTH_SCOPES` (space-separated scope list; treat as already containing `user:profile`). No refresh. |
| 2 | In-memory cache (30-minute TTL) | inherited | Used to avoid hammering disk/keychain on every menu open. |
| 3 | App-managed "cache keychain" (a CodexBar-owned entry, separate from Claude CLI's) | mostly `claudeCLI` | Stores the last good JSON for faster cold starts. **Windows: replace with DPAPI-encrypted file under `%LOCALAPPDATA%\CodexBar\cache\claude-oauth.bin`.** |
| 4 | `~/.claude/.credentials.json` file | `claudeCLI` | Plain JSON on disk. Owner is always `claudeCLI`. **Windows path:** `%USERPROFILE%\.claude\.credentials.json` *(Claude CLI on Windows writes here too)*. |
| 5 | Claude CLI's own Keychain entry (service `Claude Code-credentials`) | `claudeCLI` | macOS Keychain via Security.framework or `/usr/bin/security`. **Windows: skipped — Claude CLI on Windows stores in `.credentials.json` only, no Credential Manager use.** |

> The store invalidates its in-memory cache when the credentials *file* mtime/size changes (fingerprint stored as `ClaudeOAuthCredentialsFileFingerprintV2`).

### 2.2 Credentials JSON shape (`~/.claude/.credentials.json`)

The file has a single root key `claudeAiOauth`:

```json
{
  "claudeAiOauth": {
    "accessToken": "sk-ant-oat01-…",
    "refreshToken": "sk-ant-ort01-…",
    "expiresAt": 1735000000000,
    "scopes": ["user:profile", "user:inference"],
    "rateLimitTier": "claude_max",
    "subscriptionType": "max"
  }
}
```

Field rules:

| Field | Required | Type | Notes |
| --- | --- | --- | --- |
| `accessToken` | yes | string | Trim whitespace. Empty → `missingAccessToken`. |
| `refreshToken` | no | string | Required for `codexbar`-owned auto-refresh. |
| `expiresAt` | no | number (ms epoch) | If missing → treat as expired. |
| `scopes` | no | string array | **Must contain `user:profile`** to call usage endpoint. |
| `rateLimitTier` | no | string | e.g. `claude_max`, `claude_pro`, `claude_team`, `claude_enterprise`. |
| `subscriptionType` | no | string | e.g. `max`, `pro`, `team`. Preferred over `rateLimitTier` for plan inference. |

If the file is malformed → `decodeFailed`. If the root key is missing → `missingOAuth`. Both surface a "Run `claude` to authenticate" hint.

### 2.3 Scope validation

Before calling usage, assert `scopes.contains("user:profile")`. If not, raise:

> *"Claude OAuth token missing 'user:profile' scope (has: \<scopes\>). Run `claude setup-token` to re-generate credentials, or switch Claude Source to Web/CLI."*

Old CLI tokens with only `user:inference` will hit a 403 with a body containing `user:profile`; map that to the same message.

### 2.4 Usage endpoint

| Field | Value |
| --- | --- |
| Method | `GET` |
| URL | `https://api.anthropic.com/api/oauth/usage` |
| Header `Authorization` | `Bearer <accessToken>` |
| Header `Accept` | `application/json` |
| Header `Content-Type` | `application/json` |
| Header `anthropic-beta` | `oauth-2025-04-20` *(required)* |
| Header `User-Agent` | `claude-code/<detected-version>` (e.g. `claude-code/2.1.0`). Detect by running `claude --version` once and caching the result; fall back to `2.1.0` if detection fails. |
| Timeout | 30 s |

### 2.5 HTTP status handling

| Status | Action |
| --- | --- |
| `200` | Parse JSON, map (§2.6). |
| `401` | `unauthorized` — invalidate cached creds, surface "Run `claude` to re-authenticate". In `auto/app`, fall through to CLI/Web. |
| `403` | Server error with body. If body contains `user:profile`, raise the scope error. Otherwise show `HTTP 403 – <truncated body>` (≤400 chars). |
| 4xx/5xx | Generic server error with truncated body. |

### 2.6 Response → snapshot mapping

The response is a flat JSON object. Use dynamic key decoding (some fields are renamed by the server).

| JSON key | Maps to | Window minutes | Notes |
| --- | --- | --- | --- |
| `five_hour.utilization` | `primary` (session) | 300 | Required for "good" parse; first fallback for primary if missing. |
| `five_hour.resets_at` | session reset (ISO-8601) | | |
| `seven_day.utilization` | `secondary` (weekly all-models) | 10 080 | If `five_hour` missing, weekly becomes primary. |
| `seven_day_sonnet.utilization` | model-specific weekly (`tertiary`) | 10 080 | Preferred. |
| `seven_day_opus.utilization` | model-specific weekly (`tertiary`) | 10 080 | Used if `seven_day_sonnet` absent. |
| `seven_day_oauth_apps` | additional fallback for primary | 10 080 | Rare. |
| `extra_usage` | `providerCost` | n/a | See §2.7. |
| `seven_day_design` (or aliases) | named rate window "Designs" | 10 080 | See §2.8. |
| `seven_day_routines` (or aliases) | named rate window "Daily Routines" | 10 080 | See §2.8. |

`utilization` is `0–100` (percent **used**). If the server returns `null` for a known key but lists the key, treat as 0% (keep the bar visible).

Primary fallback chain when picking the "main" window: `five_hour → seven_day → seven_day_oauth_apps → seven_day_sonnet → seven_day_opus`. If none have a `utilization`, raise `parseFailed("missing session data")`.

### 2.7 `extra_usage` (Claude Extra spend/limit)

```json
{
  "extra_usage": {
    "is_enabled": true,
    "monthly_limit": 5000,
    "used_credits": 1834,
    "utilization": 36.68,
    "currency": "USD"
  }
}
```

Rules:

- Skip when `is_enabled` is not exactly `true`.
- Skip when `used_credits` or `monthly_limit` is null.
- **Values are in cents** — always divide by 100 for display.
- Default currency to `"USD"` if `currency` is empty.
- Period is hard-coded to `"Monthly"`. `resetsAt` is unknown.

Output struct:

```rust
ProviderCostSnapshot { used: 18.34, limit: 50.00, currency: "USD", period: "Monthly", resets_at: None }
```

### 2.8 Named rate windows ("extras")

The API surfaces optional product-specific weekly windows. They're identified by *any* of several aliases (the server name changes occasionally):

| Display id | Display title | Accepted keys (first match wins) |
| --- | --- | --- |
| `claude-design` | `Designs` | `seven_day_design`, `seven_day_claude_design`, `claude_design`, `design`, `seven_day_omelette`, `omelette`, `omelette_promotional` |
| `claude-routines` | `Daily Routines` | `seven_day_routines`, `seven_day_claude_routines`, `claude_routines`, `routines`, `routine`, `seven_day_cowork`, `cowork` |

If the key is present but the value is null, render a 0% bar (still visible). Log the matched source key for support diagnostics.

### 2.9 Plan inference (login method text)

Plan inference logic (used to render "Claude Max" / "Claude Pro" / etc. under the account row):

```text
plan = first_match(subscriptionType, rateLimitTier) -> Max | Pro | Team | Enterprise | Ultra
loginMethod = "Claude <Plan>"   // brandedLoginMethod
```

Substring matching (case-insensitive) on the tier string is fine: `claude_max → Max`, `enterprise → Enterprise`, etc. If neither field yields a match, `loginMethod` is `null`.

### 2.10 Token refresh (auto-refresh / delegated refresh)

There are **two** refresh modes depending on credential owner:

| Owner | Refresh mode | Endpoint / mechanism |
| --- | --- | --- |
| `codexbar` (we minted these) | **Direct refresh:** POST to `https://platform.claude.com/v1/oauth/token` with the refresh token + OAuth client id `9d1c250a-e61b-44d9-88ed-5944d1962f5e` (PKCE-style refresh, public client). Override via env `CODEXBAR_CLAUDE_OAUTH_CLIENT_ID`. | Standard OAuth refresh flow. |
| `claudeCLI` (CLI wrote them) | **Delegated refresh:** spawn `claude /status` inside a PTY for ≤15 s, then check whether the credentials file/keychain entry changed (fingerprint diff). | We never see refresh tokens; the CLI does it for us. |
| `environment` | No refresh. | User must update the env var. |

#### Delegated-refresh state machine

```text
attempt(now):
  if Task.isCancelled → attemptedFailed("Cancelled")
  if another attempt is in-flight → join its result
  if claude binary unavailable → cliUnavailable
  if last attempt within cooldown → skippedByCooldown
  baseline = snapshot(keychain fingerprint || file mtime)
  spawn `claude /status` (PTY, timeout=8s default, no /usage parsing)
  poll for change (up to 2s, delays [0.2, 0.5, 0.8])
  if observed change → recordAttempt(cooldown = 5 min) → attemptedSucceeded
  else → recordAttempt(cooldown = 20s) → attemptedFailed(reason)
```

Cooldowns persisted to user prefs as `claudeOAuthDelegatedRefreshLastAttemptAtV1` + `claudeOAuthDelegatedRefreshCooldownIntervalSecondsV1`. Long cooldown (5 min) on success, short (20 s) on failure to allow quick retry on transient hiccups.

The post-delegation retry then re-reads credentials with the prompt policy active. If the policy is `onlyOnUserAction` and we're in background, the retry is deferred — log "background recovery deferred until user action" and return an actionable error.

### 2.11 OAuth error → user message catalog

| Internal error | User-facing | Recovery action |
| --- | --- | --- |
| `notFound` | "Claude OAuth credentials not found." | Suggest running `claude` to authenticate. Show "Open Terminal: claude" menu action. |
| `missingOAuth` | "Claude OAuth credentials missing." | Same as above. |
| `missingAccessToken` | "Claude OAuth access token missing." | Same. |
| `decodeFailed` | "Claude OAuth credentials are invalid." | Re-login. |
| `noRefreshToken` | "Claude OAuth refresh token missing." | `claude login`. |
| `refreshFailed(msg)` | "Claude OAuth token refresh failed: <msg>" | Re-login. |
| `keychainError(status)` (macOS only) | "Claude Keychain access was denied …" | Switch source to Web/CLI, or allow access. |
| Scope error (403 with body containing `user:profile`) | "Claude OAuth token does not meet scope requirement 'user:profile'." | `claude setup-token`, or switch source. |
| Delegated refresh `cliUnavailable` | "Claude OAuth token expired and Claude CLI is not available for delegated refresh." | Install/configure `claude`, or `claude login`. |
| Delegated refresh `skippedByCooldown` | "Claude OAuth token expired and delegated refresh is cooling down." | Retry shortly or `claude login`. |
| Delegated refresh `attemptedFailed(msg)` | "Claude OAuth token expired and delegated Claude CLI refresh failed: <msg>." | `claude login`. |

In `auto/app`, OAuth failures fall through silently (don't show as the surface error). In explicit `oauth` mode, the error is the surface error.

---

## 3. Web API (cookies) path

### 3.1 Cookie source priority

When in `auto` cookie mode (the default), import cookies from browsers in this order, stopping at the first hit:

| Order | macOS behavior | Windows replacement |
| --- | --- | --- |
| 1 | Safari `~/Library/Cookies/Cookies.binarycookies` | **DROP** — no Safari on Windows. |
| 2 | Chrome/Chromium forks (Edge, Brave, Vivaldi, Arc, Opera, Chromium) `~/Library/Application Support/<vendor>/<profile>/Cookies` | Read SQLite at `%LOCALAPPDATA%\<Vendor>\<Product>\User Data\<Profile>\Network\Cookies` (Cookies file moved into `Network/` in Chrome 96+). Decrypt with DPAPI + AES-GCM. **See §3.2.** |
| 3 | Firefox `~/Library/Application Support/Firefox/Profiles/*/cookies.sqlite` | Read SQLite at `%APPDATA%\Mozilla\Firefox\Profiles\<profile>\cookies.sqlite`. Unencrypted. |

Manual mode: user pastes a full `Cookie: …` header into preferences; we parse it for a `sessionKey=` pair and use that directly.

The macOS list is owned by `ProviderBrowserCookieDefaults.defaultImportOrder` (`SweetCookieKit`). Windows port can hard-code: `[Edge, Chrome, Brave, Vivaldi, Arc, Opera, Chromium, Firefox]`. Probe by checking `User Data` folder existence per vendor.

### 3.2 Chrome v20 encryption caveat (Windows)

Chrome 127+ on Windows changed `Local State.os_crypt.encrypted_key` to be wrapped with **App-Bound Encryption** (key prefix `APPB` instead of `v10`/`v11`). Decrypting this from outside Chrome's process requires impersonating Chrome via COM or scraping `IElevator`. **This is fragile** — Chrome can break it any minor release.

Windows port policy:

1. Try decrypt; if prefix is `v10`/`v11`, DPAPI + AES-GCM (well-known approach) — use this.
2. If prefix is `APPB`, surface a one-time "Chrome v20 cookies can't be read automatically. Paste your `Cookie:` header here ↘" toast that auto-switches the user to Manual cookie source.
3. Document this in the in-app help.

### 3.3 Required cookie

| Cookie | Where | Value format | Validation |
| --- | --- | --- | --- |
| `sessionKey` | `claude.ai` domain | starts with `sk-ant-` | Trim whitespace before checking. |

That's the only cookie used. We do not need any CSRF token or `_session_id` etc. The session key is sufficient for all `claude.ai/api/*` calls.

### 3.4 Cookie header cache

After a successful Web API fetch, store the cookie header back to a cache (key: `cookie.claude`) along with a source label (e.g. `Chrome — Profile 1`) and timestamp. Next fetch reuses it before re-importing.

On `unauthorized`, `noSessionKeyFound`, or `invalidSessionKey` errors **only**, clear the cache. Other errors keep it (transient).

| macOS | Windows |
| --- | --- |
| Stored in Keychain `com.steipete.codexbar.cache`, account `cookie.claude` | DPAPI-encrypted JSON at `%LOCALAPPDATA%\CodexBar\cache\cookie-claude.bin` |

### 3.5 Web API endpoints

All calls use:

- Method: `GET`
- Header `Cookie: sessionKey=<value>` (the *only* cookie sent)
- Header `Accept: application/json`
- Timeout: 15 s

| Order | URL | Purpose | Required for success? |
| --- | --- | --- | --- |
| 1 | `https://claude.ai/api/organizations` | List of orgs → pick UUID | Yes (used in all subsequent URLs). |
| 2 | `https://claude.ai/api/organizations/{orgId}/usage` | Session/weekly/model-specific %, resets, extras | Yes. |
| 3 | `https://claude.ai/api/organizations/{orgId}/overage_spend_limit` | Spend/limit cost (Claude Extra) | Best-effort. Failure ≠ overall failure. |
| 4 | `https://claude.ai/api/account` | Email + plan billing fields | Best-effort. |

### 3.6 Organization selection

`/api/organizations` returns an array of `{uuid, name, capabilities[]}` entries. Selection rule (first match wins):

1. If `targetOrganizationID` is set on the token account → pick the one with that `uuid` (raise `organizationNotFound` if none).
2. Else: first org whose `capabilities` (lowercased) contains `chat`.
3. Else: first org whose capabilities are not exactly `["api"]` (skip API-only orgs).
4. Else: first org overall.

Org `name` is sanitized (trimmed, empty → null) and used as `accountOrganization` only when no other source populated it.

### 3.7 Usage response parsing (`/usage`)

```json
{
  "five_hour":     { "utilization": 32, "resets_at": "2026-05-12T15:00:00Z" },
  "seven_day":     { "utilization": 68, "resets_at": "2026-05-15T08:00:00Z" },
  "seven_day_sonnet": { "utilization": 41, "resets_at": "2026-05-15T08:00:00Z" },
  "seven_day_opus":   { "utilization": 12, "resets_at": "2026-05-15T08:00:00Z" }
}
```

Rules:

- `utilization` may be `Int` or `Double`. Cast to `f64`.
- If `five_hour.utilization` is missing → throw `invalidResponse` (caller falls through to CLI in `auto`).
- `seven_day_sonnet` is preferred for the opus/tertiary slot; fall back to `seven_day_opus`.
- ISO-8601 dates: try with fractional seconds first, then without.

### 3.8 Overage / spend-limit parsing

```json
{
  "is_enabled": true,
  "monthly_credit_limit": 5000,
  "currency": "USD",
  "used_credits": 1834
}
```

Same rules as OAuth `extra_usage`: skip when `is_enabled != true`, require all three of `used_credits`, `monthly_credit_limit`, `currency`. Divide both amounts by 100 (cents → dollars). Period = `"Monthly"`.

### 3.9 Account response parsing (`/account`)

```json
{
  "email_address": "jonas@skrylabs.com",
  "memberships": [
    {
      "organization": {
        "uuid": "abc-123",
        "name": "Skry Labs",
        "rate_limit_tier": "claude_pro",
        "billing_type": "stripe"
      }
    }
  ]
}
```

Plan inference for web:

```text
plan = match rate_limit_tier → Max | Pro | Team | Enterprise
if no match AND billing_type contains "stripe" AND rate_limit_tier contains "claude" → Pro
else null
```

Membership picking: if known orgId, pick the membership whose `organization.uuid` matches; else first membership.

### 3.10 Token-account routing (multi-account)

Users can store multiple Claude credentials in `~/.codexbar/config.json` under `tokenAccounts`. Two accepted shapes per token:

| Input shape | Detection | Routing |
| --- | --- | --- |
| OAuth token: `sk-ant-oat...`, or `Bearer sk-ant-oat...` | Lowercase, strip `Bearer ` prefix, check `sk-ant-oat` prefix, *must not* contain `=` or `cookie:` | Treat as OAuth. Cookie mode forced to `off`. CLI runtime: override source to `oauth`, inject env `CODEXBAR_CLAUDE_OAUTH_TOKEN=<token>`. |
| Session key: bare value | Doesn't match OAuth shape and doesn't contain `=` | Wrap as `sessionKey=<value>`. Cookie source forced to `manual`. |
| Full cookie header: contains `=` or `cookie:` | | Normalized + used as manual cookie header. Cookie source forced to `manual`. |

Each account may also carry a `sanitizedOrganizationID` to pin org selection.

### 3.11 Web error → user message catalog

| Internal | Message |
| --- | --- |
| `noSessionKeyFound` | "No Claude session key found in browser cookies." |
| `invalidSessionKey` | "Invalid Claude session key format." |
| `unauthorized` (401/403 on org or usage) | "Unauthorized. Your Claude session may have expired." |
| `serverError(code)` | "Claude API error: HTTP <code>" |
| `noOrganization` | "No Claude organization found for this account." |
| `organizationNotFound(id)` | "Claude organization '<id>' was not found for this session." |
| `invalidResponse` | "Invalid response from Claude API." (treat as fall-through trigger in `auto`) |
| Network error | "Network error: \<detail\>" |

---

## 4. CLI PTY path

### 4.1 Why PTY

Claude's `claude` binary is a TUI built for interactive terminals. It renders the `/usage` panel only when:

- stdout/stderr are a TTY (line discipline + signals routed),
- a window size is set (otherwise it refuses to render the panel),
- responses to first-run prompts (folder trust, telemetry consent) are handled.

We can't capture this with a plain `Command::stdout` pipe. We need a real pseudo-terminal.

| macOS | Windows |
| --- | --- |
| `openpty(3)` from `<util.h>`, with `winsize { rows: 50, cols: 160 }` | **ConPTY** via `CreatePseudoConsole` (Win10 1809+). Allocate input/output pipes, `CreatePseudoConsole(coord, hInput, hOutput, 0, &hPC)`, then `STARTUPINFOEX` with `PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE_REFERENCE`. Use `portable-pty` crate for cross-platform abstraction. |

Window size: `50 rows × 160 cols`. Make this configurable but match for parity.

### 4.2 Launch

```text
binary:  resolved claude path  (env CLAUDE_CLI_PATH override, else PATH walk)
args:    --allowed-tools ""    (empty string disables all tools; prevents accidental execution)
cwd:     <appdata>/CodexBar/ClaudeProbe   (stable scratch dir, auto-created)
stdin/stdout/stderr: connected to the PTY child end
```

### 4.3 Environment scrubbing

Strip these from the child env to prevent the CLI from using stale CodexBar-injected creds:

- `CODEXBAR_CLAUDE_OAUTH_TOKEN`
- `CODEXBAR_CLAUDE_OAUTH_SCOPES`
- **all** keys with prefix `ANTHROPIC_` (e.g. `ANTHROPIC_API_KEY`, `ANTHROPIC_BASE_URL`)

Set `PWD` to the working directory (some Claude versions rely on `$PWD` for the trust prompt).

### 4.4 First-run prompt auto-responder

The CLI prints various interactive prompts on first use per directory. We watch the output stream (in lowercased, whitespace-stripped form) for these substrings and auto-respond:

| Detected substring (normalized) | Send |
| --- | --- |
| `do you trust the files in this folder?` | `y\r` |
| `quick safety check:` | `\r` (Enter) |
| `yes, i trust this folder` | `\r` |
| `ready to code here?` | `\r` |
| `press enter to continue` | `\r` |

Additionally watch for the cursor-position-report (CPR) escape `ESC[6n` (`0x1B 0x5B 0x36 0x6E`) and reply with a fake answer `ESC[1;1R` so the TUI doesn't stall waiting for terminal feedback.

### 4.5 Command palette auto-responder

When sending `/usage`, the CLI may render a "Show plan" / "Show plan usage limits" command palette — auto-Enter through it.
When sending `/status`, auto-Enter through "Show Claude Code status".

Restrict each subcommand's auto-Enter list to its own actions; do not blanket-Enter every "Show …" item or you'll accidentally execute `/status` instead of `/usage`.

### 4.6 The probe sequence

```text
1. Ensure session is started (or reuse if keepCLISessionsAlive=true).
2. If session is fresh, wait 2 seconds for TUI init (drops early keystrokes otherwise).
3. Drain stale output.
4. Send "/usage" + "\r".
5. Watch output for stop substrings (any one matches):
     "Current week (all models)"
     "Current week (Opus)"
     "Current week (Sonnet only)"
     "Current week (Sonnet)"
     "Current session"
     "Failed to load usage data"
6. While waiting: send "\r" every 0.8s (the TUI redraws on keypress).
7. On stop-match or timeout: settle 2s capturing trailing output.
8. Optionally send "/status" (idle-timeout 3s, no periodic Enter) to capture identity.
9. Parse (see §4.7).
10. If keepCLISessionsAlive=false: send "/exit\r" and tear down PTY.
```

Timeouts: usage probe = 10s normally, retry once with 14–24s on first attempt looking like startup churn. `/status` = ≤12s.

If the first capture doesn't look usage-like (lowercase whitespace-stripped output lacks `currentsession`, `currentweek`, `loadingusage`, `failedtoloadusagedata`), retry once with longer timeout.

### 4.7 Parsing the `/usage` panel

**Step 1: ANSI strip.** Remove escape sequences (use a regex like `\x1b\[[0-9;?]*[a-zA-Z]`). Keep newlines.

**Step 2: Trim to latest panel.** Find the last occurrence of `Settings:` (case-insensitive). Verify the tail contains `Usage` AND (`used`/`left`/`remaining`/`available`) AND a `%`, OR `loading usage`. Slice from `Settings:` onward. This eliminates earlier TUI fragments that include status-bar `0%` context meters.

**Step 3: Find labels.** For each window, look for these label substrings (matched on lowercase, alphanumeric-only collapsed form so `Current week (all models)` → `currentweekallmodels`):

| Window | Primary label | Acceptable variants |
| --- | --- | --- |
| Session | `Current session` | — |
| Weekly | `Current week (all models)` | — |
| Opus/Sonnet | `Current week (Opus)` | `Current week (Sonnet only)`, `Current week (Sonnet)` |

If labels exist but extraction fails, use a positional fallback: collect all `<num>%` occurrences in the trimmed panel; assign by index (session=0, weekly=1, opus=2). Skip the fallback for windows whose label is absent — enterprise accounts often omit weekly entirely; we should report "unavailable" not a guess.

**Step 4: Extract percent.** For each label, look at the next 12 lines. Parse first line matching `([0-9]+(?:\.[0-9]+)?)\s*%`. Determine meaning by adjacent words:

| Adjacent word in line (lowercase) | Interpretation |
| --- | --- |
| `used`, `spent`, `consumed` | percent **used** → store `100 - n` as `percentLeft` |
| `left`, `remaining`, `available` | percent **left** → store `n` as `percentLeft` |
| neither | skip (avoid false matches from status-line context meters) |

Skip lines with `|` *and* a model token (`opus`, `sonnet`, `haiku`, `default`) — that's the status bar context meter, not usage.

**Step 5: Extract reset.** From the same 12-line window after the label, find first line matching `Resets[^\r\n]*`. Trim a trailing stray `)` (PTY artifact). Balance parens.

**Step 6: Identity.** Parse from both `/usage` and `/status` outputs (in that priority for email; `/status` preferred for plan):

| Field | Regex (case-insensitive) | Notes |
| --- | --- | --- |
| Email | `Account:\s+([^\s@]+@[^\s@]+)` then `Email:\s+(...)`, then loose `Account:\s+(\S+)`, finally any `<addr>@<host>.<tld>` | First non-empty match. |
| Org | `Org:\s*(.+)`, `Organization:\s*(.+)` | Trim. Suppress if it equals the email prefix (CLI panel often shows this). |
| Login method | `login\s+method:\s*(.+)`, else any `claude\s+<plan>` phrase | Filter out phrases containing `code v`, `code version`, `code` (avoid matching the version banner). |

### 4.8 CLI error pass-through

The CLI sometimes emits JSON-wrapped errors:

```text
Failed to load usage data: {"error":{"type":"rate_limit_error","message":"...","details":{"error_code":"..."}}}
```

Parser:

1. Extract the trailing `{...}` block via regex.
2. Parse `error.type`. If `rate_limit_error` → "Claude CLI usage endpoint is rate limited right now. Please try again later."
3. Else combine `error.message` + `error.details.error_code` into a single hint; if the code contains `token`, append "Run `claude login` to refresh."

Other plain-text heuristics:

| Output contains (lowercase) | Surface |
| --- | --- |
| `token_expired`, `token has expired` | "Claude CLI token expired. Run `claude login` to refresh." |
| `authentication_error` | "Claude CLI authentication error. Run `claude login`." |
| `rate_limit_error`, `rate limited`, `ratelimited` | "Claude CLI usage endpoint is rate limited right now." |
| `failed to load usage data`, `failedtoloadusagedata` | "Claude CLI could not load usage data. Open the CLI and retry `/usage`." |
| `do you trust the files in this folder?` without `current session` | "Claude CLI is waiting for a folder trust prompt." (auto-accept failed) |

### 4.9 Keep-alive

By default the PTY session is torn down after each probe. Enabling `Keep CLI sessions alive` (debug toggle) skips the `/exit` write and keeps the child running between probes. The PTY actor reuses the existing process when:

- it's still running AND
- the binary path matches the last-launched binary.

Otherwise it does a clean teardown (write `/exit\r`, terminate, kill process group after 1 s grace) and starts fresh.

### 4.10 Reset-time parsing

Reset strings from the CLI vary: `Resets 8pm`, `Resets at 3:00pm (America/New_York)`, `Resets May 14 at 11am`, etc. Implement a multi-format parser:

```text
normalize:
  - drop leading "resets:" / "resets "
  - extract optional "(<TZ>)" trailing parenthesis as timezone hint
  - replace " at " with " ", split day-month with no space, etc.
formats to try (POSIX locale):
  with minutes: h:mma, h:mm a, HH:mm, H:mm
  hour-only:    ha, h a
  date+time:    MMM d, h:mma   (and variants without comma, with space)
```

Anchor to "today"; if the result is before `now`, add 1 day. Use the TZ from the parenthesis, else local TZ.

---

## 5. The watchdog (`CodexBarClaudeWatchdog`)

### 5.1 What it does

A tiny separate process that wraps the `claude` invocation. Its sole job is to **kill the entire claude process tree** if the parent (the menu-bar app) dies unexpectedly. Prevents zombie `claude` processes from accumulating on crashes / forced quits.

### 5.2 macOS implementation

```text
main:
  parse argv after "--" → child binary + args
  posix_spawnp child, get pid
  setpgid(pid, pid)              # own process group
  install signal handlers: SIGTERM, SIGINT, SIGHUP → just set a flag
  loop every 200ms:
    waitpid(WNOHANG)             # child exited? exit with same code
    if shouldTerminate flag → kill process tree → exit
    if getppid() == 1 → parent died, kill tree → exit
```

Process-tree kill: `kill(-pgid, SIGTERM)`; 500 ms grace; `kill(-pgid, SIGKILL)`.

### 5.3 Why a separate process

If we did this in-process, a parent crash would skip the cleanup. The watchdog runs as a *separate* process whose only job is to die last and take its child with it.

### 5.4 Windows replacement

Equivalent design, native Win32:

```text
main:
  parse argv after "--"
  CreateJobObject + SetInformationJobObject(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
  CreateProcess(child, suspended)
  AssignProcessToJobObject(job, child)
  ResumeThread(child)
  WaitForSingleObject(child) OR poll GetExitCodeProcess(parent_handle)
  if parent dies → CloseHandle(job) → kernel kills all children
```

`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` is exactly what we want: when the watchdog process exits (for any reason), the job dies and so does claude. Ship as `codexbar-claude-watchdog.exe`. Locate it via `<app dir>\helpers\codexbar-claude-watchdog.exe` (mirror macOS's `Frameworks/Helpers/`).

Disable via env `CODEXBAR_DISABLE_CLAUDE_WATCHDOG=1` for debugging.

### 5.5 IPC

There is no custom IPC — the watchdog is opaque, just a process-tree babysitter. The PTY is wired directly from the menu app to claude through the watchdog's inherited pty handles.

---

## 6. The web probe (`CodexBarClaudeWebProbe`)

### 6.1 What it does

A CLI utility (`codexbar-claude-web-probe`) that hits a list of `claude.ai/api/*` endpoints using the user's browser cookies, prints status + top-level JSON keys + email/plan hints per endpoint. Used to:

- diagnose why the Web API path is failing,
- discover new endpoints / field renames before they break the parser,
- support engineers debug user issues without seeing tokens.

### 6.2 Default endpoint list

```text
/api/organizations
/api/organizations/{orgId}/usage
/api/organizations/{orgId}/overage_spend_limit
/api/organizations/{orgId}/members
/api/organizations/{orgId}/me
/api/organizations/{orgId}/billing
/api/me
/api/user
/api/session
/api/account
/settings/billing       (HTML)
/settings/account       (HTML)
/settings/usage         (HTML)
```

Accept custom endpoint list as args. Honor `CLAUDE_WEB_PROBE_PREVIEW=1` to include a 500-char body preview.

### 6.3 Output per endpoint

```text
==> https://claude.ai/api/...
status: 200
content-type: application/json
keys: foo, bar, baz
emails: jonas@…
plan-hints: pro, max
fields: plan=… subscription=… tier=…
preview: { … }
```

Field extraction: scan JSON for keys matching `(?i)(plan|tier|subscription|seat|billing|product)` and dump matching scalar values up to 40 entries. Useful for noticing new fields.

### 6.4 Windows port

Ship as `codexbar-claude-web-probe.exe`. Read cookies via the same Windows cookie store the main app uses (§3.1-3.2). Truncate response bodies to 200 KB to keep terminal output manageable.

---

## 7. Cost-usage scanner (local JSONL)

### 7.1 Where the logs live

| macOS | Windows |
| --- | --- |
| `$CLAUDE_CONFIG_DIR/projects/**/*.jsonl` (comma-separated list) | Same env var honored. Each entry: if it ends in `projects` use it directly, else append `\projects`. |
| `~/.config/claude/projects/**/*.jsonl` | `%USERPROFILE%\.config\claude\projects\**\*.jsonl` |
| `~/.claude/projects/**/*.jsonl` | `%USERPROFILE%\.claude\projects\**\*.jsonl` |
| `~/.pi/agent/sessions/**/*.jsonl` | `%USERPROFILE%\.pi\agent\sessions\**\*.jsonl` |

Probe all roots; missing roots are silently skipped.

### 7.2 What we parse

For each `.jsonl` file (line-delimited JSON), keep only lines matching all of:

- byte-substring contains `"type":"assistant"` (cheap pre-filter)
- byte-substring contains `"usage"`
- parsed JSON has `type == "assistant"`
- passes the Vertex-AI filter (`all` / `vertexAIOnly` / `excludeVertexAI`)
- has `timestamp` (ISO-8601-ish), `message.model`, `message.usage`

Extract per-line:

```text
input_tokens                        → input
cache_creation_input_tokens         → cache_create
cache_read_input_tokens             → cache_read
output_tokens                       → output
```

Cost calculation uses `CostUsagePricing.claudeCostUSD(model, input, cacheRead, cacheCreate, output, modelsDevCatalog)` — model-aware pricing with cache discounts.

### 7.3 Deduplication

Streaming usage chunks repeat with growing token counts and the same `(message.id, requestId)` pair. Rule: **keep overwriting; the last cumulative chunk wins.** Implemented as a `HashMap<(message_id, request_id), Row>` per file. Lines missing either id go into an unkeyed bucket (no dedup; older logs).

Cross-file canonical dedup adds `sessionId` to the key and picks a winner with this tie-break:

1. Prefer non-sidechain over sidechain.
2. Prefer `subagent`-path role (file under `/subagents/`) over `parent`.
3. Tie: smallest file path lexicographically.

### 7.4 Vertex AI detection

A row is Vertex if any of:

- `message.id` contains `_vrtx_`
- `requestId` contains `_vrtx_`
- model name contains `@` and starts with `claude-` (Vertex format: `claude-opus-4-5@20251101` vs Anthropic native `claude-opus-4-5-20251101`)
- any nested dict has key containing `vertex` or `gcp`
- any nested string field at a `provider|platform|backend|api_type|source|vendor|client` key contains `vertex`

Filter modes: `all` (default) | `vertexAIOnly` | `excludeVertexAI` — exposed in advanced settings.

### 7.5 Pi sessions

Pi agent (`~/.pi/agent/sessions`) bundles multi-provider sessions. Iterate assistant turns where the provider tag is `anthropic`; attribute their token counts to Claude. Bucket per assistant-turn timestamp (a single session can span multiple days / models).

### 7.6 Caching

| Cache | Path (macOS) | Windows path |
| --- | --- | --- |
| Native + merged Claude | `~/Library/Caches/CodexBar/cost-usage/claude-v2.json` | `%LOCALAPPDATA%\CodexBar\cost-usage\claude-v2.json` |
| Pi sessions | `~/Library/Caches/CodexBar/cost-usage/pi-sessions-v1.json` | `%LOCALAPPDATA%\CodexBar\cost-usage\pi-sessions-v1.json` |

Cache invalidation: per-file `(mtime_ms, size)` tuple. Unchanged → skip parse, reuse rows. Larger file → incremental parse from previous `parsedBytes` offset, merge.

Refresh throttle: don't rescan unless `now - lastScanUnixMs > refreshMinIntervalSeconds`. Force-rescan flag for debug.

When directory tree is fully missing, purge file entries whose path starts with that root.

---

## 8. Keychain prompt policies (macOS) → Windows DPAPI behavior

### 8.1 macOS policies

Setting: **Preferences → Providers → Claude → Keychain prompt policy** (visible only when the OAuth read strategy is `securityFramework`).

| Mode | Behavior |
| --- | --- |
| `never` | Never attempts a Claude CLI Keychain read that could prompt. Effectively disables OAuth path on macOS unless creds are already cached. Also blocks delegated refresh. |
| `onlyOnUserAction` *(default)* | Background reads use the non-prompting `security` CLI reader. Interactive prompts (via Security.framework) only fire on user-initiated actions: menu open, refresh button, settings interaction. Background reads on a 6-hour cooldown after a denial. |
| `always` | Allows interactive prompts in both user and background flows. Loud but always-fresh. |

Additional sub-toggle: **"Avoid Keychain prompts"** (a.k.a. `claudeOAuthPromptFreeCredentialsEnabled`) — when ON, prefer the `/usr/bin/security` CLI reader over the Security.framework. The CLI reader never produces a UI prompt; it just fails with non-zero exit if not permitted.

### 8.2 Cooldown gate

On a deny error (`errSecUserCanceled`, `errSecAuthFailed`, `errSecInteractionNotAllowed`, `errSecNoAccessForItem`), record a cooldown of **6 hours** in user prefs (`claudeOAuthKeychainDeniedUntil`). During cooldown, no background prompts are attempted. User action (menu open, manual refresh) **clears** the cooldown immediately.

### 8.3 Windows mapping

Windows DPAPI does not produce prompts — it succeeds or fails silently, scoped to the user account. So the "prompt policy" concept doesn't apply.

**Port policy:**

| macOS construct | Windows |
| --- | --- |
| Keychain prompt policy (3-mode) | **Hide the setting on Windows.** Treat it as `always`. |
| `claudeOAuthPromptFreeCredentialsEnabled` toggle | Hide. Always-true equivalent. |
| Cooldown gate | Still record DPAPI/file-read denial cooldowns at 6 h, but only the "this read kept failing" kind (e.g. `.credentials.json` read returned 0 bytes / permission error). |
| `Disable Keychain access` advanced toggle | Map to `Disable DPAPI cache`: when ON, never write the DPAPI-encrypted cache file. Reads still allowed (it's just a perf opt). |

The Windows port can drop `ClaudeOAuthKeychainPromptMode`, `ClaudeOAuthKeychainAccessGate`, `ClaudeOAuthKeychainReadStrategy`, and the entire SecurityCLIReader subsystem. Replace with a single function `windows_read_oauth_creds() -> Option<Credentials>` reading the file + DPAPI cache.

---

## 9. Token expiry, refresh, error states

### 9.1 Per-path failure handling

| Path | Failure modes | Fall-through? | Visual signal |
| --- | --- | --- | --- |
| OAuth | Network, 401 unauthorized, 403 (scope), expired token (refresh failure) | In `auto/app` only — silently. Explicit mode → surface. | Dim icon if all paths fail. |
| Web | No session key, unauthorized (cleared cache + retry not done — user must re-import), org not found | `auto/cli` falls through to CLI. `auto/app`: web is terminal (last-ditch) so its error becomes the surface error. | "Re-import cookies" menu item. |
| CLI | Binary missing, timed out, parse failed, rate-limited, auth error | `auto/app` falls through *only if web is available*. Otherwise the CLI error is surfaced (more actionable than "web unavailable"). | "Open Terminal: claude" menu item when error mentions OAuth. |

### 9.2 Dim icon

The menu-bar icon dims (50% alpha) when the latest snapshot is `null` (all paths failed). It stays in error state until the next successful fetch. Use `provider_status` in the shared state.

### 9.3 Surfacing OAuth errors

When the user has visible attempts on the OAuth strategy with a non-empty `errorDescription` (or any error containing the literal `oauth`), the menu surfaces an **"Open Terminal" → `claude`** action. The user runs `claude login` interactively, comes back, hits Refresh, and we re-detect cached credentials.

### 9.4 Error pass-through priority

For the **surface error** (one error string shown in the menu when no snapshot is available), prefer in this order:

1. Explicit picker mode → that source's error verbatim.
2. `auto` + last attempted source had a parse-able/actionable error → that.
3. Otherwise → "No source available" with a hint to enable one.

---

## 10. Settings keys (Claude-specific)

All persisted in `~/.codexbar/config.json` (macOS) / `%APPDATA%\CodexBar\config.json` (Windows) under the Claude provider config entry, except where noted:

| Key | Type | Default | Where stored | Meaning |
| --- | --- | --- | --- | --- |
| `source` | enum `auto | oauth | web | cli` | `auto` | provider config | Active usage source. `api` alias = `auto` (legacy). |
| `cookieSource` | enum `auto | manual | off` | `auto` | provider config | Where to load cookies from. Off disables Web entirely. |
| `cookieHeader` | string | `""` | provider config | Manual cookie header pasted by user. Stored sanitized (single line, leading `Cookie:` stripped). |
| `organizationID` | string? | null | per token account | Pin org UUID for selection. |
| `webExtrasEnabled` | bool | `false` | provider config | When true *and* primary source is CLI, also fetch web for cost/extras. Auto-disabled if source changes to anything but CLI. |
| `claudeOAuthKeychainPromptMode` | enum `never | onlyOnUserAction | always` | `onlyOnUserAction` | user defaults | macOS only. **Hide on Windows.** |
| `claudeOAuthKeychainReadStrategy` | enum `securityFramework | securityCLIExperimental` | `securityCLIExperimental` | user defaults | macOS only. **Hide on Windows.** |
| `claudeOAuthPromptFreeCredentialsEnabled` | bool | derived from read strategy | provider config | macOS only. **Hide on Windows.** |
| `claudePeakHoursEnabled` | bool | `false` | provider config | Show NY-business-hours peak indicator. See §11.3. |
| `debugKeepCLISessionsAlive` | bool | `false` | top-level debug | Skip teardown between CLI probes. |
| `debugDisableKeychainAccess` | bool | `false` | top-level debug | Global keychain (mac) / DPAPI-cache (win) opt-out. |
| `CODEXBAR_DEBUG_CLAUDE_OAUTH_FLOW` | env | unset | env | Enables verbose flow logs. |
| `CODEXBAR_CLAUDE_OAUTH_TOKEN` | env | unset | env | Inject an OAuth token (e.g. for CI / multi-account). |
| `CODEXBAR_CLAUDE_OAUTH_SCOPES` | env | unset | env | Space-separated scopes for the env token. |
| `CODEXBAR_CLAUDE_OAUTH_CLIENT_ID` | env | unset | env | Override OAuth client id (default `9d1c250a-e61b-44d9-88ed-5944d1962f5e`). |
| `CODEXBAR_DISABLE_CLAUDE_WATCHDOG` | env | unset | env | Run `claude` directly without watchdog wrapper. |
| `CLAUDE_CLI_PATH` | env | unset | env | Pin a specific `claude` binary path. Empty value removes any cached override. |
| `CLAUDE_CONFIG_DIR` | env | unset | env | Override config dir search; comma-separated list. Each entry's `/projects` subdir is scanned. |
| `DEBUG_CLAUDE_DUMP` | env | unset | env | When parsing fails, write a redacted dump to an in-memory ring buffer for support. |

---

## 11. Models surfaced

### 11.1 Rate windows

The menu card shows up to three rate windows:

| Slot | Window | Source priority |
| --- | --- | --- |
| Primary | Session (5 hours, 300 min) | `five_hour` → `seven_day` → … |
| Secondary | Weekly all-models (7 days, 10 080 min) | `seven_day` |
| Tertiary | Weekly model-specific | `seven_day_sonnet` → `seven_day_opus` |

Plus an arbitrary number of **named** extra windows (Designs, Daily Routines), each rendered as its own progress row.

Each window carries: `usedPercent` (0–100), `windowMinutes`, `resetsAt: Option<DateTime>`, `resetDescription: Option<String>`.

### 11.2 Account & plan

| Field | Source priority |
| --- | --- |
| `accountEmail` | CLI `/usage` + `/status` → Web `/account.email_address` → never from OAuth (not in payload) |
| `accountOrganization` | CLI `/status` `Org:` line → Web `/organizations[].name` → never from OAuth |
| `loginMethod` | OAuth `subscriptionType`/`rateLimitTier` (preferred for OAuth path) → Web membership tier + billing → CLI `login method:` line, sanitized |

Display as: `<email>` line, then `<plan>` line under it (e.g. `Claude Max`). Hide org if it equals the email prefix (common CLI artifact).

### 11.3 Pace, peak hours, cost

| Surface | Provided by | When shown |
| --- | --- | --- |
| Pace text ("burning $.30/hr") | `UsagePaceText` from `ProviderCostSnapshot` + session reset | When cost is non-null and progress > 0. |
| Peak hours indicator | `ClaudePeakHours.status(at: now)` — peak = weekday (Mon–Fri) `08:00–14:00` America/New_York | When `claudePeakHoursEnabled = true`. Renders "Peak · ends in 1h 20m" or "Off-peak · peak in 2d 4h". |
| Extra usage line | `providerCost` from `extra_usage` (OAuth) or `overage_spend_limit` (Web) | When `showOptionalCreditsAndExtraUsage` is on and currency ≠ `Quota`. |

### 11.4 Web-extra enrichment rules

When the primary source is **OAuth or CLI** and `webExtrasEnabled` is true, try a best-effort Web fetch and merge in:

- `providerCost` — only if the primary didn't set one.
- `extraRateWindows` — only if the primary didn't return any.

**Never** overwrite identity fields (email, org, plan) from web extras — keep them provider-scoped to the primary source.

---

## 12. Mac → Windows mapping (per path)

| Concern | macOS mechanism | Windows mechanism |
| --- | --- | --- |
| **OAuth: credential file** | `~/.claude/.credentials.json` | `%USERPROFILE%\.claude\.credentials.json` (Claude CLI writes here on Windows too) |
| **OAuth: secure cache** | Keychain `com.steipete.codexbar.cache` / `Claude Code-credentials` | DPAPI-encrypted file: `%LOCALAPPDATA%\CodexBar\cache\claude-oauth.bin` (CryptProtectData) |
| **OAuth: file watch** | mtime + size fingerprint via `FileManager.attributesOfItem` | `std::fs::metadata` + ReadDirectoryChangesW for inval. Same fingerprint shape: `{ modified_at_ms, size }`. |
| **OAuth: delegated refresh** | Spawn `claude /status` in PTY, snapshot Keychain fingerprint before/after | Spawn `claude /status` in ConPTY, snapshot `.credentials.json` mtime+size+sha256 before/after. Same cooldown (5 min on success, 20 s on fail). |
| **Web: Safari cookies** | `~/Library/Cookies/Cookies.binarycookies` | **DROP** — no Safari on Windows. |
| **Web: Chrome cookies** | macOS Keychain-encrypted SQLite at `~/Library/Application Support/Google/Chrome/<profile>/Cookies` | `%LOCALAPPDATA%\Google\Chrome\User Data\<profile>\Network\Cookies` SQLite. DPAPI + AES-GCM decrypt of `os_crypt.encrypted_key` from `Local State`. Cookie value bytes prefixed `v10`/`v11`. See §3.2 for v20/`APPB` caveat. |
| **Web: Edge / Brave / Vivaldi / Arc / Opera / Chromium** | same approach via SweetCookieKit | Same Chromium decrypt; vendor-specific `User Data` path: `%LOCALAPPDATA%\Microsoft\Edge`, `%LOCALAPPDATA%\BraveSoftware\Brave-Browser`, `%LOCALAPPDATA%\Vivaldi`, `%LOCALAPPDATA%\Packages\TheBrowserCompany.Arc_…\LocalCache\…`, `%APPDATA%\Opera Software\Opera Stable`, `%LOCALAPPDATA%\Chromium`. |
| **Web: Firefox cookies** | `~/Library/Application Support/Firefox/Profiles/*/cookies.sqlite` | `%APPDATA%\Mozilla\Firefox\Profiles\*\cookies.sqlite`. Unencrypted SQLite. Query: `SELECT name, value FROM moz_cookies WHERE host LIKE '%claude.ai'`. |
| **Web: cookie header cache** | Keychain entry | DPAPI-encrypted JSON `%LOCALAPPDATA%\CodexBar\cache\cookie-claude.bin` |
| **CLI PTY** | `openpty(3)` + `Process` | `CreatePseudoConsole` (ConPTY) via `portable-pty` or `conpty` crate. Same `50×160` window size. |
| **CLI shutdown** | `kill(-pgid, SIGTERM)` then `SIGKILL` | `TerminateJobObject(job, code)` — kills entire tree atomically. |
| **CLI watchdog** | Separate Mach-O `CodexBarClaudeWatchdog` | Separate exe `codexbar-claude-watchdog.exe` using `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. |
| **CLI environment scrub** | `ProcessInfo.environment` filter | `std::process::Command::env_remove(...)`. Same key list. |
| **CLI binary resolution** | `BinaryLocator` walks PATH, npm globals, brew, asdf | Walk: env `CLAUDE_CLI_PATH` → `where claude` → `%APPDATA%\npm\claude.cmd` → `%APPDATA%\npm\claude.ps1` → `%LOCALAPPDATA%\Programs\claude\claude.exe` (if Anthropic ships one) → PATH walk. |
| **OAuth user agent version detection** | `claude --version` parsed | Same. Cache the result in-memory + on disk. |
| **Keychain prompt policy** | Three-state (`never`/`onlyOnUserAction`/`always`) + cooldown | Hidden on Windows. Treat as `always`. |
| **Cost cache root** | `~/Library/Caches/CodexBar/cost-usage/` | `%LOCALAPPDATA%\CodexBar\cost-usage\` |
| **Config root** | `~/.codexbar/config.json` | `%APPDATA%\CodexBar\config.json` (and honor `$XDG_CONFIG_HOME`-style env if user sets it). |
| **OAuth client id** | Constant + env override | Constant + env override (same value: `9d1c250a-e61b-44d9-88ed-5944d1962f5e`). |
| **Login flow** | `claude /login` in PTY, scrape first `https://…` URL, hand off to default browser. Wait for `Successfully logged in` / `Login successful` / `Logged in successfully` then settle 350 ms. | Same logic, ConPTY. Open URL via `ShellExecuteW`. |

---

## 13. UX polish notes (Phantom/Duolingo bar)

The macOS app's menu card is functional but plain. For the Windows port, ship these specifically:

1. **First-time empty state** — when no Claude source is configured, show a single big card with three "Connect" CTAs (OAuth / Cookies / CLI), each with an icon, 1-line plain-English summary, and a "Why this method?" reveal.
2. **Live progress bars** — animate percent changes (0.5 s eased) rather than jumping. Use a single mono-color bar tinted by Claude's terracotta `#CC7C5E`.
3. **Reset countdowns** — instead of `Resets at 8pm`, show a live countdown that updates every minute: `Resets in 2h 14m`. Tooltip shows the absolute time.
4. **Plan badge** — small pill next to the email showing the plan (`Max`, `Pro`, etc.) with a subtle gradient matching plan tier (Max = warm gradient, Pro = neutral, Team = blue, Enterprise = purple).
5. **Source indicator** — tiny label like `via OAuth` / `via cookies (Chrome)` / `via CLI` below the rates row. Click to open Settings → Source picker.
6. **Cookie staleness toast** — when Web returns 401, show a one-tap toast: "Cookies expired. Re-import from <browser> → tap → Manual paste". Don't make the user dig through preferences.
7. **OAuth one-click repair** — when delegated refresh fails, surface a single button: "Run `claude login` in Terminal". Pre-fill the command in a new Terminal window via `cmd /k claude` on Windows.
8. **Peak hours easter-egg** — when the peak indicator is on and we're inside peak, color the chevron orange and add a "🔥 peak hours" subtitle in the card header. Use sparingly.
9. **CLI debug dump** — keep the in-memory ring buffer of the last 5 parse-failure dumps (gated by `DEBUG_CLAUDE_DUMP=1` env). Surface in a hidden Cmd+Shift+D dialog for support.

---

## 14. Acceptance checklist (parity per path)

### 14.1 OAuth API

- [ ] Reads `%USERPROFILE%\.claude\.credentials.json` and parses `claudeAiOauth.{accessToken,refreshToken,expiresAt,scopes,rateLimitTier,subscriptionType}`.
- [ ] Reads `CODEXBAR_CLAUDE_OAUTH_TOKEN` env first.
- [ ] Reads DPAPI cache file second.
- [ ] Reads `.credentials.json` third.
- [ ] Skips usage call when `scopes` lacks `user:profile`, surfaces actionable error.
- [ ] Sends `Authorization: Bearer …`, `anthropic-beta: oauth-2025-04-20`, `User-Agent: claude-code/<version>`.
- [ ] Calls `GET https://api.anthropic.com/api/oauth/usage`.
- [ ] Maps `five_hour`, `seven_day`, `seven_day_sonnet|opus|oauth_apps`, `extra_usage`, `seven_day_design|routines` (with key aliases).
- [ ] Converts `extra_usage` amounts ÷100 (cents → dollars).
- [ ] Infers plan via `subscriptionType` then `rateLimitTier`.
- [ ] On 403 with body containing `user:profile`, surfaces scope error.
- [ ] Auto-refresh: direct refresh against `https://platform.claude.com/v1/oauth/token` when owner is `codexbar`.
- [ ] Delegated refresh: spawns `claude /status` PTY, detects file change, 5-min/20-s cooldowns.
- [ ] In-memory cache TTL 30 min, invalidates on file fingerprint change.

### 14.2 Web API (cookies)

- [ ] Reads `sessionKey` from Edge/Chrome/Brave/Vivaldi/Arc/Opera/Chromium (DPAPI + AES-GCM) and Firefox (unencrypted SQLite).
- [ ] Validates value prefix `sk-ant-`.
- [ ] Skips Safari (no Windows Safari).
- [ ] Handles Chrome `APPB` v20 case by surfacing manual-paste toast.
- [ ] Cache cookie header in DPAPI file, clear on 401/no-session/invalid-key only.
- [ ] Calls `/api/organizations`, picks org (target id → chat capability → non-API-only → first).
- [ ] Calls `/api/organizations/{orgId}/usage` with `Cookie: sessionKey=…` header only.
- [ ] Parses `five_hour`, `seven_day`, `seven_day_sonnet|opus`, `seven_day_design|routines` aliases.
- [ ] Best-effort `/overage_spend_limit` → cost ÷100.
- [ ] Best-effort `/account` → email + plan inference.
- [ ] OAuth-token-shaped tokens in `tokenAccounts` route to OAuth path, not cookies.

### 14.3 CLI PTY

- [ ] Spawns via ConPTY at `50×160`.
- [ ] Wraps with `codexbar-claude-watchdog.exe` + Job Object kill-on-close.
- [ ] Scrubs env: `CODEXBAR_CLAUDE_OAUTH_TOKEN`, `CODEXBAR_CLAUDE_OAUTH_SCOPES`, all `ANTHROPIC_*`.
- [ ] Sets `PWD` and `CWD` to scratch dir `%LOCALAPPDATA%\CodexBar\ClaudeProbe`.
- [ ] Args: `--allowed-tools ""`.
- [ ] Auto-responds to first-run prompts (folder trust, safety check, ready, continue).
- [ ] Auto-replies to CPR (`ESC[6n`) with `ESC[1;1R`.
- [ ] Auto-Enters through `/usage` command palette ("Show plan", "Show plan usage limits") and `/status` palette ("Show Claude Code status") — scoped per subcommand.
- [ ] Sends `/usage`, retries with longer timeout if first capture isn't usage-like.
- [ ] Optionally captures `/status` for identity.
- [ ] Parses ANSI-stripped panel: trim to last "Settings:" header, label-aware percent extraction (used/left semantics), 12-line windows, positional fallback only where label exists.
- [ ] Parses reset times in many formats with optional TZ in parens.
- [ ] Surfaces `rate_limit_error`, `token_expired`, `authentication_error`, `failed to load usage data`.
- [ ] Tears down on completion unless `keepCLISessionsAlive` is set.
- [ ] On termination, kills entire job (no zombies).

### 14.4 Watchdog

- [ ] `codexbar-claude-watchdog.exe` exists in helpers folder.
- [ ] Uses `CreateJobObject` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- [ ] On parent death, child dies.
- [ ] On SIGTERM-equivalent (`SetConsoleCtrlHandler`), kills child tree and exits with `128 + signal`.
- [ ] Honors `CODEXBAR_DISABLE_CLAUDE_WATCHDOG=1` (run claude directly).

### 14.5 Web probe

- [ ] `codexbar-claude-web-probe.exe` ships.
- [ ] Default endpoint list matches §6.2.
- [ ] Argv override accepted.
- [ ] `CLAUDE_WEB_PROBE_PREVIEW=1` includes 500-char preview.
- [ ] Extracts emails, plan hints, notable plan/tier/subscription fields.
- [ ] Resolves `{orgId}` placeholder.

### 14.6 Cost scanner

- [ ] Roots: `CLAUDE_CONFIG_DIR` (comma-separated), `%USERPROFILE%\.config\claude\projects`, `%USERPROFILE%\.claude\projects`, `%USERPROFILE%\.pi\agent\sessions`.
- [ ] Walks `**/*.jsonl`, skips hidden + package descendants.
- [ ] Pre-filters lines by byte-substring `"type":"assistant"` AND `"usage"`.
- [ ] Token fields: `input_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `output_tokens`.
- [ ] Dedup streaming chunks by `(message.id, requestId)` per file; cross-file canonical by `(sessionId, message.id, requestId)` with non-sidechain > subagent > path lex tie-break.
- [ ] Vertex AI filter (all / vertexAIOnly / excludeVertexAI), detection rules per §7.4.
- [ ] Pi sessions attributed to Claude by `provider == "anthropic"`.
- [ ] Cache files at `%LOCALAPPDATA%\CodexBar\cost-usage\{claude-v2,pi-sessions-v1}.json`.
- [ ] Incremental parse from `parsedBytes` when file grows; full reparse otherwise.
- [ ] Throttle by `refreshMinIntervalSeconds`.
- [ ] Pricing via models.dev catalog cached separately.

### 14.7 Settings / behavior

- [ ] Source picker: `Auto | OAuth | Web | CLI` with subtitle "Auto falls back to the next source if the preferred one fails."
- [ ] Cookie source: `Auto | Manual` (no Off allowed by default; debug can re-enable).
- [ ] Manual cookie field (paste `Cookie:` header).
- [ ] `webExtrasEnabled` toggle visible only when source is CLI; auto-clears when source changes.
- [ ] Peak hours toggle.
- [ ] Hide macOS-only keychain/security/promptpolicy settings.
- [ ] Login button runs `claude /login`, opens detected URL in default browser, success → enable Claude + set source to OAuth.
- [ ] Successful OAuth login auto-enables the provider and switches Source to OAuth.

### 14.8 Single planner consolidation

- [ ] Only **one** function decides the auto-pipeline order. No second `.auto` branch in any fetcher.

---

## 15. Quick reference: header / endpoint / cookie summary

```text
OAuth usage:
  GET https://api.anthropic.com/api/oauth/usage
    Authorization: Bearer <accessToken>
    anthropic-beta: oauth-2025-04-20
    Accept: application/json
    Content-Type: application/json
    User-Agent: claude-code/<version>

OAuth refresh (codexbar-owned tokens):
  POST https://platform.claude.com/v1/oauth/token
    body: { grant_type: "refresh_token", refresh_token: ..., client_id: ... }

Web (cookie):
  GET https://claude.ai/api/organizations
  GET https://claude.ai/api/organizations/{orgId}/usage
  GET https://claude.ai/api/organizations/{orgId}/overage_spend_limit
  GET https://claude.ai/api/account
    Cookie: sessionKey=sk-ant-...
    Accept: application/json

Cookie name:       sessionKey   (value starts "sk-ant-")
Cookie domain:     claude.ai
OAuth client id:   9d1c250a-e61b-44d9-88ed-5944d1962f5e
Required scope:    user:profile
```

---

## 16. Source file index (mac → study targets; do not port directly)

| File | Read for |
| --- | --- |
| `Sources/CodexBarCore/Providers/Claude/ClaudeProviderDescriptor.swift` | Pipeline order + strategy registration. |
| `Sources/CodexBarCore/Providers/Claude/ClaudeSourcePlanner.swift` | The planner algorithm (§1). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeUsageFetcher.swift` | OAuth/web/CLI orchestration, web-extras enrichment, all error mapping. |
| `Sources/CodexBarCore/Providers/Claude/ClaudeUsageDataSource.swift` | The `auto|oauth|web|cli` enum. |
| `Sources/CodexBarCore/Providers/Claude/ClaudeStatusProbe.swift` | CLI `/usage` + `/status` parsing (§4.7). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeCLISession.swift` | PTY session lifecycle + prompt auto-responder (§4.3–4.5). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeOAuth/ClaudeOAuthCredentials.swift` | Credential parse + JSON shape (§2.2). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeOAuth/ClaudeOAuthUsageFetcher.swift` | Usage endpoint + dynamic key decode + extras (§2.4–2.8). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeOAuth/ClaudeOAuthDelegatedRefreshCoordinator.swift` | Delegated refresh state machine (§2.10). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeOAuth/ClaudeOAuthKeychainPromptMode.swift` | Prompt policy enum (mac-only, drop on win). |
| `Sources/CodexBarCore/Providers/Claude/ClaudePlan.swift` | Plan inference (§2.9, §3.9). |
| `Sources/CodexBarCore/Providers/Claude/ClaudePeakHours.swift` | Peak-hours indicator. |
| `Sources/CodexBarCore/Providers/Claude/ClaudeCredentialRouting.swift` | Token-account routing (§3.10). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeWeb/ClaudeWebAPIFetcher.swift` | All web endpoints + cookie cache (§3). |
| `Sources/CodexBarCore/Providers/Claude/ClaudeWeb/ClaudeWebExtraRateWindowParser.swift` | Extras parsing + key aliases. |
| `Sources/CodexBarCore/Vendored/CostUsage/CostUsageScanner+Claude.swift` | Local log scanning (§7). |
| `Sources/CodexBarClaudeWatchdog/main.swift` | Watchdog process design (§5). |
| `Sources/CodexBarClaudeWebProbe/ClaudeWebProbeEntry.swift` | Web probe CLI (§6). |
| `Sources/CodexBar/ClaudeLoginRunner.swift` | Login flow `claude /login` + URL extraction. |
| `Sources/CodexBar/Providers/Claude/ClaudeSettingsStore.swift` | Settings persistence + token-account snapshot. |
| `Sources/CodexBar/Providers/Claude/ClaudeProviderImplementation.swift` | UI integration: settings pane, login menu action. |
| `Sources/CodexBar/Providers/Claude/ClaudeLoginFlow.swift` | App-side wiring of login flow. |
| `Sources/CodexBar/UsageStore+ClaudeDebug.swift` | Debug dump (planner + per-source diagnostics). |

---

## 17. Documented vs. observed inconsistencies (port hygiene)

These were found while reading source vs. docs. Surface them in the new spec so the Windows port doesn't inherit them silently.

1. **Two `.auto` decision sites disagree.** `ClaudeProviderDescriptor` does `oauth → cli → web` (correct, current). `ClaudeUsageFetcher.executeAuto` independently does `oauth → web → cli`. Used only by debug probes today but still a bug. **Port: single planner only (§1.5).**
2. **Docs say opus weekly maps to `seven_day_sonnet` first.** Code agrees, but the *display* label is still `Sonnet` even when the underlying field is `seven_day_opus`. Confusing for opus-on-team users. **Port: rename label to "Model weekly" or expose the actual model.**
3. **`docs/claude.md` says "Web extras are internal-only".** Code exposes `claudeWebExtrasEnabled` as a public toggle when source is CLI. Reconcile the doc.
4. **`docs/CLAUDE.md` lists native log roots as `~/.config/claude/projects` first** but the code searches `CLAUDE_CONFIG_DIR` env first (each entry's `/projects` subdir). Docs are slightly behind.
5. **Watchdog handles `getppid() == 1` reparenting on macOS.** Windows has no exact equivalent; the Job Object's kill-on-close achieves the same end. Document the semantic difference in port notes.
6. **CLI `Account:` parsing has a known false-positive:** when the org name is the email prefix, code suppresses it as a duplicate. Document this so the Windows port doesn't "fix" what looks like a bug.
7. **`utilization` is sometimes Int, sometimes Double** in the same JSON response — handle both in Rust (`serde_json::Value::as_f64()` on a `Number`). Don't rely on `Number::is_i64`.
8. **`extra_usage` and `overage_spend_limit` both report amounts in cents** — both code paths divide by 100. Do *not* skip this for the OAuth-path version.
