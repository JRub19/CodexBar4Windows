# Tier-1 Providers Spec — Cursor, Copilot, Gemini, Vertex AI, Factory (Droid), OpenRouter

> Implementation blueprint for the Windows refactor (Tauri 2 + React + shared Rust crate).
> Sources: `Sources/CodexBarCore/Providers/<Name>/`, `Sources/CodexBar/Providers/<Name>/`,
> `Sources/CodexBar/<Name>LoginRunner.swift`, and `docs/<name>.md`.
> Behavior, contracts, and pipelines — no Swift code. Target quality: Phantom / Duolingo.

The Windows refactor centralizes provider work behind one Rust trait so React only sees a normalized
`UsageSnapshot`. Each Tier-1 provider has its own auth & fetch pipeline, but they share several
cross-cutting subsystems: browser-cookie extraction, OAuth token refresh, JWT claim parsing, and
manual-cookie-header pasting. The "Mac → Windows mapping" subsection of every provider names the
concrete Windows-native replacement.

The common output type every fetch returns is:

```text
UsageSnapshot {
  primary:   RateWindow?     // headline meter
  secondary: RateWindow?     // second bar
  tertiary:  RateWindow?     // third bar (only some providers)
  providerCost: ProviderCostSnapshot?    // dollar / cents spend
  extraRateWindows: [NamedRateWindow]?   // optional named lanes
  cursorRequests: CursorRequestUsage?    // legacy request plans
  openRouterUsage: OpenRouterUsageSnapshot?
  identity:  ProviderIdentitySnapshot?   // email/org/plan label
  updatedAt: timestamp
}
```

A `RateWindow` is `{ usedPercent, windowMinutes?, resetsAt?, resetDescription? }`. Windows are tagged
by length in minutes (5h = 300, 24h = 1440, 7-day = 10 080, monthly = nil).

---

## 1. Cursor — provider ID `cursor`

> One-line: web-only browser-cookie probe against `cursor.com/api/usage-summary`. Token-/cents-based
> plan with a separate legacy request-based plan and a team-pool fallback for enterprise members.

### 1.1 Auth source order

Fetch tries these in order, stopping at the first one that returns 200 from `/api/usage-summary`:

| Order | Source | Storage | Notes |
|-------|--------|---------|-------|
| 1 | Manual cookie header | `~/.codexbar/config.json` → `providers.cursor.cookieHeader` | User pastes a `Cookie:` header; takes precedence when `cookieSource == .manual`. |
| 2 | Cached cookie header | Keychain `com.steipete.codexbar.cache`, account `cookie.cursor` | Reused after any successful auto-import. Cleared on 401/403. |
| 3 | Browser cookies, strict names | OS cookie stores | Iterate Safari → Chrome → Firefox; require **named** session cookie. |
| 4 | Browser cookies, any domain | OS cookie stores | Same iteration; accept any cookie on the cursor domain and let the API validate. |
| 5 | Stored WebKit session | `~/Library/Application Support/CodexBar/cursor-session.json` | Captured by in-app login flow. |

Browser order resolves through `ProviderDefaults.metadata[.cursor].browserCookieOrder` and is filtered
to the installed set so we never prompt the keychain for a browser the user does not have.

### 1.2 Session cookie names (any one counts)

```
WorkosCursorSessionToken
__Secure-next-auth.session-token
next-auth.session-token
wos-session
__Secure-wos-session
authjs.session-token
__Secure-authjs.session-token
```

### 1.3 Cookie domains scanned

`cursor.com`, `www.cursor.com`, `cursor.sh`, `authenticator.cursor.sh`.

### 1.4 Endpoints

| Method | URL | Purpose | Required headers |
|--------|-----|---------|------------------|
| GET | `https://cursor.com/api/usage-summary` | Plan + on-demand usage, billing cycle | `Accept: application/json`, `Cookie:` |
| GET | `https://cursor.com/api/auth/me` | Email, name, `sub` | `Accept: application/json`, `Cookie:` |
| GET | `https://cursor.com/api/usage?user=<sub>` | Legacy request-based usage (gpt-4) | `Accept: application/json`, `Cookie:` |

Status handling: `401/403` → `notLoggedIn` (cached cookie cleared), `200` → decode, anything else →
`networkError(HTTP nnn)`. Timeout default 15 s.

### 1.5 Response → snapshot mapping

`/api/usage-summary` returns these blocks (all monetary values **in cents**):

```text
billingCycleStart, billingCycleEnd, membershipType, limitType, isUnlimited
individualUsage.plan { enabled, used, limit, remaining, breakdown{included,bonus,total},
                      autoPercentUsed, apiPercentUsed, totalPercentUsed }
individualUsage.onDemand { enabled, used, limit, remaining }
individualUsage.overall  { enabled, used, limit, remaining }   // enterprise personal cap
teamUsage.onDemand       { ... }
teamUsage.pooled         { enabled, used, limit, remaining }   // shared team pool
```

Headline percent precedence (the loadbearing detail):

1. `plan.totalPercentUsed` if present (already a percentage, clamped 0–100)
2. mean of `plan.autoPercentUsed` and `plan.apiPercentUsed`
3. either of the lane percents alone
4. `plan.used / plan.limit` (cents)
5. `overall.used / overall.limit` (Enterprise / Team personal cap)
6. `teamUsage.pooled.used / pooled.limit`

USD figures follow the same source: whichever block produced the headline is divided by 100.
On-demand is its own block, surfaced as `ProviderCostSnapshot` (period: `Monthly`).

| Snapshot field | From |
|----------------|------|
| `primary.usedPercent` | headline percent (above) — or legacy `requests.used/requests.limit` when present |
| `secondary` | `plan.autoPercentUsed` (Auto + Composer) |
| `tertiary` | `plan.apiPercentUsed` (named-model / API) |
| `primary.resetsAt` | `billingCycleEnd` (ISO-8601, with fractional seconds) |
| `primary.resetDescription` | `"Resets MMM d at h:mma"` (en_US_POSIX) |
| `providerCost.used` / `.limit` | `onDemand.used / .limit` in USD |
| `cursorRequests` | from `/api/usage` → `gpt-4.numRequestsTotal` and `.maxRequestUsage` |
| `identity.accountEmail` | `/api/auth/me .email` |
| `identity.loginMethod` | `"Cursor "+membershipType.capitalized` (special-cases: Enterprise, Pro, Hobby, Team) |

`plan.autoPercentUsed` and `apiPercentUsed` are *already* percent units even if they are fractional
(`0.36` means 0.36%, not 36%) — do **not** multiply by 100.

### 1.6 Settings keys (`providers.cursor.*` in config.json)

| Key | Type | Default | Validation |
|-----|------|---------|------------|
| `cookieSource` | `auto` \| `manual` \| `off` | `auto` | enum |
| `cookieHeader` | string | `""` | strip surrounding quotes, normalize whitespace |
| `tokenAccounts` | array of `{ id, label, token, externalIdentifier? }` | `[]` | token = cookie header for the cursor token-account type |

The manual cookie header overrides everything else. Setting `cookieSource: off` disables the strategy.

### 1.7 Login flow (in-app browser)

1. User clicks "Add account" in CodexBar menu → spawn a small browser window pointed at
   `https://cursor.com/dashboard`.
2. If unauthenticated, Cursor redirects through `authenticator.cursor.sh` — emit phase
   `waitingLogin`. The webview uses a **non-persistent data store** so we control cookie capture.
3. On final landing at `cursor.com/dashboard` (server redirect or `didFinish`), wait ~500 ms for
   cookies to settle, then read all cookies from the webview's data store and filter to
   `cursor.com` / `cursor.sh` domains.
4. Persist filtered cookies to `cursor-session.json`. Run a single probe to grab the email for the
   account label.
5. On failure (`no session cookies found`, navigation error, user close) emit a failure phase.

### 1.8 Edge cases

- **Stale cookies from one browser, fresh from another.** Each browser pass tries `fetchWithCookieHeader`
  and only moves on when the API returns `notLoggedIn`. Network/parse errors do *not* fall through —
  they bubble up so we don't mask real outages.
- **Domain-cookie fallback.** A second pass accepts cookies on the domain even when none have the
  known session names — covers Cursor renames and host-only cookies.
- **Cached header invalidation.** Only `notLoggedIn` clears the keychain cache; transient errors
  preserve it.
- **Legacy request plan detection.** `/api/usage.gpt-4.maxRequestUsage != nil` → use
  `numRequestsTotal / maxRequestUsage` for the primary instead of the percent path.
- **Multi-account.** `tokenAccounts` entries inject a cookie header per-account; the selector lives
  in `TokenAccountSupportCatalog` and stays out of the fetch path.
- **Team plan detection.** When neither `plan` nor `overall` returns numbers, fall through to
  `teamUsage.pooled` so enterprise pool members still see a meter.

### 1.9 Mac → Windows mapping

| Mac concept | Windows equivalent |
|-------------|--------------------|
| Safari Cookies.binarycookies | n/a — skip; surface a tip if Edge is detected so we use Edge instead |
| `~/Library/Application Support/Google/Chrome/*/Cookies` | `%LOCALAPPDATA%\Google\Chrome\User Data\<Profile>\Network\Cookies` (SQLite, encrypted with DPAPI v10 keyed by Local State `os_crypt.encrypted_key`) — see auth doc for the shared decoder |
| Firefox cookies.sqlite | `%APPDATA%\Mozilla\Firefox\Profiles\*.default*\cookies.sqlite` (no encryption) |
| Edge / Brave / Vivaldi / Arc | Same Chromium layout under `%LOCALAPPDATA%\Microsoft\Edge`, `BraveSoftware\Brave-Browser`, `Vivaldi\User Data`, `Arc\User Data` |
| WebView login window | Tauri WebView2 child window pointed at `cursor.com/dashboard`; capture cookies via `webview.cookies()` filtered to `*.cursor.com`/`*.cursor.sh` |
| `~/Library/Application Support/CodexBar/cursor-session.json` | `%APPDATA%\CodexBar\cursor-session.json` (same JSON shape) |
| Keychain cookie cache | DPAPI-protected blob in `%APPDATA%\CodexBar\cache\cookie.cursor.bin` |

### 1.10 Acceptance checklist

- [ ] Importing cookies from Edge, Chrome (Default + named profiles), Brave, Arc, and Firefox works.
- [ ] Manual cookie paste accepts both `name=value; name=value` and full `Cookie:` headers.
- [ ] Headline percent matches Cursor's "Total" tile within ±1% for Pro, Hobby, Team, and Enterprise.
- [ ] Legacy request plan (gpt-4 only) shows request count `used / limit`.
- [ ] On-demand spend renders as USD with cents precision.
- [ ] Billing cycle reset string is locale-stable (`MMM d at h:mma`, en_US_POSIX).
- [ ] In-app browser login lands on `/dashboard` and persists cookies under Windows path.
- [ ] 401/403 clears keychain cache and falls through to the next source.
- [ ] Domain-only cookie pass succeeds when WorkOS rotates session-cookie names.

---

## 2. Copilot — provider ID `copilot`

> One-line: GitHub OAuth device flow + Copilot internal usage endpoint. No browser cookies. Optional
> GitHub Enterprise host. Token stored in `config.json` (plaintext in Mac; DPAPI on Windows).

### 2.1 Auth source order

| Order | Source | Notes |
|-------|--------|-------|
| 1 | `config.providers.copilot.apiKey` | Single-account; primary token. |
| 2 | `tokenAccounts` for `copilot` | Multi-account; each holds a GitHub OAuth token + `externalIdentifier: "github:user:<id>"`. |
| 3 | `COPILOT_API_TOKEN` env var | Resolved by `ProviderTokenResolver`. |

There is **no fallback** to a browser. The only way to acquire a token is the device flow.

### 2.2 Endpoints

| Method | URL | Purpose |
|--------|-----|---------|
| POST | `https://<host>/login/device/code` | Begin device flow. `<host>` = `github.com` or normalized enterprise host. |
| POST | `https://<host>/login/oauth/access_token` | Poll for token using `device_code`. |
| GET | `https://api.<host>/copilot_internal/user` | Usage. `api.` prefix added unless already present. |
| GET | `https://api.github.com/user` | Identity (account label, dedupe key). |

Required headers on every Copilot API call (load-bearing — GitHub treats this UA as a real Copilot
client):

```
Authorization: token <github_oauth_token>
Accept: application/json
Editor-Version: vscode/1.96.2
Editor-Plugin-Version: copilot-chat/0.26.7
User-Agent: GitHubCopilotChat/0.26.7
X-Github-Api-Version: 2025-04-01
```

Device-flow POSTs use `Content-Type: application/x-www-form-urlencoded`. Body keys:

| Endpoint | Body |
|----------|------|
| `/login/device/code` | `client_id=Iv1.b507a08c87ecfe98`, `scope=read:user` |
| `/login/oauth/access_token` | `client_id=Iv1.b507a08c87ecfe98`, `device_code=<code>`, `grant_type=urn:ietf:params:oauth:grant-type:device_code` |

The client ID `Iv1.b507a08c87ecfe98` is VS Code's official Copilot client.

### 2.3 Login flow (device-flow)

1. User clicks "Add Copilot account" → `POST /login/device/code` returns
   `device_code`, `user_code` (e.g. `ABCD-1234`), `verification_uri`, `expires_in`, `interval`.
2. Copy `user_code` to clipboard. Show an alert with the code + an "Open Browser" button.
3. Open `verification_uri` (or `verification_uri_complete` when present) in the default browser.
4. Show a "Waiting for authentication..." modal with a Cancel button.
5. Loop: sleep `interval` seconds → `POST /login/oauth/access_token`.
   - `authorization_pending` → keep waiting.
   - `slow_down` → add 5 s to interval, keep waiting.
   - `expired_token` → fail with timeout.
   - 200 → parse `access_token`.
6. With the token, call `GET /user` → label is `<login> (<plan>)` where `<plan>` is derived from the
   first Copilot usage probe (`copilotPlan` capitalized). If the identity call fails *and* accounts
   already exist, refuse to save the token (prevents anonymous duplicates on re-auth).
7. Match against existing accounts via `externalIdentifier = "github:user:<id>"`. Fall back to
   legacy `login`-based identifiers, then to username prefix.
8. Save / update the token account, enable the Copilot provider, show success modal.

### 2.4 Enterprise host normalization

Accept any of `octocorp.ghe.com`, `https://octocorp.ghe.com`, `https://octocorp.ghe.com/login`. Strip
scheme, drop everything after the first `/`, lowercase, trim leading/trailing dots. Empty → default
`github.com`. The API host is `api.<normalizedHost>` (or just `api.github.com` for default).

### 2.5 Response → snapshot mapping (`/copilot_internal/user`)

```text
quota_snapshots.premium_interactions { entitlement, remaining, percent_remaining, quota_id }
quota_snapshots.chat                  { ... }
copilot_plan: "individual" | "business" | "enterprise" | ...
assigned_date, quota_reset_date
// fallback shape for older accounts:
monthly_quotas { chat, completions }
limited_user_quotas { chat, completions }
```

The decoder accepts numbers as Int, Double, **or** strings — GitHub returns whichever. When
`percent_remaining` is missing but both `entitlement > 0` and `remaining` are present, derive it:
`percent = clamp(0, 100, remaining / entitlement * 100)`.

Snapshot mapping:

| Field | Source |
|-------|--------|
| `primary.usedPercent` | `100 - premium_interactions.percent_remaining` |
| `secondary.usedPercent` | `100 - chat.percent_remaining` |
| `resetsAt`, `windowMinutes` | none — Copilot does not return per-quota reset times |
| `identity.loginMethod` | `copilot_plan.capitalized` |
| `identity.accountEmail` | not exposed by this endpoint; pull from `/user` if needed for switcher labels |

If only the chat quota has data, leave `primary = nil` and put chat in `secondary` so the menu label
remains `"Premium"` / `"Chat"`. Placeholder snapshots (all zeros) are filtered out.

### 2.6 Settings keys (`providers.copilot.*`)

| Key | Type | Default | Validation |
|-----|------|---------|------------|
| `apiKey` | string | `""` | Trim, drop quotes; never logged unredacted |
| `enterpriseHost` | string | `""` | Normalized via the algorithm above |
| `tokenAccounts` | array | `[]` | Each `externalIdentifier` must match `github:user:<id>` |

### 2.7 Edge cases

- **`authorization_pending` vs `slow_down`.** Treat as continue; on slow_down, add 5 s to the
  sleep — not the configured interval (GitHub's recommendation).
- **`expired_token`.** Raise to UI as `URLError.timedOut`; the user has to restart the flow.
- **`401/403` on usage.** Throw `userAuthenticationRequired` so the menu shows a re-auth CTA.
- **Identity dedupe.** External identifier is the only stable key. Legacy accounts stored just the
  GitHub login — match those case-insensitively, then **rewrite** to the stable ID on update.
- **Enterprise host swap.** Changing the enterprise host invalidates existing tokens; surface a
  warning before saving.
- **No reset dates.** UI must not draw a reset countdown for Copilot.

### 2.8 Mac → Windows mapping

| Mac | Windows |
|-----|---------|
| Keychain `com.steipete.CodexBar` / `copilot-api-token` | DPAPI-protected blob in `%APPDATA%\CodexBar\secrets\copilot.tok` |
| `NSAlert` with Cancel | Tauri dialog with `cancellation_token` wired to the async poll task |
| `NSPasteboard.general` | Tauri clipboard plugin (`clipboard_manager`) |
| `NSWorkspace.shared.open(url)` | `tauri::api::shell::open` |
| `runModal` waiting alert | Native Tauri window with a spinner + Cancel button |

### 2.9 Acceptance checklist

- [ ] Device flow works against both `github.com` and a configured `*.ghe.com` host.
- [ ] User code is copied to clipboard before the browser opens.
- [ ] Polling tolerates `authorization_pending`, `slow_down`, `expired_token`.
- [ ] Cancel button cancels the polling task; no orphan tokens are saved.
- [ ] Token dedupe uses `github:user:<id>` and rewrites legacy `login` identifiers.
- [ ] `copilot_plan` flows through to the identity label.
- [ ] Headers exactly match the spec (VS Code spoofing) — any deviation triggers 401 from
      GitHub's bot detector.
- [ ] Chat-only accounts render `secondary` only (no broken primary).
- [ ] Snapshot decoder accepts Int, Double, and stringified numbers for entitlement/remaining.

---

## 3. Gemini — provider ID `gemini`

> One-line: borrow the Gemini CLI's OAuth credentials, refresh them, then call Google's private
> Cloud Code "Code Assist" quota API. Extract OAuth client_id/secret from a packaged JS file.

### 3.1 Auth source order

| Order | Source | Notes |
|-------|--------|-------|
| 1 | `~/.gemini/oauth_creds.json` | Written by `gemini` CLI. Required. |
| 2 | `~/.gemini/settings.json → security.auth.selectedType` | Gates the strategy: `oauth-personal` or unknown ⇒ proceed; `api-key` / `vertex-ai` ⇒ hard fail. |

There is **no** manual fallback; if the CLI is not installed and authed, the provider errors out.
The login flow drives the user back to the CLI.

### 3.2 Endpoints

| Method | URL | Body | Purpose |
|--------|-----|------|---------|
| POST | `https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota` | `{ "project": "<id>" }` or `{}` | Per-model quota buckets |
| POST | `https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist` | `{ "metadata": { "ideType": "GEMINI_CLI", "pluginType": "GEMINI" } }` | Tier + managed project |
| GET | `https://cloudresourcemanager.googleapis.com/v1/projects` | n/a | Discover the user's Gemini project |
| POST | `https://oauth2.googleapis.com/token` | `client_id`, `client_secret`, `refresh_token`, `grant_type=refresh_token` | Refresh access token |

All authenticated requests: `Authorization: Bearer <access_token>`,
`Content-Type: application/json` on POSTs.

### 3.3 OAuth credentials file format

`~/.gemini/oauth_creds.json`:

```json
{
  "access_token": "ya29...",
  "refresh_token": "1//0g...",
  "id_token": "<JWT with email, hd>",
  "expiry_date": 1731000000000
}
```

`expiry_date` is **milliseconds since epoch**; divide by 1000 before constructing a timestamp.
`refresh_token` is optional in the file but required to refresh — if missing and expired, raise
`notLoggedIn`.

### 3.4 OAuth client_id/secret extraction

Google does not publish these. We harvest them from the installed Gemini CLI's bundled JS.

1. Resolve the `gemini` binary via `PATH`, `BinaryLocator`, then `which gemini`.
2. Resolve symlinks (`realpath`) — important for Homebrew/npm shims.
3. Walk known layouts, checking `oauth2.js` files for the constants:
   - Homebrew nested: `<base>/libexec/lib/node_modules/@google/gemini-cli/node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js`
   - Bun/npm sibling: `<base>/../gemini-cli-core/dist/src/code_assist/oauth2.js`
   - Nix: `<base>/share/gemini-cli/node_modules/...`
   - npm nested: `<base>/node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js`
4. If none match, walk parent dirs (max 8 ascents) looking for `package.json` with
   `"name": "@google/gemini-cli"`.
5. If still not found, walk the `bundle/` directory of the package: start at `bundle/gemini.js`,
   follow `import` statements (regex on `(?:import|export)...['"](\./[^'"]+\.js)['"]`), then walk any
   remaining `.js` files in `bundle/`.
6. For `fnm`-managed installs, invoke `fnm exec --using <ver> npm root -g` and try
   `<root>/@google/gemini-cli/...`.
7. Regex-extract:
   ```
   (?:const|let|var)?\s*OAUTH_CLIENT_ID\s*=\s*['"]([\w\-\.]+)['"]\s*;
   (?:const|let|var)?\s*OAUTH_CLIENT_SECRET\s*=\s*['"]([\w\-]+)['"]\s*;
   ```

This is the **only** way the refresh request can be made.

### 3.5 Fetch sequence

1. Read `settings.json` → check `security.auth.selectedType`. Reject `api-key`, `vertex-ai`.
2. Read `oauth_creds.json`. If `access_token` missing → `notLoggedIn`.
3. If `expiry_date < now`, extract client creds (§3.4) and call the token endpoint. On 200, write
   the new `access_token`, recompute `expiry_date = now + expires_in*1000` (ms), and write back the
   file atomically. On 401, raise `notLoggedIn`.
4. Call `loadCodeAssist`. Extract `currentTier.id` and `cloudaicompanionProject` (which may be a
   string *or* an object containing `id`/`projectId`). Used for managed-project quota and tier.
5. If no project from loadCodeAssist, fall back to listing projects and picking either one whose
   ID starts with `gen-lang-client` or one with a `generative-language` label.
6. POST to `:retrieveUserQuota` with `{"project":"<id>"}` (or `{}` when nothing is known).
7. Parse `buckets[]` (described in §3.7).
8. Build `identity.accountEmail` from the `id_token` JWT (`email` claim) and `identity.loginMethod`
   from the tier (§3.6).

### 3.6 Plan / tier mapping

| `currentTier.id` | `hd` claim in id_token | Display |
|------------------|------------------------|---------|
| `standard-tier` | any | `Paid` |
| `free-tier` | present | `Workspace` |
| `free-tier` | absent | `Free` |
| `legacy-tier` | any | `Legacy` |
| (loadCodeAssist failed) | — | leave blank |

### 3.7 Quota response → snapshot

```text
buckets: [
  { modelId: "gemini-2.5-pro", remainingFraction: 0.82, resetTime: "2026-05-13T07:00:00Z", tokenType: "input" },
  ...
]
```

`remainingFraction` is `0.0–1.0`. Pipeline:

1. Group by `modelId`, keep the **lowest** `remainingFraction` per model (input vs output buckets).
2. Bucket the models:
   - `flash-lite` → Flash Lite group
   - contains `flash` but not `flash-lite` → Flash group
   - contains `pro` → Pro group
3. For each group, take the model with the lowest fraction.

| Snapshot field | Source |
|----------------|--------|
| `primary` | min Pro model. `usedPercent = (1 - frac) * 100`, `windowMinutes = 1440` |
| `secondary` | min Flash model, same shape |
| `tertiary` | min Flash-Lite model, same shape |
| `resetsAt` | ISO-8601 `resetTime`, with or without fractional seconds |
| `resetDescription` | `"Resets in Xh Ym"` from now until `resetTime` (humanized) |
| `identity.accountEmail` | `email` claim from `id_token` JWT |
| `identity.loginMethod` | plan label from §3.6 |

When *all* groups are empty the legacy `parse(text:)` CLI parser is **not** invoked from the fetch
path — it's kept only for offline debug.

### 3.8 Settings keys

Gemini has effectively no per-provider settings — it reuses the CLI's state files. The
`source modes` array in the descriptor is `[.auto, .api]` and the menu only lets the user toggle the
provider on/off.

### 3.9 Login flow

1. Locate `gemini` binary via `BinaryLocator`. If missing → return `missingBinary`; UI tells the user
   to install the CLI.
2. Delete `~/.gemini/oauth_creds.json` and `~/.gemini/google_accounts.json` so the next run forces a
   fresh OAuth flow.
3. Write a temp shell command file: `#!/bin/bash\ncd ~\n"<binary>"`.
4. Make it executable, then `NSWorkspace.shared.open(...)` to launch Terminal.app on it. Schedule a
   cleanup of the temp file after 10 seconds.
5. In the background, poll the filesystem for `oauth_creds.json` (every 1 s, up to 5 min). When it
   appears, sleep 500 ms (let the write finish) and fire the `onCredentialsCreated` callback so the
   menu auto-refreshes.

### 3.10 Edge cases

- **API-key auth selected.** Refuse to fetch. Surface a tip to switch the CLI to "Login with Google".
- **Multiple installs / fnm.** The package-root walker bounded to 8 ascents prevents an unrelated
  Gemini install from polluting credential extraction.
- **Project discovery failure.** Send `{}` body — the API still returns reasonable defaults for
  free-tier managed projects.
- **Token endpoint write fails.** Don't crash the fetch; the in-memory `access_token` from the
  refresh response is still usable for this run.

### 3.11 Mac → Windows mapping

| Mac | Windows |
|-----|---------|
| `~/.gemini/oauth_creds.json` | `%USERPROFILE%\.gemini\oauth_creds.json` (same JSON; check that the CLI writes here on Windows — confirmed by upstream) |
| `~/.gemini/settings.json` | `%USERPROFILE%\.gemini\settings.json` |
| `which gemini` / `PATH` walk | `where.exe gemini` + `BinaryLocator` searching `%APPDATA%\npm`, `%LOCALAPPDATA%\fnm_multishells`, `%PROGRAMFILES%`, scoop shims |
| Homebrew layout | scoop install (`%USERPROFILE%\scoop\apps\gemini-cli\current\`), npm global (`%APPDATA%\npm\node_modules\@google\gemini-cli\`), winget |
| AppleScript Terminal launch | `start cmd.exe /K "gemini"` via `tauri::api::process::Command`. On Windows Terminal use `wt.exe new-tab gemini`. |
| Temp `.command` script | Temp `.cmd` file in `%TEMP%`, executable bit not needed. |
| Filesystem poll for creds | Same poll; or use `notify` crate to watch `%USERPROFILE%\.gemini\` for create events. |

### 3.12 Acceptance checklist

- [ ] OAuth client_id/secret extraction works for at least: scoop, winget, npm-global, fnm,
      `bundle/gemini.js` mode.
- [ ] Token refresh updates `oauth_creds.json` atomically.
- [ ] Tier mapping renders `Paid` / `Workspace` / `Free` / `Legacy` correctly.
- [ ] Pro / Flash / Flash-Lite buckets land in primary / secondary / tertiary respectively.
- [ ] Reset timer reads `Xh Ym` and never goes negative ("Resets soon").
- [ ] `api-key` / `vertex-ai` auth-types are rejected before any network call.
- [ ] Login flow watches for credentials file and auto-refreshes.

---

## 4. Vertex AI — provider ID `vertexai`

> One-line: read gcloud Application Default Credentials, refresh via Google OAuth, query Cloud
> Monitoring quota time-series. Cost data is *separately* harvested from local Claude Code logs.

### 4.1 Auth source order

| Order | Source | Notes |
|-------|--------|-------|
| 1 | `$GOOGLE_APPLICATION_CREDENTIALS` | Absolute path. Service account JSON also accepted. |
| 2 | `$CLOUDSDK_CONFIG/application_default_credentials.json` | When `CLOUDSDK_CONFIG` is set |
| 3 | `~/.config/gcloud/application_default_credentials.json` | Default ADC location (cross-platform — `gcloud` writes here on Windows too) |

Project ID resolution:

| Order | Source |
|-------|--------|
| 1 | `service_account.project_id` field |
| 2 | `$CLOUDSDK_CONFIG/configurations/config_default` (INI) |
| 3 | `~/.config/gcloud/configurations/config_default` (INI) |
| 4 | `$GOOGLE_CLOUD_PROJECT`, `$GCLOUD_PROJECT`, `$CLOUDSDK_CORE_PROJECT` |

### 4.2 Credentials file shapes

User credentials (from `gcloud auth application-default login`):

```json
{
  "client_id": "32555940559.apps.googleusercontent.com",
  "client_secret": "...",
  "refresh_token": "1//0g...",
  "id_token": "<JWT>",
  "token_expiry": "2026-05-12T08:00:00Z",
  "type": "authorized_user"
}
```

Service account credentials (`client_email` + `private_key`) are handled differently: we do **not**
sign a JWT ourselves. Instead, we invoke `gcloud auth application-default print-access-token`
(20 s timeout) and assume an in-memory 50-minute lifetime. The `client_email` is the account label
and `project_id` is consumed directly.

### 4.3 Token refresh

Endpoint: `POST https://oauth2.googleapis.com/token` with form body:

```
client_id=<id>&client_secret=<secret>&refresh_token=<token>&grant_type=refresh_token
```

Refresh policy: refresh if `expiry_date - now < 5 min`. Error mapping:

| HTTP / error | Maps to |
|--------------|---------|
| 400/401 + `error: invalid_grant` | `expired` (user must rerun `gcloud auth application-default login`) |
| 400/401 + `error: unauthorized_client` | `revoked` |
| Other 4xx | `invalidResponse(status)` |
| Network | `networkError(...)` |

Successful refresh returns `access_token` (+ optional new `id_token` with the email claim) plus
`expires_in` (seconds). Persist nothing to gcloud's file — the access token is cached only in
memory for this run.

### 4.4 Endpoints

| Method | URL | Notes |
|--------|-----|-------|
| GET | `https://monitoring.googleapis.com/v3/projects/<projectId>/timeSeries` | Cloud Monitoring time-series. Paginated. |
| POST | `https://oauth2.googleapis.com/token` | Token refresh. |

`timeSeries` query parameters:

```
filter=metric.type="serviceruntime.googleapis.com/quota/allocation/usage"
       AND resource.type="consumer_quota"
       AND resource.label.service="aiplatform.googleapis.com"
interval.startTime=<now-24h ISO8601>
interval.endTime=<now ISO8601>
aggregation.alignmentPeriod=3600s
aggregation.perSeriesAligner=ALIGN_MAX
view=FULL
pageToken=<token>
```

Two filters are run: one with `quota/allocation/usage`, one with `quota/limit`. Both required —
without either we return `noData`. Status mapping: 401 → `unauthorized`, 403 → `forbidden`, other →
`invalidResponse`.

### 4.5 Response → snapshot mapping

Each `timeSeries[]` entry has `metric.labels`, `resource.labels`, `points[].value`. Key is
`(quota_metric, limit_name, location)` where:

- `quota_metric` = `metric.labels.quota_metric` ?? `resource.labels.quota_id`
- `limit_name` = `metric.labels.limit_name` ?? `""`
- `location` = `resource.labels.location` ?? `"global"`

For each key, take `max` of `points[].value.doubleValue / int64Value` from usage and limit series,
then compute `percent = usage / limit * 100` for keys that appear in both. The snapshot's headline
is the **maximum** percent across matched keys.

| Snapshot field | Source |
|----------------|--------|
| `primary.usedPercent` | max matched `usage/limit * 100` |
| `windowMinutes` | nil (current quota state, not a window) |
| `resetsAt` / `resetDescription` | nil |
| `identity.accountEmail` | `id_token.email` (or `client_email` for service accounts) |
| `identity.accountOrganization` | `projectId` |
| `identity.loginMethod` | `"gcloud"` |
| `providerCost` | nil (filled separately by the cost scanner) |

### 4.6 Settings keys

The provider has no UI-editable settings beyond the enable toggle. All state lives in gcloud config.

### 4.7 Login flow (Mac)

1. Show alert with instructions and an "Open Terminal" button.
2. AppleScript runs:
   ```
   gcloud auth application-default login \
     --scopes=openid,https://www.googleapis.com/auth/userinfo.email,https://www.googleapis.com/auth/cloud-platform
   ```
3. After 2 s, trigger a UsageStore refresh so the fresh credentials are picked up.

### 4.8 Token cost tracking (separate path)

Vertex AI Claude usage logs to `~/.claude/projects/**/*.jsonl` (same as direct Anthropic). The
**only** marker we trust is the model name format `claude-opus-4-5@20251101` (note the `@`). The
Anthropic API uses `-` as separator, so we distinguish them by:

1. Primary: presence of `@` in `message.model`.
2. Fallback: `metadata.provider == "vertexai"` or any metadata key containing `vertex` / `gcp`.

If Claude Code normalizes to `-` (which it sometimes does in newer versions), Vertex AI entries are
indistinguishable from direct Anthropic — surface this as a known limitation in the UI when cost is
zero but quota usage is non-zero.

### 4.9 Edge cases

- **`noData` after auth success.** Common when the project has no Vertex usage in the last 24h.
  Snapshot returns empty primary; the menu still shows cost data from local logs.
- **Service-account file but no `gcloud` binary.** `printAccessToken` will fail. Surface a clear
  error: "gcloud CLI required for service-account ADC."
- **Project mismatch.** ADC says project A but Cloud Monitoring is queried against project A — if
  the user has multiple projects, the meter only reflects A. There is no UI override; document this.
- **Aggregation window.** Always 24 h with 1 h alignment. If we ever want "current 5h", change
  `interval.startTime` and `alignmentPeriod` together.
- **Paginate via `nextPageToken`.** Big organizations can return >1 page even for a single quota.

### 4.10 Mac → Windows mapping

| Mac | Windows |
|-----|---------|
| `~/.config/gcloud/application_default_credentials.json` | `%APPDATA%\gcloud\application_default_credentials.json` (gcloud uses APPDATA on Windows) |
| `~/.config/gcloud/configurations/config_default` | `%APPDATA%\gcloud\configurations\config_default` |
| `gcloud auth application-default print-access-token` subprocess | `tauri::api::process::Command::new("gcloud.cmd").args([...])` — the `.cmd` extension is required to find scoop/installer shims. Timeout 20 s. |
| AppleScript Terminal launch | Open `cmd.exe` (or `wt.exe`) with the gcloud command pre-typed; alternatively, launch the user's default browser to the same login URL gcloud uses. |
| `~/.claude/projects/` for cost scan | `%USERPROFILE%\.claude\projects\` — verified same path on Windows. Honor `$CLAUDE_CONFIG_DIR` env (comma-separated). |

### 4.11 Acceptance checklist

- [ ] ADC user credentials refresh against `oauth2.googleapis.com`.
- [ ] Service-account credentials path invokes `gcloud auth application-default print-access-token`.
- [ ] Project ID falls through env-var ladder when config file is missing.
- [ ] Time-series filter strings are constructed exactly (whitespace is significant in monitoring's
      filter parser).
- [ ] `usage / limit` aggregation matches the GCP Console Quotas page within ±1%.
- [ ] `noData` does not crash; snapshot still renders the identity + zero meter.
- [ ] Login button opens a working terminal with the gcloud command and refresh fires after.
- [ ] Cost scanner picks up `claude-*-*@<date>` model names but not `claude-*-*-<date>`.

---

## 5. Factory (Droid) — provider ID `factory`

> One-line: WorkOS-backed auth with **eight** ordered sources covering session cookies, bearer
> tokens, refresh tokens, and browser-storage scraping. New "token rate limits" billing payload
> exposes 5h / weekly / monthly windows.

### 5.1 Auth source order (loadbearing)

| # | Source | What it gives |
|---|--------|---------------|
| 1 | Manual cookie header (settings) | Cookie string with optional `access-token=` JWT |
| 2 | Cached header (keychain cache `cookie.factory`) | Last good cookie blob |
| 3 | Stored session cookies | `factory-session.json` → `cookies[]` |
| 4 | Stored bearer token | `factory-session.json` → `bearerToken` |
| 5 | Stored WorkOS refresh token | `factory-session.json` → `refreshToken` → mint bearer via WorkOS |
| 6 | Local-storage WorkOS tokens | Safari sqlite + Chrome leveldb scrapes (§5.7) |
| 7 | Browser cookies (Safari first, then Chrome/Firefox) | Factory cookies |
| 8 | WorkOS cookies (Safari, then Chrome/Firefox) | `workos.com` cookies → mint bearer |

Each successful step caches its result back into `factory-session.json` so the next probe is one
hop. `invalid_grant` from WorkOS clears the stored refresh token. `notLoggedIn` from Factory clears
the stored cookies (but not the bearer / refresh — those have their own validation).

### 5.2 Cookie names treated as session

```
wos-session
__Secure-next-auth.session-token, next-auth.session-token
__Secure-authjs.session-token, authjs.session-token, __Host-authjs.csrf-token
session
access-token        ← also used as bearer when value contains a dot (JWT shape)
```

Stale-token retry filters (removed during the 409 retry chain): `access-token`, `__recent_auth`.

Cookie domains scanned: `factory.ai`, `app.factory.ai`, `auth.factory.ai`.

### 5.3 Base URL selection

The probe rotates through `app.factory.ai`, `api.factory.ai`, `auth.factory.ai` in order,
deduped and re-ordered so `auth.factory.ai` is tried first **if** the cookie set already has cookies
on that domain.

### 5.4 Endpoints

All requests carry these headers (Factory's API rejects requests without `x-factory-client`):

```
Accept: application/json
Content-Type: application/json
Origin: https://app.factory.ai
Referer: https://app.factory.ai/
x-factory-client: web-app
Cookie: <if cookies present>
Authorization: Bearer <token>  ← if a bearer is available
```

| Method | URL | Purpose |
|--------|-----|---------|
| GET | `<baseURL>/api/app/auth/me` | Org, plan, user.id |
| GET | `https://api.factory.ai/api/billing/limits` | New `usesTokenRateLimitsBilling` + 5h/7d/monthly limits + `extraUsageBalanceCents` |
| GET | `<baseURL>/api/organization/subscription/usage?useCache=true[&userId=<id>]` | Standard/Premium token usage |
| POST | `https://api.workos.com/user_management/authenticate` | Mint access/refresh tokens |

### 5.5 WorkOS token minting

Body shape A (refresh token):

```
{ "client_id":"<wos>", "grant_type":"refresh_token", "refresh_token":"<rt>",
  "organization_id":"<optional>" }
```

Body shape B (cookies only):

```
{ "client_id":"<wos>", "grant_type":"refresh_token", "useCookie": true,
  "organization_id":"<optional>" }
```

The cookie path also sends a `Cookie:` header with the workos.com cookies. Try both WorkOS client
IDs in order:

```
client_01HXRMBQ9BJ3E7QSTQ9X2PHVB7
client_01HNM792M5G5G1A2THWPXKFMXB
```

`HTTP 400` with body containing `"missing refresh token"` ⇒ `noSessionCookie` (caller should clear
refresh storage). Other 4xx ⇒ `networkError`.

### 5.6 Session storage

File: `factory-session.json`. Shape:

```json
{
  "cookies": [ {<HTTPCookie properties>} ],
  "bearerToken": "ey...",
  "refreshToken": "1//0g..."
}
```

`HTTPCookie` props serialize the same way as Cursor's store (dates become `timeIntervalSince1970`
+ a `_isDate` marker key; URLs become strings + `_isURL`).

### 5.7 Local-storage WorkOS extraction

For Safari: walk `~/Library/Containers/com.apple.Safari/Data/.../WebKit/WebsiteData/Default/`, find
`origin` files containing `app.factory.ai` or `auth.factory.ai`, then read
`LocalStorage/localstorage.sqlite3` (table `ItemTable` or `localstorage`, columns `key`/`value`).

For Chrome/Chromium forks: enumerate profile dirs (`Default`, `Profile *`, `user-*`), find
`Local Storage/leveldb`, then byte-scan all `.ldb` / `.log` files for `workos:refresh-token` and
`workos:access-token` with regex:

```
workos:refresh-token[^A-Za-z0-9_-]*([A-Za-z0-9_-]{20,})
workos:access-token[^A-Za-z0-9_-]*([A-Za-z0-9_-]{20,})
```

Supported Chromium browsers: `chrome`, `chromeBeta`, `chromeCanary`, `arc`, `arcBeta`, `arcCanary`,
`dia`, `chatgptAtlas`, `chromium`, `helium`. Helium uses a non-`User Data` root.

Org ID is extracted from the access-token JWT's middle segment (`org_id` claim) when available.

### 5.8 409 retry chain

When `/api/app/auth/me` returns 409 (stale-token detection):

1. Retry without the `Authorization` header (cookies only).
2. Retry without `access-token` / `__recent_auth` cookies.
3. Retry without `session` / `wos-session` cookies.
4. Retry without both.
5. Retry with **only** the auth.js session cookies (`__Secure-authjs.session-token` family) +
   `__Host-authjs.csrf-token`.

Each retry returns early on `notLoggedIn` since cookies aren't the problem. The original error is
re-thrown if all retries continue to 409.

### 5.9 Response → snapshot mapping

Two output shapes, based on `/api/billing/limits.usesTokenRateLimitsBilling`:

**Legacy shape (`subscription/usage`):**

```text
usage.startDate, usage.endDate  (ms-since-epoch)
usage.standard { userTokens, orgTotalTokensUsed, totalAllowance, usedRatio, ... }
usage.premium  { ... }
```

| Snapshot field | Source |
|----------------|--------|
| `primary` | Standard `usedRatio` (preferred) or `userTokens / totalAllowance` |
| `secondary` | Premium, same |
| `resetsAt` | `usage.endDate` (ms → seconds) |
| `resetDescription` | `"Resets MMM d at h:mma"` |
| `identity.organization` | `auth.organization.name` |
| `identity.loginMethod` | `"Factory <tier> - <planName>"` (de-duplicated when plan already contains "Factory") |

`usedRatio` rules:
- If in `[-0.001, 1.001]` → treat as fraction, scale to %.
- If allowance is **unreliable** (`<= 0` or `> 1e12` — sentinel "unlimited") *and* ratio in
  `[-0.1, 100.1]` → treat as already-a-percent.
- Otherwise compute from `userTokens / totalAllowance`.
- Allowance > 1 trillion is "unlimited"; show `min(100, used / 100M * 100)`.

**Token-rate-limits shape (`billing/limits`):**

```text
limits.standard { fiveHour, weekly, monthly }      each { usedPercent, windowEnd, secondsRemaining }
limits.core     { fiveHour, weekly, monthly }      optional
extraUsageBalanceCents, overagePreference
```

| Snapshot field | Source |
|----------------|--------|
| `primary` | `standard.fiveHour` → `windowMinutes = 300` |
| `secondary` | `standard.weekly` → `windowMinutes = 10080` |
| `tertiary` | `standard.monthly` → `windowMinutes = nil` |
| `extraRateWindows` | core five-hour / weekly / monthly (when `core.hasUsageData`) |
| `providerCost` | `extraUsageBalanceCents / 100` as USD, period `"Extra usage balance"` |
| `identity.loginMethod` | adds `"Fallback: <overagePreference>"` |

For each `FactoryBillingWindow`:
- `resetAt`: prefer `secondsRemaining` (now + secs); otherwise `windowEnd.date` if it's in the
  future.
- "Stale-but-not-rolling" detection: if there's a `windowEnd` but no `secondsRemaining` *and*
  `resetAt == nil`, force `usedPercent = 0` (the web UI does the same — treats as already reset).
- Otherwise clamp `usedPercent` to `[0, 100]`.

`windowEnd` decoder accepts Double (seconds *or* milliseconds — `> 1e12` ⇒ ms), numeric strings,
and ISO-8601 (with or without fractional seconds).

### 5.10 Settings keys (`providers.factory.*`)

| Key | Type | Default |
|-----|------|---------|
| `cookieSource` | enum | `auto` |
| `cookieHeader` | string | `""` |
| `tokenAccounts` | array | `[]` — each entry stores a cookie header |

### 5.11 Login flow

Trivial on Mac: open `https://app.factory.ai` in the default browser. The next refresh picks up
cookies from whatever browser the user logged in with. On Windows, do the same plus offer an
in-app WebView fallback (Tauri's WebView2 with non-persistent storage) for users who don't want
to share cookies with their daily browser.

### 5.12 Edge cases

- **JWT vs opaque session tokens.** Log a hint when picking a bearer: tokens containing `.` are
  JWTs (acceptable for `Authorization: Bearer`); opaque tokens are sent only as cookies.
- **Multiple WorkOS client IDs.** Two app IDs exist in the wild — try both, take the first 200.
- **Org switching.** Some accounts return `organization_id` in the access token's `org_id` claim; we
  use it for token re-mint to keep the user in the same org.
- **`useCookie: true`.** Required when the refresh token lives only in a `workos.com` cookie (not in
  local storage).
- **Helium.** Helium is a Chromium variant that omits the `User Data` subdir — handle the alternate
  root layout.

### 5.13 Mac → Windows mapping

| Mac | Windows |
|-----|---------|
| Safari Containers WebKit local storage | n/a (skip — no Safari on Windows) |
| Chrome `Local Storage/leveldb` (raw byte scan) | `%LOCALAPPDATA%\Google\Chrome\User Data\<Profile>\Local Storage\leveldb` — same format, scan the same way |
| Edge / Brave / Arc / Vivaldi leveldb | Same Windows-rooted paths (`%LOCALAPPDATA%\Microsoft\Edge`, `BraveSoftware\Brave-Browser`, `Arc\User Data`, `Vivaldi\User Data`) |
| Firefox cookies for workos.com | `%APPDATA%\Mozilla\Firefox\Profiles\*.default*\cookies.sqlite` |
| `factory-session.json` in App Support | `%APPDATA%\CodexBar\factory-session.json` |
| `sqlite3` API for Safari localstorage | n/a |
| AppleScript open browser | `tauri::api::shell::open` |

### 5.14 Acceptance checklist

- [ ] All eight fetch sources execute in order; each writes its successful state back.
- [ ] WorkOS token minting succeeds for both client IDs.
- [ ] `useCookie: true` body works when only WorkOS cookies are present.
- [ ] Token-rate-limits payload renders the 5h / weekly / monthly windows correctly.
- [ ] Stale window detection (windowEnd present, no secondsRemaining, expired) zeros the meter.
- [ ] LevelDB byte scanner finds `workos:refresh-token` across Chrome, Edge, Brave, Arc, Helium.
- [ ] 409 retry chain progresses through every variant before giving up.
- [ ] Extra usage balance renders as USD in providerCost.
- [ ] Org name + plan compose into the identity label without duplicate "Factory".

---

## 6. OpenRouter — provider ID `openrouter`

> One-line: API-key auth (no OAuth, no cookies). Two endpoints: total credits and per-key
> rate-limit. Credit-based: balance = totalCredits − totalUsage.

### 6.1 Auth source order

| Order | Source |
|-------|--------|
| 1 | `$OPENROUTER_API_KEY` |
| 2 | `config.providers.openrouter.apiKey` |
| 3 | `tokenAccounts` (multi-account) |

Tokens look like `sk-or-v1-<random>`. Validate non-empty after trimming surrounding quotes (people
paste with quotes from `.env` files).

### 6.2 Endpoints

| Method | URL | Purpose |
|--------|-----|---------|
| GET | `<baseURL>/credits` | `{ data: { total_credits, total_usage } }` |
| GET | `<baseURL>/key` | `{ data: { rate_limit, limit, usage } }` |

`<baseURL>` defaults to `https://openrouter.ai/api/v1`. Overridable via `$OPENROUTER_API_URL`.

Required headers on both calls:

```
Authorization: Bearer <api_key>
Accept: application/json
HTTP-Referer: <$OPENROUTER_HTTP_REFERER, if set>
X-Title: <$OPENROUTER_X_TITLE, default "CodexBar">
```

Credit endpoint timeout: 15 s. Key endpoint timeout: **1 s** with a parallel sleep task — if
`/key` takes longer it is skipped silently so the credits meter is never delayed.

### 6.3 Response → snapshot mapping

`/credits` shape:

```json
{ "data": { "total_credits": 50.0, "total_usage": 12.34 } }
```

Derived: `balance = max(0, total_credits − total_usage)`,
`usedPercent = min(100, total_usage / total_credits * 100)` (0 when totalCredits == 0).

`/key` shape:

```json
{ "data": { "rate_limit": { "requests": 60, "interval": "10s" },
            "limit": 100.0, "usage": 27.5 } }
```

`keyQuotaStatus`:
- `available` — `limit > 0` and `usage >= 0`
- `noLimitConfigured` — `keyDataFetched && (limit nil || limit <= 0)`
- `unavailable` — `/key` failed or timed out

| Snapshot field | Source |
|----------------|--------|
| `primary.usedPercent` | `keyUsedPercent` = `min(100, max(0, keyUsage / keyLimit * 100))` if `available`; else nil |
| `windowMinutes` | nil — credit usage is monotone, not a window |
| `resetsAt` / `resetDescription` | nil |
| `identity.loginMethod` | `"Balance: $X.XX"` (balance formatted to 2 decimal places) |
| `openRouterUsage` | full snapshot (totals, balance, keyLimit, keyUsage, rateLimit) for the menu card |

### 6.4 Settings keys

| Key | Type | Default | Notes |
|-----|------|---------|-------|
| `apiKey` | string | `""` | sk-or-v1-* |

The env-var `$OPENROUTER_API_KEY` takes precedence over the stored key in `ProviderTokenResolver`.

### 6.5 Login flow

There is no flow. The user pastes their key from `https://openrouter.ai/settings/keys`. The
preferences pane stores it via `config.providers.openrouter.apiKey`. The dashboard link in the menu
goes to `https://openrouter.ai/settings/credits`.

### 6.6 Edge cases

- **Empty key.** Throws `invalidCredentials` before any HTTP request.
- **`/key` not available for free tiers.** Snapshot lands without `primary`; the menu card shows
  just the balance.
- **Slow `/key`.** The 1 s sleep race cancels the request and returns "not fetched" — the credits
  meter still updates.
- **Stale credits (≤60 s server cache).** Document the latency; do not pre-empt by polling faster.
- **Non-200 bodies.** Redact bearer/sk-or-v1-* tokens in logs (`LogRedactor`). Honor
  `$CODEXBAR_DEBUG_OPENROUTER_ERROR_BODIES=1` for full debug bodies, still redacted.
- **API URL override.** Trim surrounding quotes. Fail closed (use default) if not a parseable URL.
- **HTTP-Referer + X-Title.** Both optional; defaults reduce friction for shared keys but users with
  per-app limits should override `X-Title`.

### 6.7 Mac → Windows mapping

| Mac | Windows |
|-----|---------|
| Plain string in keychain-backed config.json | DPAPI-protected blob in `%APPDATA%\CodexBar\config.json` (whole-file encryption is fine since OpenRouter has no auxiliary state) |
| `ProcessInfo.environment` | `std::env::var` |
| `URLSession` | `reqwest` with `Authorization: Bearer` |

### 6.8 Acceptance checklist

- [ ] Bearer header is exactly `Bearer <key>` (no `token ` prefix).
- [ ] `/credits` and `/key` fire in parallel; `/key` does not block the primary path past 1 s.
- [ ] Balance string format `Balance: $12.34` (en-US locale, 2 fractional digits).
- [ ] `noLimitConfigured` vs `unavailable` are distinguishable in the UI.
- [ ] Env var overrides stored key.
- [ ] Logs redact `sk-or-v1-*` and `Bearer` tokens in error bodies.
- [ ] Trimming strips both `"..."` and `'...'` wrappers.

---

## Cross-cutting notes

### Browser-cookie pipeline (shared)

Cursor, Factory, and (via auth-doc) several other providers share the same cookie-import surface.
Single Rust implementation must expose:

- A `CookieRecord { name, value, domain, path, expires_at, secure, http_only }` type.
- A `read_cookies(browser, domains) -> Result<Vec<CookieRecord>>` for each Browser variant
  (`edge`, `chrome`, `chrome_canary`, `brave`, `vivaldi`, `arc`, `opera`, `chromium`, `helium`,
  `firefox`). Linux/Mac variants stay behind feature flags.
- A `BrowserDetection` capable of returning the installed set and the cookie-eligible subset
  (skips browsers with no profile data; we never want to provoke a Keychain/DPAPI prompt for an
  uninstalled browser).
- A `BrowserCookieAccessGate` that remembers per-browser failures within a single app session so we
  don't retry repeatedly.

Decryption layer:
- Chromium ≥ v80: `os_crypt.encrypted_key` from `Local State`, DPAPI-unwrap the first 5 bytes
  prefix, then AES-GCM decrypt cookie values with the 32-byte key.
- Firefox: SQLite `cookies.sqlite`, columns plain.
- See the auth-spec document for the Rust crate(s) (`rookie`, `chrome_cookies`, or a custom DPAPI
  reader using `windows-rs`).

### Manual-cookie / paste-header

Every cookie-based provider accepts a manual fallback (`cookieSource = manual`). Implementation:

- `CookieHeaderNormalizer` trims, strips wrapping quotes, normalizes `;` separators.
- Accepts either raw `name=value; name=value` or the full `Cookie: name=value` header.
- The fetch path treats manual cookies as authoritative; failures do not invalidate them (only the
  user can fix bad pastes).

### Cached cookie / token cache

Mac keychain (`com.steipete.codexbar.cache`, account `cookie.<provider>`) becomes a DPAPI-protected
file at `%APPDATA%\CodexBar\cache\cookie.<provider>.bin`. Same struct: `{ cookieHeader, sourceLabel,
storedAt }`. Cleared on `notLoggedIn` only — transient failures must preserve it.

### JWT claim parsing

Used by Gemini (`email`, `hd`), Vertex AI (`email`), and Factory (`sub`, `org_id`). All three use
base64url decoding (`-`→`+`, `_`→`/`, padding to multiple of 4). Implementation should live in one
helper:

```text
fn parse_jwt_claims(token: &str) -> Option<JsonValue>
```

### OAuth refresh

Gemini and Vertex AI both POST to `oauth2.googleapis.com/token` with form-encoded bodies. Different
client credentials sources but identical wire shape. Share a `RefreshRequest` builder.

### Provider-specific quirks

| Provider | Quirk |
|----------|-------|
| Cursor | "team plan" detection requires falling back from `individualUsage.plan` → `individualUsage.overall` → `teamUsage.pooled`. The headline percent path enumerates all six precedence rules. |
| Cursor | Auto/API percents are *already* percentages even if `< 1`. Do not multiply. |
| Copilot | GitHub's bot detector rejects requests without the exact VS Code header set. Bumping `Editor-Version` numbers should be tested before shipping. |
| Copilot | Device flow uses VS Code's client ID `Iv1.b507a08c87ecfe98`; using a custom OAuth app for Copilot is **not** possible (GitHub blocks non-Copilot client IDs from `/copilot_internal`). |
| Gemini | OAuth client_id/secret are extracted from a packaged JS file. Plan for the upstream layout to change — keep the path list editable, with bundle walking as a backstop. |
| Gemini | `cloudaicompanionProject` decodes as either a string or an object; both shapes must work. |
| Vertex AI | gcloud config parser is INI-style; the only key we read is `project = ...`. |
| Vertex AI | Quota usage requires Cloud Monitoring API access. Service accounts without `monitoring.timeSeries.list` will fail with 403; the UI should link to the IAM page. |
| Vertex AI | Cost detection depends on the `@` model name suffix — fragile by design. |
| Factory | Token-rate-limits billing (`usesTokenRateLimitsBilling`) is a new flag — both code paths must coexist for years. |
| Factory | LevelDB byte-scanning is fragile but unavoidable; never use Chrome's bundled `leveldb` library because that locks the DB. |
| Factory | The same access-token's `org_id` JWT claim is reused to scope WorkOS refreshes. |
| OpenRouter | The `/key` endpoint is parallelized with a 1-second timeout because OpenRouter has had latency spikes there; never block the primary `/credits` call on it. |
| OpenRouter | `Authorization: Bearer <key>` — not `token <key>` (which is GitHub's form). |

### Shared snapshot post-processing

After every provider's fetch:

1. Clamp `usedPercent` to `[0, 100]`.
2. If `resetsAt` is in the past, drop it (don't show negative timers).
3. Apply quota-warning thresholds from settings (yellow at 80%, red at 95% by default).
4. Stamp `updatedAt`. UI uses this to drive the "Refreshed Xs ago" text and the staleness banner
   (red when older than 10 minutes).

### Refresh cadence

The provider refresh loop (documented in `docs/refresh-loop.md`) runs every 60 s by default with
per-provider override. Tier-1 providers should keep default cadence except Cursor and Factory,
which have noticeable cold-start cost (browser cookie reads can be 200–500 ms on Windows due to
DPAPI). The shared scheduler should debounce repeated failures with exponential backoff up to 10 min.
