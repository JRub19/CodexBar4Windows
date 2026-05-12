# 41. Provider: Codex (deep-dive blueprint for Windows refactor)

> Audience: a Rust + TypeScript engineer building the Codex provider on Tauri 2 + React + a shared Rust crate, with **Phantom-wallet / Duolingo level polish**. No Swift assumed.
> Source of truth on macOS lives under `Sources/CodexBarCore/Providers/Codex/`, `Sources/CodexBarCore/OpenAIWeb/`, `Sources/CodexBar/Providers/Codex/` and the `CodexAccountPromotion*` files in `Sources/CodexBar/`.
> This document describes **behavior, contracts, and state machines** — not Swift code. Code blocks are illustrative pseudocode (Rust where appropriate) and stay ≤15 lines.

---

## 1. Mental model: what is "Codex" inside this app?

"Codex" is the OpenAI Codex CLI agent. The provider surfaces three things to the user:

1. **Rate-limit windows** (5h "primary" + 7d "secondary", percent used, reset time).
2. **Credits** (balance, has-credits, unlimited flag, recent purchases/spend events).
3. **Dashboard extras** (code-review remaining, usage breakdown chart, credits-history chart, plan tier, account email).

Three independent backends produce that data:

| Backend                | Protocol                  | Trust level     | What it gives                                                                                |
| ---------------------- | ------------------------- | --------------- | -------------------------------------------------------------------------------------------- |
| **OAuth API**          | HTTPS w/ Bearer token     | Best (canonical)| windows, credits, plan, account_id (via JWT)                                                 |
| **Local Codex CLI**    | JSON-RPC over stdio       | Good            | windows, credits, account identity                                                           |
| **OpenAI web extras**  | HTTPS w/ session cookies  | Add-on only     | code review, usage breakdown chart, credits history, credits-purchase URL, signed-in email   |

The three are layered: OAuth or CLI is the *primary* source; web extras *enrich* the snapshot with dashboard-only fields. Web never owns the primary windows by itself (it can, as a degraded fallback, but is not the canonical path for the app runtime).

---

## 2. Selection order — when does each backend run?

There are **two runtimes** in CodexBar: the menubar app (`runtime=app`) and the standalone CLI tool (`runtime=cli`). Codex behaves differently in each.

### 2.1 App runtime (the menubar app users actually run)

| Setting `usageSource` | Order tried                                  | Notes                                                                            |
| --------------------- | -------------------------------------------- | -------------------------------------------------------------------------------- |
| `auto` (default)      | 1. OAuth API → 2. CLI RPC                    | If OAuth credentials exist they win. CLI is the fallback.                        |
| `oauth`               | Only OAuth API                               | Hard pin.                                                                        |
| `cli`                 | Only CLI RPC                                 | Hard pin.                                                                        |
| `web`                 | Only OpenAI web                              | Debug/legacy — not exposed in app UI when extras are off.                        |

**Web extras** are a *separate background refresh* layered on top, triggered when:

- `openAIWebAccessEnabled` toggle is on, **and**
- `codexCookieSource ∈ {auto, manual}` (i.e. user opted in), **and**
- The current account has a usable cookie source.

When extras succeed, the source label becomes `oauth + openai-web` (or `codex-cli + openai-web`).

### 2.2 CLI runtime (`CodexBarCLI usage --provider codex --source auto`)

| Setting `usageSource` | Order tried                                  |
| --------------------- | -------------------------------------------- |
| `auto`                | 1. Web → 2. CLI RPC                          |
| `oauth`               | OAuth API                                    |
| `web`                 | Web only                                     |
| `cli`                 | CLI only                                     |

> **Why different?** The CLI is often run in CI/scripts; OAuth tokens may be absent or stale, so web (with manual cookies) is the friendlier default there. The app always has the user logged in via `codex login`, so OAuth is the natural primary.

### 2.3 OAuth fallback semantics (subtle but important)

The OAuth strategy reports `shouldFallback = true` only for **recoverable auth states** the CLI can actually fix:

- `unauthorized` (401/403)
- `notFound` / `missingTokens` (auth.json missing or empty)
- Refresh token `expired` / `revoked` / `reused`

It does **not** fall back on `invalidResponse`, `serverError`, `networkError`, `decodeFailed`, refresh `networkError`, or refresh `invalidResponse` — because the CLI cannot repair those, and silently spawning `codex app-server` repeatedly on every refresh tick would burn CPU and obscure the real failure.

**Windows implication:** mirror this exact predicate. Wrap it as `fn should_fallback(err: &CodexOAuthError) -> bool` and unit-test all branches.

---

## 3. OAuth API path

### 3.1 Credential file

- **Path:** `~/.codex/auth.json` (override: `$CODEX_HOME/auth.json`).
- On Windows: `%USERPROFILE%\.codex\auth.json` (default), `%CODEX_HOME%\auth.json` (override). The CLI itself uses the same file, so we do not need to teach the CLI a Windows path.
- **Permissions:** macOS sets `chmod 0600` after every write. Windows: use restrictive ACLs (owner-only) via `windows-acl` or call `icacls` — see §13.

### 3.2 File schema

```json
{
  "OPENAI_API_KEY": null,
  "tokens": {
    "id_token":      "eyJ...",
    "access_token":  "eyJ...",
    "refresh_token": "rt_...",
    "account_id":    "account-..."
  },
  "last_refresh": "2025-12-28T12:34:56Z"
}
```

Both snake_case (`access_token`) and camelCase (`accessToken`) are accepted on read. We always write snake_case to match the upstream Codex CLI.

If `OPENAI_API_KEY` is a non-empty string and `tokens` is absent/empty → **API-key-only auth**. CodexBar treats this as a degraded mode: no refresh, accountId unknown; OAuth strategy still works (the key goes in as the `Authorization: Bearer` value). The promotion flow (§5) explicitly rejects API-key-only **live** auth because there is nothing to safely preserve.

### 3.3 JWT identity extraction

The `id_token` is a JWT whose payload contains:

- `email` (sometimes nested in `https://api.openai.com/profile`)
- `chatgpt_plan_type` (sometimes nested in `https://api.openai.com/auth`)
- `chatgpt_account_id` (workspace/provider account id)

This is the **canonical identity** used for matching managed accounts to live accounts and for the email-hash scoping in history ownership (§7). Decoding rules:

| Field          | Lookup order                                                                                          |
| -------------- | ----------------------------------------------------------------------------------------------------- |
| email          | `payload.email` → `payload["https://api.openai.com/profile"].email`                                   |
| plan           | `payload["https://api.openai.com/auth"].chatgpt_plan_type` → `payload.chatgpt_plan_type`              |
| account_id     | `tokens.account_id` → `payload["https://api.openai.com/auth"].chatgpt_account_id` → `payload.chatgpt_account_id` |

Trim, lowercase the email, normalize the account_id (lowercased trimmed string), then build a `CodexIdentity`:

```rust
enum CodexIdentity {
    ProviderAccount(String),   // canonical (account_id present)
    EmailOnly(String),         // legacy / API-key-only
    Unresolved,                // no usable identity
}
```

### 3.4 Refresh flow

| Field        | Value                                                                |
| ------------ | -------------------------------------------------------------------- |
| URL          | `POST https://auth.openai.com/oauth/token`                           |
| Headers      | `Content-Type: application/json`                                     |
| Body         | `{ client_id, grant_type: "refresh_token", refresh_token, scope }`   |
| `client_id`  | `app_EMoamEEZ73f0CkXaXp7hrann`                                       |
| `scope`      | `openid profile email`                                               |
| Trigger      | `now - last_refresh > 8 days` (`needsRefresh`)                       |
| Timeout      | 30s                                                                  |

Response is JSON: `{ access_token, refresh_token, id_token }`. On success, persist the new triple plus `last_refresh = now` (ISO 8601 with fractional seconds preferred on read; emit `.iso8601()` on write). On 401, parse `error.code` (or fall back to `error` / `code`):

| code                              | Mapped to            |
| --------------------------------- | -------------------- |
| `refresh_token_expired`           | `RefreshError::Expired`     |
| `refresh_token_reused`            | `RefreshError::Reused`      |
| `invalid_grant` / `refresh_token_invalidated` | `RefreshError::Revoked` |
| anything else                     | `RefreshError::Expired` (treat as terminal) |

### 3.5 Usage API call

| Field         | Value                                                                  |
| ------------- | ---------------------------------------------------------------------- |
| URL (default) | `GET https://chatgpt.com/backend-api/wham/usage`                       |
| URL (alt)     | `GET {chatgpt_base_url}/api/codex/usage` if base URL lacks `/backend-api` (PathStyle resolution from upstream Codex's `backend-client`) |
| Headers       | `Authorization: Bearer <access_token>`, `Accept: application/json`, `Accept-Language: en-US,en;q=0.9`, `User-Agent: CodexBar`, `ChatGPT-Account-Id: <account_id>` (if known) |
| Timeout       | ~10s; refresh is 30s                                                   |

### 3.6 Response decode (be tolerant)

```json
{
  "plan_type": "pro",
  "rate_limit": {
    "primary_window":   { "used_percent": 15, "reset_at": 1735401600, "limit_window_seconds": 18000 },
    "secondary_window": { "used_percent":  5, "reset_at": 1735920000, "limit_window_seconds": 604800 }
  },
  "credits": { "has_credits": true, "unlimited": false, "balance": 150.0 }
}
```

Decoder rules:

- `plan_type` is an open enum: known values (`guest, free, go, plus, pro, free_workspace, team, business, education, quorum, k12, enterprise, edu`) plus an `Unknown(String)` fallback. Never error on a new tier.
- Window decode failures are **isolated**: if `primary_window` fails to parse, set primary to `None` but still keep `secondary_window`. Tag a `primary_window_decode_failed` flag for telemetry.
- If both windows are missing/failed but `credits` decoded — still return a partial `ProviderFetchResult` with credits and `Unresolved` windows. This prevents the auto-mode fallback from escalating a usable response into a CLI spawn.

### 3.7 Mapping to `UsageSnapshot`

`UsageSnapshot { primary, secondary, tertiary, provider_cost, updated_at, identity }`:

- `primary.used_percent`   = `primary_window.used_percent` (clamped 0..=100)
- `primary.window_minutes` = `primary_window.limit_window_seconds / 60`
- `primary.resets_at`      = Unix `reset_at` → `DateTime<Utc>`
- `primary.reset_description` = relative ("Resets in 2 h 13 m") — see `UsageFormatter::reset_description`
- Same for `secondary`.
- `identity = ProviderIdentitySnapshot { provider_id: Codex, account_email, account_organization, login_method: plan_type.raw_value() }`.

---

## 4. Codex CLI integration (RPC + diagnostic PTY)

There are **two** CLI integrations. Don't confuse them.

### 4.1 RPC (the primary CLI strategy)

- Binary lookup: `codex` on PATH, plus app-bundled fallbacks (`BinaryLocator`).
- Spawn command:

  ```
  codex -s read-only -a untrusted app-server
  ```

- **Transport:** JSON-RPC 2.0 over **stdin/stdout** of the child. Not a PTY. On Windows use `tokio::process::Command` with `.stdin(Stdio::piped()).stdout(Stdio::piped())`. Wrap stdout in a line-delimited JSON-RPC framer.
- Methods called, in order:
  1. `initialize` (params: `clientName`, `clientVersion`)
  2. `account/read` (returns email + plan + workspaceId)
  3. `account/rateLimits/read` (returns windows + credits)
- **Timeouts:** `initialize` gets a longer startup budget (~5–8 s) to allow node + bun + login-shell PATH resolution; subsequent reads are short (~2–3 s).
- **On timeout:** kill the child (`process.kill().await`). This unblocks the stdout reader instead of leaving the refresh hung. Forward an explicit "CLI timeout" error.
- **Error contract:** any app-server error is **terminal** for the CLI strategy (no further retries within that strategy) **except** when the error body embeds a `wham/usage` JSON blob, in which case Codex parses it as a recoverable usage response. Preserve that special case verbatim.

### 4.2 PTY `/status` diagnostic (debug-only)

- Used **only** for manual diagnostics (Preferences → Debug → "Run /status diagnostic"). The CLI runtime's automatic refresh does **not** launch this.
- Implementation runs `codex` (no subcommand) in a real PTY (`openpty()` on POSIX). Windows equivalent: use `conpty` via the `portable-pty` crate.
- Steps:
  1. Open pty, spawn `codex -s read-only -a untrusted`.
  2. Drain output for 400 ms to settle the TUI.
  3. Respond to cursor-position queries (`ESC [ 6 n` → reply `ESC [ 1;1 R`) to keep the TUI happy.
  4. Detect "Update available!" prompt and dismiss it (`ESC [ B` + `Enter`).
  5. Send `/status\r`. Resend on no response (max 2 resends), with `Enter` retries (max 6).
  6. Wait for a status marker: `Credits:`, `5h limit`, `5-hour limit`, or `Weekly limit`.
  7. After the marker appears, drain ~2 s more for the panel to finish rendering.
  8. Decode UTF-8, return the rendered text; caller parses it via `CodexStatusProbe` regex.
- Update-needed detection: any of `Update available!`, `Run bun install -g @openai/codex`, `0.60.1 ->` (case-insensitive substring) → surface a "CLI update needed" error.

> **Confirm:** RPC is **not** PTY. PTY is **only** used for the explicit `/status` diagnostic. Document this in the Windows scaffolding so we don't accidentally PTY the RPC.

### 4.3 Output parsing (PTY)

`CodexStatusProbe` parses the rendered `/status` panel after ANSI stripping:

| Pattern (after `strip_ansi`) | Field                                          |
| ---------------------------- | ---------------------------------------------- |
| `Credits:\s*<num>`           | credits balance (number, may include `,`)       |
| `5h limit \(([0-9]+)%[^)]*\) — resets <text>` | primary window percent + reset text |
| `Weekly limit \(([0-9]+)%[^)]*\) — resets <text>` | weekly window percent + reset text |

---

## 5. OpenAI web dashboard extras (the big one)

### 5.1 What "extras" actually means

Cell-by-cell, the dashboard adds:

| Field                              | OAuth gives it? | Web gives it? | UI surface                            |
| ---------------------------------- | --------------- | ------------- | ------------------------------------- |
| `primary_limit` (5h)               | yes             | yes (parsed)  | Session ring                          |
| `secondary_limit` (weekly)         | yes             | yes (parsed)  | Weekly ring                           |
| `credits_remaining`                | yes (balance)   | yes (parsed)  | "Credits remaining" row               |
| `code_review_remaining_percent`    | no              | yes           | "Code review N% remaining" row        |
| `code_review_limit` (RateWindow)   | no              | yes           | Code-review ring                      |
| `usage_breakdown[]` (30 days)      | no              | yes           | Stacked bar chart in menu             |
| `credit_events[]` (table rows)     | no              | yes           | Credits history line chart            |
| `daily_breakdown[]` (derived)      | no              | yes (derived) | Tooltip/breakdown                     |
| `credits_purchase_url`             | no              | yes           | "Buy credits" button → §8             |
| `signed_in_email`                  | weak (JWT)      | strong (DOM)  | Account match / mismatch detection    |
| `account_plan`                     | yes             | yes (parsed)  | Plan badge                            |

Web is the only path for `code_review`, `usage_breakdown`, and `credit_events`. When extras succeed they are **merged into** the snapshot produced by OAuth/CLI — never replace it unless windows were missing.

### 5.2 Two ways cookies arrive

```
Preferences → Providers → Codex → OpenAI cookies = Off | Automatic | Manual
```

- **Off** — web extras disabled entirely. (Also forced when `openAIWebAccessEnabled = false`.)
- **Automatic** — CodexBar reads cookies from local browsers (next section).
- **Manual** — user pastes a `Cookie:` header from a `chatgpt.com` request in DevTools.

### 5.3 Automatic cookie import (the source order)

| Order | Browser                  | macOS path                                                                       | Windows path (Tauri target)                                                                                          |
| ----- | ------------------------ | -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| 1     | Safari (Mac only)        | `~/Library/Cookies/Cookies.binarycookies`                                        | n/a — Safari not on Windows                                                                                          |
| 2     | Chrome / Edge / Brave / Arc / Vivaldi / Opera (Chromium engines) | `~/Library/Application Support/<Vendor>/.../Cookies` (SQLite, Chrome Safe Storage / Keychain decryption) | `%LOCALAPPDATA%\Google\Chrome\User Data\<Profile>\Network\Cookies` etc. SQLite + **DPAPI v10 decryption** of the `os_crypt_v10` blob from `Local State`. |
| 3     | Firefox                  | `~/Library/Application Support/Firefox/Profiles/*/cookies.sqlite`                | `%APPDATA%\Mozilla\Firefox\Profiles\*\cookies.sqlite` (no decryption needed; sqlite is plaintext)                    |

Domains imported: `chatgpt.com` and `openai.com`. **No cookie-name filter** — all cookies for those domains are slurped, then we test against the dashboard.

Each browser source produces zero or more **candidates** (a candidate = label + list of HTTP cookies). For each candidate, in order:

1. Build a `Cookie:` header from `chatgpt.com` cookies only.
2. Hit `GET https://chatgpt.com/backend-api/me` (then `…/api/auth/session`) with `Cookie: …` + `Accept: application/json` + 10s timeout. BFS-scan the JSON for the first `email` key whose value contains `@`. This is the **fast API path** that avoids touching WebKit.
3. If API email known and target email known → match / mismatch decision.
4. If API path produced nothing, fall back to a WebView probe (see §5.5).
5. On match: persist cookies into the per-account WebKit data store, run a confirm-probe against the persistent store, return success.
6. On mismatch: still persist the cookies keyed by the *found* email (so a future account switch can reuse them), but report `NoMatchingAccount`.

### 5.4 Per-account isolated cookie jars

On macOS, `WKWebsiteDataStore(forIdentifier: UUID)` gives a persistent isolated cookie jar per account. Identifier = SHA-256 of normalized lowercased email, truncated to 16 bytes, masked into a v4 UUID. Stores are cached by email so the same `WKWebsiteDataStore` instance is returned (preserves WebView cache identity).

**Windows mapping:** use **one of**:

- A `reqwest::cookie::Jar` per account (in-memory), serialized to disk in `%LOCALAPPDATA%\CodexBar\openai-dashboard-jars\<email-uuid>.json`. Restore on boot.
- For the first-time interactive login or Cloudflare interstitial recovery, embed a **WebView2** window pointed at `https://chatgpt.com/codex/cloud/settings/analytics#usage`, then extract cookies via `ICoreWebView2_2.CookieManager`.

Per-account isolation matters for the multi-account stacked menu (§11) — switching accounts must not nuke another account's session.

### 5.5 The scraping pipeline (Mac WebKit; Windows = `reqwest` + headless WebView2 fallback)

On macOS the scrape is a hybrid:

1. **API preflight** (fast, no WebView): build the `Cookie:` header from the per-account data store, hit `GET https://chatgpt.com/backend-api/wham/usage` (4 s timeout) and `GET …/backend-api/me` (2 s) for identity.
2. **Off-screen WebView** (1×1 px sliver, `alpha=0.001` to avoid throttling) navigates to `https://chatgpt.com/codex/cloud/settings/analytics#usage`.
3. **Inject `openAIDashboardScrapeScript`** (a single JS payload) and `evaluateJavaScript`. The script returns one dict per scrape with keys:
   - `loginRequired`, `workspacePicker`, `cloudflareInterstitial`, `href`
   - `bodyText`, `bodyHTML`
   - `signedInEmail` (from `client-bootstrap` JSON or `__NEXT_DATA__`)
   - `creditsPurchaseURL`
   - `rows[]` — flattened DOM rows of the Credits usage history table
   - `usageBreakdownJSON` — Recharts dataset for the 30-day stacked bar chart
   - `usageBreakdownDebug`, `usageBreakdownError`
   - `scrollY`, `scrollHeight`, `viewportHeight`
   - `creditsHeaderPresent`, `creditsHeaderInViewport`, `didScrollToCredits`
4. **Polling loop with deadlines** (default 60 s budget). It repeats scrape calls and accumulates the latest non-empty fields. It actively:
   - Forces navigation back to the usage URL if the SPA wanders off.
   - Waits up to 2.5 s after the credits header becomes visible for the (often virtualized) table to render.
   - Waits up to 4 s for `usageBreakdownError` to clear before giving up on the chart.
   - Re-scrolls into view when `creditsHeaderPresent && !creditsHeaderInViewport`.

**Failure conditions** (throw):

- `loginRequired` (also triggered when `client-bootstrap.authStatus != "logged_in"`)
- `cloudflareInterstitial`
- Deadline exceeded with no returnable data → `noDashboardData(body sample)`

### 5.6 Body parsing (regex + tolerant)

Body-text parsers (`OpenAIDashboardParser`), all locale-aware:

| Field                      | Regex / strategy                                                                 |
| -------------------------- | -------------------------------------------------------------------------------- |
| `code_review_remaining`    | `Code\s*review[^0-9%]*([0-9]{1,3})%\s*remaining` (also "Core review" typo)       |
| `credits_remaining`        | `credits\s*remaining[^0-9]*([0-9][0-9.,]*)` (+ "remaining credits", "credit balance") |
| 5h limit                   | line containing `5h`, `5-hour`, `5 hour`, or `\b5\s*h\b` → next 5 lines: percent + reset text |
| Weekly limit               | line containing `weekly`, `7-day`, `7d`, `\b7\s*d\b` → same windowed parse       |
| Code review limit          | line containing `code review`/`core review` but **not** `github code review`      |
| Plan                       | BFS-scan `client-bootstrap` then `__NEXT_DATA__` JSON for keys containing `plan`/`tier`/`subscription`; whitelist values `free, plus, pro, team, enterprise, business, edu, education, gov, premium, essential` |
| Signed-in email            | parse `client-bootstrap` `session.user.email` first, then BFS for any `email` key with `@` |
| Auth status                | `client-bootstrap.authStatus` — anything ≠ `logged_in` forces `loginRequired`    |

**Credits-used numeric parsing** handles localized decimals: US `1,234.56` (thousands), EU `1.234,56` (when token contains "crédit" → swap commas to dots), and unicode thin spaces (` `, ` `).

**Reset-time parsing** handles "today", "tomorrow", "Tuesday", "Mon", "Mar 5 9:00pm", "3/5", "2025-03-05", etc., advancing into the future when a relative day already passed.

### 5.7 Filtering noise from `usage_breakdown`

`OpenAIDashboardDailyBreakdown.removingSkillUsageServices` strips any service whose name starts with `skillusage:` (lowercased, trimmed) — these are internal markers, never shown.

### 5.8 Cookie-header cache (per-provider/per-account)

Cookies that worked once are cached to **`com.steipete.codexbar.cache`** Keychain entry on macOS (account = `cookie.codex`, scope = `managedAccount(UUID)` or default). Payload: `{ source_label, stored_at, cookie_header }`. On next refresh:

- If the cached cookie header is non-empty, try it first via the **manual cookie path** (skip browser scraping → skip Keychain prompts).
- If the cached cookie header probe returns `manualCookieHeaderInvalid`, `noMatchingAccount`, or `dashboardStillRequiresLogin` → clear the cache entry and retry browser import.

**Windows mapping:** Windows Credential Manager (via the `keyring` crate) under the same scoping convention. Service name `CodexBar.cache`, target `cookie.codex` or `cookie.codex.account-<uuid>`.

### 5.9 Diagnostics surfaced to UI

| Symptom                                    | UI message                                                                              |
| ------------------------------------------ | --------------------------------------------------------------------------------------- |
| `loginRequired`                            | "OpenAI web access requires login." → "Refresh OpenAI cookies and try again."           |
| `cloudflareInterstitial`                   | "OpenAI web refresh hit a Cloudflare challenge."                                        |
| `noMatchingAccount(found)`                 | "OpenAI web session does not match Codex account. Found: Safari=alice@…, Chrome=bob@…." |
| `dashboardStillRequiresLogin`              | "Browser cookies imported, but dashboard still requires login."                         |
| `manualCookieHeaderInvalid`                | "Manual cookie header is missing a valid OpenAI session cookie."                        |
| `browserAccessDenied(details)`             | "Browser cookie access denied. \<hint\>" (e.g. macOS Keychain rejected `Chrome Safe Storage`) |
| `noDashboardData(body)`                    | "OpenAI dashboard data not found. Body sample: …"                                       |

---

## 6. Account promotion flow (THE hard one — most space here)

This is unique to Codex among all providers. It exists because:

- The Codex CLI itself ("live system Codex") owns the file `~/.codex/auth.json`.
- CodexBar can also store **managed** Codex accounts in its own per-account home directories under `~/Library/Application Support/CodexBar/managed-codex-homes/<UUID>/auth.json`.
- The user wants a single mental model: "switch which Codex account my terminal `codex` command uses, *and* what CodexBar is tracking."
- "Promotion" = take a managed account and **make it the live system account** (overwrite `~/.codex/auth.json` with the managed account's auth, while safely preserving the previous live account as another managed account so nothing is lost).

### 6.1 Vocabulary (memorize)

| Term                               | Meaning                                                                                                                  |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| **Live system account**            | Whatever `~/.codex/auth.json` (or `$CODEX_HOME/auth.json`) currently points at. Owned by the user's Codex CLI install.     |
| **Managed account**                | A Codex account CodexBar stores inside its own sandbox under `managed-codex-homes/<UUID>/auth.json`.                      |
| **`CodexActiveSource`**            | App setting: `liveSystem` or `managedAccount(UUID)`. Picks which auth CodexBar uses for fetches.                          |
| **Promotion**                      | Make a managed account become the new live system account.                                                                |
| **Displaced live**                 | The live account that was overwritten by promotion. We must save it as a managed account first.                          |
| **Converged no-op**                | The target managed account already matches the live system account — no file mutation needed.                            |
| **Identity matching**              | Compare by `provider_account_id` first, then by normalized email. A mismatch on either is a true mismatch.               |

### 6.2 User-visible behavior

In the menu: **Codex → System Account → ▾**. A submenu lists every managed account plus the live system account. A checkmark next to the current live one. Clicking another row triggers a **promotion**:

1. Coordinator marks `isPromotingSystemAccount = true` (UI: spinner on row, disable other rows).
2. Service plans + executes the swap (see state machine).
3. On completion, the menu rebuilds with the new checkmark; a toast/notification announces "Now using `<email>` for Codex CLI."

If the promotion fails the user sees a modal with a friendly title + reason and no destructive change has occurred to `~/.codex/auth.json`.

### 6.3 The five files (responsibility split)

| File                                       | Responsibility                                                                                                          |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| `CodexAccountPromotionCoordinator`         | Observable @MainActor object. Owns `isAuthenticatingLiveAccount`, `isPromotingSystemAccount`, `userFacingError`. Calls `service.promoteManagedAccount(id)` and maps errors. |
| `CodexAccountPromotionService`             | Orchestrator. Builds the context, decides "converged no-op vs full promotion", and calls planner + executor + auth swap. |
| `CodexAccountPromotionPreparation`         | Reads everything from disk: target managed account, live `auth.json`, JWT identities, workspace identities. Produces a `PreparedPromotionContext`. |
| `CodexAccountPromotionPlanning`            | Pure function. Given the prepared context, returns one of `none / reject / importNew / refreshExisting / repairExisting` for the displaced-live disposition. **No filesystem side effects.** |
| `CodexAccountPromotionExecution`           | Applies the plan: imports / refreshes / repairs the managed-account record for the displaced live. **Never swaps live auth itself** — that is the service's last step. |

### 6.4 Inputs prepared up-front

```
PreparedPromotionContext {
  snapshot:               CodexAccountReconciliationSnapshot,   // settings projection
  managedAccounts:        ManagedCodexAccountSet,
  storedManagedAccounts:  [PreparedStoredManagedAccount],       // each has authIdentity + homeState
  target:                 PreparedStoredManagedAccount,         // the one we promote
  live:                   PreparedLiveAccount,                  // ~/.codex
}
```

`PreparedStoredManagedAccount.homeState ∈ { readable(authMaterial), missing(homeURL), unreadable(homeURL) }`.

`PreparedLiveAccount.homeState ∈ { missing, unreadable, apiKeyOnly(authMaterial), readable(authMaterial) }`.

`PreparedAuthMaterial` packages the raw bytes, the parsed `CodexOAuthCredentials`, the `CodexAuthBackedAccount` (email + plan + identity), and an `authIdentity: PreparedIdentity { email, identity, providerAccountID, workspaceLabel, workspaceAccountID }`.

### 6.5 Converged no-op detection

Before planning, the service checks: does the target *already* match the live? If yes, no file mutation. Logic:

1. If `live.authIdentity` exists, compare to `target.authIdentity ?? target.persistedIdentity` via `CodexIdentityMatcher.matches`.
   - If they match **and** `live.authIdentity.email != nil` → `liveSystem`.
   - If they match **and** `live.authIdentity.email == nil` but `providerAccountID != nil` → `managedAccount(target.id)`.
2. If `live.authIdentity` is absent but `snapshot.liveSystemAccount` exists and its identity matches target → `liveSystem`.
3. Otherwise → not converged; proceed to full promotion.

On converged: write the resolved `CodexActiveSource`, kick a scoped refresh, return outcome `convergedNoOp` with `didMutateLiveAuth = false`.

### 6.6 Planning the displaced-live disposition

Given `live.homeState`:

| live state                 | Plan                                          |
| -------------------------- | --------------------------------------------- |
| `missing`                  | `none(liveMissing)` — nothing to preserve     |
| `unreadable`               | `reject(liveUnreadable)`                      |
| `apiKeyOnly`               | `reject(liveAPIKeyOnlyUnsupported)`           |
| `readable` but no identity | `reject(liveIdentityMissingForPreservation)`  |
| `readable` matching target | `none(targetMatchesLiveAuthIdentity)`         |
| `readable`, distinct       | search for an existing managed dest…          |

For the "readable, distinct" case the planner walks all *other* stored managed accounts and tries to find a **destination** in this priority order:

1. **`refreshExisting`** — a managed account whose live `authIdentity` matches the displaced-live identity (by provider account id, or by email if id missing). Its on-disk auth will be refreshed with the displaced live bytes. Reason:
   - `readableHomeIdentityMatch` (live had a real email), or
   - `readableHomeIdentityMatchUsingPersistedEmailFallback` (live had only providerAccountID).
2. **`reject(conflictingReadableManagedHome)`** — there is *another* readable managed home with a *different* identity but the same `provider_account_id`. Refuse to clobber, surface a "resolve duplicate first" error.
3. **`repairExisting`** — persisted record matches by `providerAccountID` (or legacy email-only match) but the on-disk home is `missing` or `unreadable`. Repair it by writing new bytes. Reasons:
   - `persistedProviderMatchWithMissingHome`
   - `persistedProviderMatchWithUnreadableHome`
   - `persistedLegacyEmailMatch`
4. **`importNew(noExistingManagedDestination)`** — no existing record fits; create a fresh managed account for the displaced live.

### 6.7 Execution semantics (safety contract)

```text
Safety:
  - Executor never swaps live auth. The service does that AS THE LAST STEP, only after planning + execution succeed.
  - Import: best-effort cleanup. On any error, the freshly created managed home is removed (only if it is inside the managed-homes root — never delete outside the sandbox).
  - Refresh/repair: writes the new auth.json (0600 perms) BEFORE store commit (matches current behavior; restartable).
```

Steps for `importNew`:

1. `homeFactory.makeHomeURL()` → fresh `<root>/<UUID>/` (where root is `<AppSupport>/CodexBar/managed-codex-homes/`).
2. `mkdir -p`. Write displaced-live `auth.json` atomically with 0600 perms.
3. Build a `ManagedCodexAccount` from displaced-live identity (`email`, `providerAccountID`, optional workspace fields, `managedHomePath`, timestamps).
4. `store.loadAccounts() → append new → store.storeAccounts(...)`.
5. After commit, **re-read the store** and resolve final disposition (handles concurrent edits): if account is there → `.imported(id)`. If a same-identity record exists but with a different home path → `.alreadyManaged(id)` and clean up the orphan path.

Steps for `refreshExisting` / `repairExisting`:

1. Validate the destination is inside the managed-homes root (`homeFactory.validateManagedHomeForDeletion`) — refuse otherwise.
2. `mkdir -p` (idempotent). Write displaced-live `auth.json` (0600).
3. Build a refreshed `ManagedCodexAccount` (preserves `id` and `createdAt`, updates everything else).
4. Atomic replace in the store.

### 6.8 The final live swap

Only after planning+execution succeed:

```rust
liveAuthSwapper.swap(target_auth_bytes, live_home_url)?;
// implementation:
//   write to "<live_home>/auth.json.codexbar-staged-<UUID>" with 0600
//   atomic rename → "<live_home>/auth.json"
// on error: try to clean up the staged file; map to liveAuthSwapFailed
```

On Windows: use `tokio::fs::rename` (atomic on the same volume). For the 0600-equivalent, set restrictive ACLs (owner-only) **before** the rename so the renamed file inherits them. If the file system is on a different volume than the staged file path, fall back to `fs::copy` + `fs::remove_file` after explicit sync.

Then: `activeSourceWriter.write(.liveSystem)`, `accountScopedRefresher.refresh(allowDisabled: true)`, return success.

### 6.9 State machine summary

```
                       ┌────────────────┐
 click submenu row ──▶ │ Coordinator    │ (isPromotingSystemAccount = true)
                       └────────┬───────┘
                                │
                                ▼
                 ┌─────────────────────────────┐
                 │ Build PreparedPromotionCtx  │  (read disk, JWT, workspaces)
                 └─────────────┬───────────────┘
                               │
                ┌──────────────┴──────────────┐
                │                              │
       converged check passes?           else continue
                │                              │
                ▼                              ▼
       write activeSource = …       Plan displaced-live disposition
       refresh scoped state          (none/reject/import/refresh/repair)
       return convergedNoOp                    │
                                               ▼
                                Execute plan against managed store
                                               │
                                               ▼
                            Atomic write live auth.json (target's bytes)
                                               │
                                               ▼
                            activeSource = liveSystem; refresh; toast
```

### 6.10 Error → UI mapping (use exactly these strings; they are tested)

| `CodexAccountPromotionError`                  | User-facing message                                                                                          |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `targetManagedAccountNotFound`                | "That account is no longer available in CodexBar. Refresh the account list and try again."                   |
| `targetManagedAccountAuthMissing`             | "CodexBar could not find saved auth for that account. Re-authenticate it and try again."                     |
| `targetManagedAccountAuthUnreadable`          | "CodexBar could not read saved auth for that account. Re-authenticate it and try again."                     |
| `liveAccountUnreadable`                       | "CodexBar could not read the current system account on this Mac." (Windows: "on this PC")                     |
| `liveAccountMissingIdentityForPreservation`   | "CodexBar could not safely preserve the current system account before switching."                            |
| `liveAccountAPIKeyOnlyUnsupported`            | "CodexBar can't replace a system account that is signed in with an API key only setup."                      |
| `displacedLiveManagedAccountConflict`         | "CodexBar found another managed account that already uses the current system account. Resolve duplicate first." |
| `displacedLiveImportFailed`                   | "CodexBar could not save the current system account before switching."                                       |
| `managedStoreCommitFailed`                    | "CodexBar could not update managed account storage."                                                         |
| `liveAuthSwapFailed`                          | "CodexBar could not replace the live Codex auth on this Mac." (Windows: "on this PC")                         |
| Interaction blocked (concurrent op)           | "Finish the current managed account change before switching the system account."                            |

All under title **"Could not switch system account"**.

### 6.11 Interaction guards

The coordinator considers interaction **blocked** if any of:

- `isPromotingSystemAccount` is already true.
- `isAuthenticatingLiveAccount` is true (the user is `codex login`-ing the live account).
- `managedAccountCoordinator.hasConflictingManagedAccountOperationInFlight` is true (a managed account is mid-add or mid-remove).

If blocked, surface the "finish the current change first" message and **do not** start a new promotion.

### 6.12 Polish hooks (Phantom / Duolingo)

- **Optimistic UI:** swap the checkmark immediately, dim the row with a spinner. Revert if the promise rejects.
- **Confetti-light:** a one-line toast in the menu, slid-in then fading out at 1.4 s. Use the brand teal `#49A3B0` (Codex color).
- **Sound:** subtle, optional. Off by default.
- **Empty / first-time:** if there are zero managed accounts, the submenu shows a single "Add account…" row that opens the login flow.

---

## 7. Managed account state, reconciliation, history ownership

### 7.1 What "managed" means

A **managed account** is an alternate Codex login that lives **inside CodexBar's sandbox**, not in `~/.codex`. Each gets its own home directory:

```
<AppSupport>/CodexBar/managed-codex-homes/<UUID>/auth.json
```

Plus a row in the catalog:

```
<AppSupport>/CodexBar/managed-codex-accounts.json
```

with schema (v2):

```json
{
  "version": 2,
  "accounts": [{
    "id": "<UUID>",
    "email": "alice@example.com",
    "providerAccountID": "account-abc",
    "workspaceLabel": "Acme Corp",
    "workspaceAccountID": "ws-abc",
    "managedHomePath": "<AppSupport>/CodexBar/managed-codex-homes/<UUID>",
    "createdAt": 1735000000.0,
    "updatedAt": 1735100000.0,
    "lastAuthenticatedAt": 1735100000.0
  }]
}
```

Decoding rules:

- `version` must be 1 or 2. `1` triggers a one-shot migration: for each row, hydrate `providerAccountID` by parsing the local `auth.json` JWT.
- Sanitization (sets, dedup): unique by `id`, by `(email, providerAccountID)` if id known, by `email` if only legacy.

Permissions: `0600` on the catalog file (Mac). Same on Windows via owner-only ACL.

**Windows mapping:** `%APPDATA%\CodexBar\managed-codex-homes\<UUID>\auth.json` (roaming) — or use `%LOCALAPPDATA%` to avoid sync. Recommend `%LOCALAPPDATA%` because tokens roaming across machines is bad. Catalog: `%LOCALAPPDATA%\CodexBar\managed-codex-accounts.json`.

### 7.2 Reconciliation snapshot

Every refresh, the `CodexAccountReconciler` produces a `CodexAccountReconciliationSnapshot`:

```
storedAccounts:              [ManagedCodexAccount]
activeStoredAccount:         Option<ManagedCodexAccount>   // matches activeSource
liveSystemAccount:           Option<ObservedSystemCodexAccount>
matchingStoredAccountForLiveSystemAccount: Option<…>
activeSource:                CodexActiveSource             // user preference
hasUnreadableAddedAccountStore: bool                       // catalog read error
storedAccountRuntimeIdentities: HashMap<UUID, CodexIdentity>
storedAccountRuntimeEmails:     HashMap<UUID, String>
```

The `CodexActiveSourceResolver` then computes a **resolved** source: when `activeSource = managedAccount(id)` but that managed account's identity matches the live system identity, the resolved source is `liveSystem` (don't waste a second auth indirection). The persisted value may be corrected via `persistResolvedCodexActiveSourceCorrectionIfNeeded()`.

### 7.3 Coordinator / Service split (managed accounts, separate from promotion)

| Component                            | Job                                                                                              |
| ------------------------------------ | ------------------------------------------------------------------------------------------------ |
| `ManagedCodexAccountCoordinator`     | Observable; tracks `isAuthenticatingManagedAccount`, `isRemovingManagedAccount`. Calls service.  |
| `ManagedCodexAccountService.authenticateManagedAccount(existingAccountID, timeout)` | Allocates a new managed home, runs `CodexLoginRunner` against that home, parses the resulting `auth.json`, optionally prompts to pick a workspace, persists. Replaces any same-(email, providerAccountID) record. |
| `ManagedCodexAccountService.removeManagedAccount(id)` | Validates the home path is inside the sandbox, removes the catalog entry, deletes the home dir. |

The login subprocess is exactly:

```
env CODEX_HOME=<managed-home>  codex login
```

— spawned via `/usr/bin/env codex login` on macOS, with a 120 s default timeout, `setpgid` so we can kill the whole process group on timeout. **Windows:** use `tokio::process::Command::new("codex.exe").arg("login").env("CODEX_HOME", home).env("PATH", augmented_path)`. Group kill via `JobObject` (or `taskkill /T /F /PID <pid>`).

On success, the service reads `<home>/auth.json`, decodes the JWT, and **optionally** resolves the workspace identity:

- Call `CodexOpenAIWorkspaceResolver.listWorkspaces(credentials)` (uses OAuth token to query OpenAI's workspaces API).
- If more than one workspace, present an `NSAlert` popup ("Choose Codex workspace" + a sorted list). On confirm, persist the choice back into `auth.json` (`tokens.account_id = <workspaceID>`). On cancel → `workspaceSelectionCancelled` error.

**Windows mapping:** replace `NSAlert` with a Tauri command that opens a small Webview dialog with a `<select>`, a "Add Workspace" button, and a "Cancel" button. Persist the same way.

### 7.4 Codex history ownership and per-account scoping

There is a long history of plan-utilization samples (8-week local history) that must follow the **same** account across sessions. The keying scheme:

| Identity                       | Canonical key                                                                |
| ------------------------------ | ---------------------------------------------------------------------------- |
| `ProviderAccount(id)`          | `codex:v1:provider-account:<normalized-id>`                                  |
| `EmailOnly(email)`             | `codex:v1:email-hash:<sha256(normalized_email)>`                             |
| `Unresolved`                   | no canonical key (history won't load for that pseudo-account)                |

Legacy stored keys are classified:

- `canonical(key)` — already in the new format.
- `legacyEmailHash(hash)` — raw `sha256(email)` left over from v1.
- `legacyOpaqueScoped(key)` — some other historical scope.
- `legacyUnscoped` — global bucket from very early versions.

`CodexHistoryOwnership.belongsToTargetContinuity` decides whether a stored key "belongs" to the current target: a legacy email hash counts only if the target has a matching email-hash key (i.e. we promoted email-only → provider-account but it's the same human).

`CodexOwnershipContext` aggregates the runtime identity (via reconciliation snapshot) **plus**:

- A `hasAdjacentMultiAccountVeto` flag that's `true` when the active managed account and the live system account resolve to *different* identities. When set, no legacy/loose continuity matches are accepted — strict-only.
- `currentWeeklyResetAt` from the latest snapshot or attached dashboard.

This prevents history from a *different* logged-in account leaking into the current view in multi-account setups.

---

## 8. Buy Credits window (`OpenAICreditsPurchaseWindowController`)

When the user clicks **Buy credits** in the menu:

1. Resolve a `purchaseURL` — prefer `dashboardSnapshot.creditsPurchaseURL` (scraped from the page), fall back to `https://chatgpt.com/codex/cloud/settings/usage`.
2. Open a 980×760 window (capped to 92%/88% of the visible screen), titled **"Buy Credits"**, with a fresh `WKWebView` whose `websiteDataStore` is the **same per-account store** used for scraping (so the session is already logged in).
3. Inject a debug log handler `codexbarLog` (forwarded to `~/Library/Caches/.../codexbar-buy-credits.log`).
4. After `didFinish` navigation, if `autoStartPurchase` was set, inject `autoStartScript` — a self-contained JS payload that:
   - Walks the DOM (incl. shadow roots + same-origin iframes) for buttons matching `(credit AND (buy|add|purchase|top up))` or "Add more". Clicks the best candidate.
   - Then polls every 500 ms (max 90 attempts) for a **dialog "Next"** button and auto-advances through the purchase modal.
   - Falls back through `el.dispatchEvent(MouseEvent("click"))`, `pointer` event sequence, `requestSubmit()` on the containing `<form>`, and a synthetic click at the element's center.
5. Window cleanup goes through `WebKitTeardown.scheduleCleanup` — stop loads, clear delegates, defer release on Intel Macs to avoid WebKit autorelease crashes.

**Windows mapping:** Tauri WebView window (a separate WebView2 window with the per-account user-data folder pointed at the same cookie jar dir as the scraper). The auto-click JS is portable — keep it verbatim. WebView2 has no WebKit teardown crash class, so cleanup is trivial: drop the window.

---

## 9. Settings keys (every Codex-specific key)

Stored under per-provider config in `<AppSupport>/CodexBar/settings.json` (or the equivalent app-group). Codex's keys, scoped under `providers.codex`:

| Key                              | Type                                        | Default      | Surface                                                                                  |
| -------------------------------- | ------------------------------------------- | ------------ | ---------------------------------------------------------------------------------------- |
| `source`                         | `auto` / `oauth` / `cli` (saved as `ProviderSourceMode`) | `auto`       | Preferences → Providers → Codex → Usage source                                           |
| `cookieSource`                   | `off` / `auto` / `manual`                   | `off`        | Preferences → Providers → Codex → OpenAI cookies                                         |
| `cookieHeader`                   | `String` (secret)                           | `""`         | Manual paste; only used when cookieSource = manual                                       |
| `codexActiveSource`              | `liveSystem` / `managedAccount(UUID)`       | `liveSystem` | Menu → System Account submenu                                                            |

Global keys that affect Codex:

| Key                              | Type     | Default | Effect                                                                                       |
| -------------------------------- | -------- | ------- | -------------------------------------------------------------------------------------------- |
| `openAIWebAccessEnabled`         | `bool`   | `false` | Toggle: enable OpenAI dashboard scraping                                                     |
| `openAIWebBatterySaverEnabled`   | `bool`   | `false` | When on, reduce background refresh frequency for the web path (manual refreshes still run)   |
| `historicalTrackingEnabled`      | `bool`   | `true`  | Persist plan-utilization history locally                                                     |
| `showOptionalCreditsAndExtraUsage` | `bool` | `true`  | Show credits remaining + extra usage rows in the menu                                        |
| `debugDisableKeychainAccess`     | `bool`   | `false` | If on, the cookie-header cache cannot read/write Keychain → degrades cookie reuse            |

Managed-account storage (separate file): `managed-codex-accounts.json` (catalog) + the per-home `auth.json` files (per §7.1).

Cookie cache (separate, Keychain/Credential Manager-backed): `com.steipete.codexbar.cache` service, account `cookie.codex` (or scoped variant for managed accounts).

---

## 10. Models surfaced to the menu

The composite menu model assembled per refresh:

```
ProviderCardModel(Codex) {
  primary:     RateWindow?,             // 5h session
  secondary:   RateWindow?,             // weekly
  tertiary:    RateWindow?,             // sometimes weekly_sonnet / code_review
  credits:     CreditsSnapshot?,        // { remaining, events[], updatedAt }
  monthly_cost: MoneySnapshot?,         // extra usage
  account:     ProviderIdentitySnapshot,
  dashboardExtras: OpenAIDashboardSnapshot? {
      codeReviewRemainingPercent,
      codeReviewLimit,
      usageBreakdown,
      creditEvents,
      dailyBreakdown,
      creditsPurchaseURL,
      accountPlan,
  }
}
```

Rate-window display rules (`UsageFormatter`):

- `usedPercent`: int 0..=100, clamped.
- Reset string: relative ("Resets in 2 h"), or absolute ("Resets Mon 9 PM") when more than 24 h out.
- Hide a window entirely if `usedPercent == 0` and `resetsAt == nil`.

Charts:

- **Credits-history chart** (`CreditsHistoryChartMenuView`): line chart from `creditEvents` (date, creditsUsed).
- **Cost-history chart** (`CostHistoryChartMenuView`): aggregated from `daily_breakdown`.
- **Plan-utilization history** (`PlanUtilizationHistoryChartMenuView`): sparkline of `usedPercent` over the last 8 weeks per window (sampled at each refresh that produced new data).

---

## 11. Multi-account / multi-token

| Concept                                | Codex behavior                                                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Stacked menu vs switcher               | Configurable in Preferences → Advanced → Display. Codex caps at **6** visible accounts (UI hard cap).         |
| Where accounts come from               | (a) the live system account, (b) every managed account in the catalog. They are deduped by canonical identity. |
| Per-account fetch order                | Each account uses its own `CODEX_HOME` for OAuth + CLI; web extras use its own per-email WKWebsiteDataStore. |
| Promotion semantics                    | Any managed account can be promoted (§6); the live system account is always shown but "promote to itself" is the converged no-op. |
| Concurrent refreshes                   | Serialized per-account at the runtime layer; cross-account refreshes can happen in parallel (separate jars).  |
| Selecting a row in the menu            | Sets `codexActiveSource = managedAccount(id)` (or `liveSystem`) and kicks `refreshCodexAccountScopedState`. No file mutation unless the user explicitly clicks **System Account → row**. |

Manual cookie tokens are **not** how multi-account Codex works (unlike Claude). Codex multi-account is OAuth-only because the source of truth is `auth.json`.

---

## 12. Errors, retries, dim states

| Path             | Failure                                | Reaction                                                                                                          |
| ---------------- | -------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| OAuth API        | 401/403, `notFound`, refresh `expired/revoked/reused` | Strategy returns error. If `auto` mode, fall back to CLI. Otherwise surface user-facing.                          |
| OAuth API        | `decodeFailed`, `invalidResponse`, `serverError`, `networkError`, refresh `networkError` / `invalidResponse` | **Do not** fall back. Surface error, keep last-known snapshot (dim icon).                                          |
| CLI RPC          | Any app-server error                   | Terminal for the CLI strategy unless the error body contains a `wham/usage` JSON blob (then parse and use it).      |
| CLI RPC          | Timeout                                | Kill child. Return timeout error. No automatic retry within the strategy (next tick retries).                      |
| Web dashboard    | `loginRequired`                        | Clear the per-account cookie store **only** if cached header verification failed; otherwise surface "refresh OpenAI cookies and try again". |
| Web dashboard    | `cloudflareInterstitial`               | Surface user-facing. Possibly trigger an in-app WebView2 window so the user can solve the challenge.               |
| Web dashboard    | `noMatchingAccount(found)`             | Surface with found-account list. Cache mismatched cookies under the *found* email for potential later reuse.       |
| Credits refresh  | "data not available yet"               | Keep cached credits, friendly message: "Codex credits are still loading; will retry shortly."                       |
| Credits refresh  | network/api                            | Show cached value + error pill: "Last Codex credits refresh failed: <msg>. Cached values from <stamp>."             |
| Promotion        | any `CodexAccountPromotionError`       | Modal alert with mapped message. No file mutation has occurred. UI re-enables submenu.                              |
| Managed login    | `loginFailed`, `missingEmail`, `workspaceSelectionCancelled` | Clean up the partially created managed home (only if it's inside the sandbox).                                     |

**Dim states:** when the last refresh failed but a previous successful snapshot exists, the menu shows the value at 60% opacity with a small "stale" badge and the last-success timestamp.

Retry cadence:

- App background refresh: ~30 s (foreground) / ~5 min (battery saver).
- Hard retries: only on user action ("Refresh now") or when the user toggles `codexCookieSource` / re-pastes cookies.

---

## 13. Mac → Windows mapping (cheat sheet)

| Concern                          | macOS                                                  | Windows (Tauri 2)                                                                                                                                  |
| -------------------------------- | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| OAuth refresh storage            | Plaintext `~/.codex/auth.json`, 0600                   | `%USERPROFILE%\.codex\auth.json` (Codex CLI owns this), restrictive ACL via `windows-acl` or `icacls`. Optionally also keep an encrypted DPAPI copy in `%LOCALAPPDATA%\CodexBar\codex-oauth.bin` for our managed accounts. |
| Per-account managed home         | `~/Library/Application Support/CodexBar/managed-codex-homes/<UUID>` | `%LOCALAPPDATA%\CodexBar\managed-codex-homes\<UUID>`                                                                                                |
| Managed accounts catalog         | `…/managed-codex-accounts.json`, 0600                  | `%LOCALAPPDATA%\CodexBar\managed-codex-accounts.json`, owner-only ACL                                                                              |
| Cookie cache (header)            | Keychain `com.steipete.codexbar.cache`                 | Credential Manager via `keyring` crate, service `CodexBar.cache`, target `cookie.codex` or `cookie.codex.account-<uuid>`                          |
| Per-account WebView jar          | `WKWebsiteDataStore(forIdentifier: uuid)`              | `reqwest::cookie::Jar` (serialized) **and/or** WebView2 user-data dir at `%LOCALAPPDATA%\CodexBar\webview2\<email-uuid>`                           |
| Browser cookie sources           | Safari binarycookies, Chromium SQLite + Chrome Safe Storage (Keychain), Firefox sqlite | Chromium SQLite at `%LOCALAPPDATA%\<Vendor>\<Product>\User Data\<Profile>\Network\Cookies` + DPAPI v10 decryption (`os_crypt.encrypted_key` from `Local State`), Firefox `%APPDATA%\Mozilla\Firefox\Profiles\*\cookies.sqlite` |
| Headless / off-screen WebView    | `WKWebView` (1×1 px sliver, alpha 0.001)               | **Prefer `reqwest`** for the API preflight and for any pure JSON endpoint. **WebView2** only when JS hydration is unavoidable (`code_review`, `usage_breakdown`, credits-history table). Run a minimized, off-screen Tauri window with a `<WebView2>` and execute the same JS payload via `eval()`. |
| Interactive login window         | `WKWebView` window (Cursor flow, login flows)          | Tauri WebView2 window pointed at `https://chatgpt.com/...`. Cookies persist via the user-data folder.                                              |
| CLI binary lookup                | `BinaryLocator.resolveCodexBinary` (PATH + bundled fallback) | Search `%PATH%`, then `%LOCALAPPDATA%\Programs\codex\codex.exe`, then `%USERPROFILE%\.bun\bin\codex.exe` (Bun install).                            |
| CLI subprocess (RPC)             | `Process` + stdio pipes                                | `tokio::process::Command` + `Stdio::piped()`. Group kill via `JobObject` (`windows::Win32::System::JobObjects`) or `taskkill /T /F /PID`.          |
| CLI subprocess (PTY `/status`)   | `openpty()` POSIX                                      | `portable_pty::native_pty_system()` (ConPTY on Win10+). Same JS-free output parser.                                                                |
| Atomic file write                | `Data.write(.atomic)` (same volume)                    | `tokio::fs::write` to `*.tmp`, then `tokio::fs::rename` (atomic on the same volume).                                                               |
| File permissions (0600)          | `chmod 0600`                                           | Restrict ACL to current user via `SetNamedSecurityInfo` or `icacls "%path%" /inheritance:r /grant:r "%USERNAME%:F"`.                              |
| Process group / timeout kill     | `setpgid` + `kill(-pgid, SIGTERM/SIGKILL)`             | Assign to `JobObject` with `KILL_ON_JOB_CLOSE`; close the job to kill the tree.                                                                    |
| Notifications                    | `NSUserNotificationCenter` / `UserNotifications`       | Tauri notification API or Win32 toast via `windows-rs`.                                                                                            |
| Status item / menu               | `NSStatusItem` + `NSMenu`                              | Tauri system tray + `tauri-plugin-positioner` for menu placement.                                                                                  |
| Keychain prompt policy           | Implicit; Mac may pop "allow access"                   | Credential Manager has no equivalent prompt — no policy toggle needed, but keep settings UI for cross-platform consistency.                       |
| App group support                | shared container for widget extension                  | n/a yet (no Windows widget). Use a single user-scoped directory.                                                                                   |

### 13.1 Specifics worth calling out for the Rust implementer

- The OAuth strategy is the *only* place we mutate `~/.codex/auth.json` for refresh. Use a **per-process mutex** around read-modify-write; rely on the atomic rename for cross-process safety. (The Codex CLI itself also writes this file during `codex login`; collisions are extremely unlikely but the rename pattern keeps us safe.)
- Number parsing in the dashboard must handle thin-space groupings (`U+202F`, `U+00A0`) and the "crédit" → comma-decimal heuristic. Lift these to a shared `text_parsing.rs` and unit-test the existing fixtures.
- The dashboard scrape JS is **one** file (`openAIDashboardScrapeScript`). Ship it as a string constant. Don't rewrite it in Rust.
- Identity normalization happens at three layers (read, store, match). Centralize in one `CodexIdentity::normalize()` to avoid divergence — every comparison flows through `CodexIdentityMatcher::matches`.

---

## 14. Acceptance checklist

A Windows implementer can declare the Codex provider "done" when **all** of these pass.

### 14.1 OAuth API

- [ ] Reads `%CODEX_HOME%\auth.json` if set, else `%USERPROFILE%\.codex\auth.json`.
- [ ] Accepts both snake_case (`access_token`) and camelCase (`accessToken`) field names; writes snake_case.
- [ ] `OPENAI_API_KEY`-only files are loaded as a degraded read-only mode (no refresh).
- [ ] Refresh fires when `now - last_refresh > 8 days`, POSTs the exact body, parses error codes (`refresh_token_expired/reused/invalid_grant/refresh_token_invalidated`).
- [ ] On successful refresh: writes the new triple + `last_refresh = now ISO-8601` atomically with restrictive ACL.
- [ ] Usage call sends `Authorization: Bearer`, `ChatGPT-Account-Id`, `User-Agent: CodexBar`, `Accept-Language: en-US,en;q=0.9`.
- [ ] Tolerant decode: unknown `plan_type` becomes `Unknown(String)`; primary window decode failure does not nuke secondary or credits.
- [ ] Partial results (credits-only) are returned, not escalated to CLI fallback.
- [ ] `should_fallback` predicate matches the exact macOS truth table (§3.2 + §2.3).

### 14.2 CLI RPC

- [ ] Spawns `codex -s read-only -a untrusted app-server` with `stdin/stdout` piped (no PTY).
- [ ] Calls `initialize`, `account/read`, `account/rateLimits/read` in that order, framed as line-delimited JSON-RPC.
- [ ] On `initialize` timeout (long budget) or method timeout (short budget), child is killed and reader unwinds.
- [ ] App-server errors are terminal for the strategy, except when the error body contains a parsable `wham/usage` JSON object.

### 14.3 CLI PTY `/status` diagnostic

- [ ] Uses ConPTY (Windows) / openpty (Mac); not used in automatic refresh.
- [ ] Auto-dismisses the "Update available!" prompt; surfaces "CLI update needed" on detection.
- [ ] Parses `Credits:`, `5h limit (...%)`, `Weekly limit (...%)` reliably; locale-tolerant numbers.

### 14.4 Web dashboard extras

- [ ] Off by default; gated by `openAIWebAccessEnabled`.
- [ ] Cookie source `auto`: tries Chromium browsers (with DPAPI v10 key decryption), then Firefox. Source order is configurable per `ProviderBrowserCookieDefaults`.
- [ ] Cookie source `manual`: accepts a `Cookie:` header, normalizes (`CookieHeaderNormalizer`), validates against `/backend-api/me`.
- [ ] Per-account isolated cookie jars persisted to disk; clearing one account's jar never touches another's.
- [ ] Cookie-header cache in Credential Manager: load before browser scan, clear on `manualCookieHeaderInvalid` / `noMatchingAccount` / `dashboardStillRequiresLogin`.
- [ ] Scrape returns: code_review %, code_review limit window, credits remaining, primary+secondary rate limits, usage breakdown (Recharts), credit events (table rows), credits purchase URL, signed-in email, account plan.
- [ ] Filters `skillusage:*` services out of the usage breakdown.
- [ ] Surfaces `loginRequired` and Cloudflare interstitial distinctly.
- [ ] When `auto` cookie source succeeds, the dashboard snapshot is merged into the OAuth/CLI snapshot — never replaces canonical windows except as a degraded fallback.

### 14.5 Account promotion (the trickiest)

- [ ] Submenu lists all managed + live accounts, with checkmark on the resolved live one, max 6.
- [ ] Promotion is blocked when another managed-account add/remove or another promotion is in-flight; UI shows the "finish current change" alert.
- [ ] Converged no-op detection: target identity matches live identity → just writes `codexActiveSource`, no file mutation, no toast spam.
- [ ] Planning produces exactly one of `none / reject(reason) / importNew / refreshExisting(destination) / repairExisting(destination)`; planner is **pure** (filesystem reads happen only in preparation).
- [ ] `apiKeyOnly` live → rejected with the API-key-only message; live `auth.json` is **never** touched.
- [ ] Conflict: another readable managed home shares the live account_id but disagrees → rejected with the duplicate-account message.
- [ ] `importNew`: creates `<managed-homes>/<UUID>/auth.json` (0600 / owner-only ACL), commits the catalog, then writes live `auth.json` atomically via staged file + rename. Failure rolls back the freshly-created home (only inside the sandbox).
- [ ] `refreshExisting` / `repairExisting`: writes auth into the **existing** managed home, validates the home is inside the sandbox before writing, updates catalog (preserves `id` + `createdAt`).
- [ ] All `CodexAccountPromotionError` variants surface exactly the strings in §6.10.
- [ ] After success: `codexActiveSource = liveSystem`, scoped refresh fires with `allowDisabled = true`, the menu and the system-account submenu reflect the new state within one refresh tick.

### 14.6 Managed accounts (separate from promotion)

- [ ] Adding a managed account creates a fresh UUID home, runs `codex login` with `CODEX_HOME` pointed there, captures the resulting `auth.json` + JWT identity.
- [ ] Workspace picker appears if `>1` workspace; cancel → `workspaceSelectionCancelled`; accept → persists workspaceID into `auth.json.tokens.account_id`.
- [ ] Removing a managed account validates the path is inside the sandbox before `remove_dir_all`; never deletes outside.
- [ ] Catalog: v1 → v2 migration hydrates `providerAccountID` from the local JWT once; never errors silently on missing JWTs.

### 14.7 History ownership

- [ ] History keys map exactly: `providerAccount → codex:v1:provider-account:<id>`; `emailOnly → codex:v1:email-hash:<sha256>`.
- [ ] Legacy email-hash keys continue to load when the current target has a matching canonical email-hash key.
- [ ] `hasAdjacentMultiAccountVeto` set when active managed and live diverge → strict-only continuity.

### 14.8 Buy Credits window

- [ ] Opens a per-account WebView2 window pointed at the resolved purchaseURL.
- [ ] Auto-start JS runs after `didFinish`, clicks the "Buy credits"/"Add more" button, then auto-advances through the dialog "Next" button.
- [ ] Closing the window drops the WebView cleanly; no memory leak or stuck cookie state.

### 14.9 Settings, observability, UX polish

- [ ] All settings keys in §9 are persisted, observable, and re-emit a snapshot on change.
- [ ] Source label in the menu shows `oauth + openai-web` (or `codex-cli + openai-web`) when web extras succeeded; just `oauth`/`codex-cli` otherwise.
- [ ] Toggling `openAIWebAccessEnabled` off immediately suppresses web extras and clears `openai-web` from the label on the next tick.
- [ ] Battery saver: when on, suppress background web refreshes; an explicit "Refresh now" still fires them.
- [ ] All error strings rendered in Settings → Providers → Codex use `CodexUIErrorMapper.userFacingMessage` mapping (so "token_expired" never reaches the user verbatim).
- [ ] Tray icon dims by 40% when the last refresh failed and a previous snapshot is shown.

### 14.10 Safety / sandbox invariants

- [ ] Every managed-home delete or write goes through `ManagedCodexHomeFactory.validateManagedHomeForDeletion` (or its Windows analog); deletions outside the sandbox are unreachable.
- [ ] Live `auth.json` writes are atomic (staged + rename); no half-written file is ever observed by the Codex CLI.
- [ ] Credentials never leave the machine: no telemetry, no logs containing the access_token or cookie values. `LogRedactor.redact` style redaction on every log line that may contain a secret.

---

## 15. Code-shape sketches (for orientation only)

### 15.1 Shared crate: identity + refresh predicate

```rust
pub enum CodexIdentity { ProviderAccount(String), EmailOnly(String), Unresolved }

impl CodexIdentity {
    pub fn from_jwt(id_token: &str, account_id_field: Option<&str>) -> Self { /* … */ }
}

pub fn should_fallback_oauth(err: &CodexOAuthError, mode: SourceMode) -> bool {
    if mode != SourceMode::Auto { return false; }
    matches!(err,
        CodexOAuthError::Unauthorized
        | CodexOAuthError::CredentialsNotFound
        | CodexOAuthError::CredentialsMissingTokens
        | CodexOAuthError::RefreshExpired
        | CodexOAuthError::RefreshRevoked
        | CodexOAuthError::RefreshReused)
}
```

### 15.2 Shared crate: refresh call

```rust
pub async fn refresh(creds: CodexOAuthCredentials) -> Result<CodexOAuthCredentials, RefreshError> {
    if creds.refresh_token.is_empty() { return Ok(creds); }
    let body = json!({
        "client_id":     "app_EMoamEEZ73f0CkXaXp7hrann",
        "grant_type":    "refresh_token",
        "refresh_token": creds.refresh_token,
        "scope":         "openid profile email",
    });
    let res = HTTP.post("https://auth.openai.com/oauth/token").json(&body).send().await?;
    if res.status() == 401 { return Err(map_401(res).await); }
    let v: serde_json::Value = res.json().await?;
    Ok(creds.merged_from(&v))
}
```

### 15.3 Shared crate: promotion service (skeleton)

```rust
pub async fn promote(target_id: Uuid, ctx: PromotionDeps) -> Result<PromotionResult, PromotionError> {
    let prepared = PreparedContextBuilder::new(&ctx).build(target_id).await?;
    if let Some(src) = converged_active_source(&prepared) {
        ctx.settings.write_active_source(src);
        ctx.usage.refresh_scoped(true).await; return Ok(PromotionResult::no_op(src));
    }
    let target_bytes = required_target_bytes(&prepared.target)?;
    let plan = PreservationPlanner::plan(&prepared);
    let exec = PreservationExecutor::new(&ctx).execute(plan, &prepared)?;
    ctx.live_auth_swapper.swap(target_bytes, &prepared.live.home_url)?;
    ctx.settings.write_active_source(CodexActiveSource::LiveSystem);
    ctx.usage.refresh_scoped(true).await;
    Ok(PromotionResult::promoted(target_id, exec.disposition))
}
```

### 15.4 Tauri command surfaces

| Command (JS → Rust)                                                  | Effect                                                                          |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `codex_promote_managed_account({ id })`                              | Calls `promote(id)`, streams progress events back via `event::emit("codex/promotion/state", …)` |
| `codex_authenticate_managed_account({ existing_id?, timeout_ms })`   | Spawns `codex login` in a managed home; emits progress + final account row     |
| `codex_remove_managed_account({ id })`                               | Sandbox-validated removal                                                       |
| `codex_set_active_source({ source })`                                | Writes `codexActiveSource`; runs scoped refresh                                 |
| `codex_set_cookie_source({ mode })`                                  | Switches off/auto/manual; on switch to off, clears cached cookie header         |
| `codex_set_cookie_header({ header })`                                | Stores manual cookie header; revalidates on next refresh                        |
| `codex_open_buy_credits_window({ url, account_email, auto_start })`  | Opens the WebView2 window (§8)                                                  |
| `codex_force_web_refresh({ account_email })`                         | Forces a fresh dashboard scrape (bypasses battery saver)                        |

### 15.5 Key paths summary

| Concern                          | Path (Windows)                                                                                 |
| -------------------------------- | ---------------------------------------------------------------------------------------------- |
| Live auth.json                   | `%USERPROFILE%\.codex\auth.json` (or `%CODEX_HOME%\auth.json`)                                |
| Managed homes                    | `%LOCALAPPDATA%\CodexBar\managed-codex-homes\<UUID>\auth.json`                                |
| Catalog                          | `%LOCALAPPDATA%\CodexBar\managed-codex-accounts.json`                                          |
| WebView2 jars                    | `%LOCALAPPDATA%\CodexBar\webview2\<email-uuid>\`                                               |
| Cookie cache (Credential Mgr)    | `CodexBar.cache` / `cookie.codex` (default), `cookie.codex.account-<uuid>` (scoped)            |
| Settings                         | `%LOCALAPPDATA%\CodexBar\settings.json`                                                        |
| Cost-usage cache (Codex local)   | `%LOCALAPPDATA%\CodexBar\cost-usage\codex-v2.json`                                             |
| Debug buy-credits log            | `%TEMP%\codexbar-buy-credits.log`                                                              |

---

## 16. Test fixtures to port (high-value)

When wiring the Rust crate, port the existing macOS fixtures:

- `Tests/.../Codex/Fixtures/openai-dashboard-*.html` — full HTML snapshots of the usage page (logged-in, logged-out, workspace-picker, Cloudflare, multi-workspace).
- `Tests/.../Codex/Fixtures/auth.json` variants — OAuth-only, API-key-only, missing-id-token, expired-last-refresh.
- `Tests/.../Codex/Fixtures/wham-usage-*.json` — full + partial-window + unknown-plan responses.
- `Tests/.../Codex/Fixtures/credits-history-*.html` — virtualized table, localized number formats (US, EU, FR with crédit).
- `Tests/.../CodexPromotion/Cases/*.yaml` — preplanner cases (each yaml maps a `PreparedPromotionContext` shape to the expected `Plan` variant). Replay them against the Rust planner; they're the safety net.

A passing test suite using these fixtures, plus the §14 acceptance checklist, is sufficient evidence that the Windows refactor preserves behavior.
