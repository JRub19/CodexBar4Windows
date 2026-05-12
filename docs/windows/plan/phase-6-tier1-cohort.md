# Phase 6: Tier 1 Provider Cohort (Cursor, Copilot, Gemini, OpenRouter, Factory)

Status: planned. Owner: Windows port team. Branch policy per CLAUDE.md: work directly on `main` with atomic, conventional commits, push after each commit.

## Why this phase exists

Phases 0 through 5 stood up the Tauri 2 shell, the React popup, the shared Rust `codexbar` crate, the tray icon, the descriptor and registry framework, the cookie pipeline used by Claude in Phase 4, and the first two live v1 providers (Codex and Claude). Phase 6 finishes the v1 cohort by lighting up the remaining five providers: Cursor, Copilot, Gemini, OpenRouter, and Factory.

After this phase the popup must show real, live data for all seven v1 providers simultaneously on a Windows 11 machine that has any reasonable combination of Edge, Chrome, Brave, Firefox, the Gemini CLI, the GitHub login, an OpenRouter API key, and a Factory account.

The reference specs are:

* `C:\Code\CodexBar4Windows\docs\windows\spec\30-provider-system-architecture.md` for the framework contract
* `C:\Code\CodexBar4Windows\docs\windows\spec\42-providers-tier1.md` for the exact per-provider behavior, headers, body shapes, and edge cases
* `C:\Code\CodexBar4Windows\docs\windows\spec\60-auth-cookies-secrets.md` for DPAPI, cookie pipeline, and OAuth refresh helpers

Every commit in this phase is small enough to revert in isolation. A reviewer should be able to read one commit, run the matching unit test, and see green.

## Dependencies on prior phases

Phase 6 assumes the following pieces already exist on `main`:

* `rust/src/providers/registry.rs` with the `inventory!`-driven catalog from Phase 3
* `ProviderDescriptor`, `ProviderMetadata`, `ProviderBranding`, `ProviderFetchPlan`, `Strategy` trait, `ProviderFetchOutcome` types from Phase 3
* `rust/src/host/cookies/` with a working Chromium DPAPI decoder for Edge, Chrome, Brave, Firefox, plus the `CookieRecord` type, used by Claude in Phase 4
* `rust/src/host/secrets/dpapi.rs` for DPAPI-protected blob read and write under `%APPDATA%\CodexBar\`
* `rust/src/host/http.rs` exposing a shared `reqwest::Client` with default timeouts, redacting `Authorization` and `Cookie` headers in tracing
* `rust/src/host/keyring.rs` removed or stubbed: Windows does not use Keychain
* `apps/desktop-tauri/src/providers/shared/ProviderCard.tsx`, `ProviderSettingsPanel.tsx`, `ProviderIcon.tsx` from Phase 3
* IPC commands `provider_descriptors`, `provider_snapshots`, `provider_refresh`, `provider_login` and the `usage:updated` event
* Claude OAuth refresh, written in Phase 4, currently in `rust/src/providers/claude/oauth.rs`. Phase 6 generalizes the helper.

If any of the above is missing or broken, stop and fix it first. Phase 6 does not paper over Phase 3 or Phase 4 holes.

## What this phase does not deliver

Phase 6 is scoped to the five providers above plus their shared scaffolding. It does not deliver:

* The Vertex AI provider. Vertex requires gcloud ADC, Cloud Monitoring quota timeseries, and a separate cost scanner pass. It moves to Phase 7 alongside the cost scanning work in spec 70.
* Status feeds, statuspage.io polling, or incident chips. Those land in Phase 7 with spec 55.
* The cost scanner for JSONL logs. Phase 7.
* Notifications, toast surfacing, or the global hotkey. Phase 8.
* CLI parity (`codexbar usage --provider <name>`). Phase 9 per spec 90.
* Auto update and signing. Phase 10.

## Deliverables broken out per provider

The phase is organized so a reader can take one provider front to back. Cross cutting work is collected in section F.

### A. Cursor provider

A1. A new module `rust/src/providers/cursor/` with:

* `descriptor.rs` returning the `ProviderDescriptor` with `id = ProviderId::Cursor`, `display_name = "Cursor"`, `session_label = "Total"`, `weekly_label = "Auto"`, `opus_label = Some("API")`, `supports_opus = true`, `default_enabled = true`, `dashboard_url = "https://cursor.com/dashboard"`, `subscription_dashboard_url = "https://cursor.com/settings"`, `status_link_url = "https://status.cursor.com"`.
* `branding.rs` with the Cursor brand color (`#000000` foreground on white, hex `#0F172A` accent) and `icon_resource_name = "cursor"`. Drop the SVG into `apps/desktop-tauri/src/assets/icons/cursor.svg`.
* `models.rs` mirroring spec 42 section 1.5: `CursorUsageSummary`, `CursorAuthMe`, `CursorLegacyUsage` with `serde` derives. Monetary fields decode as `i64` cents.
* `strategies/` directory with `manual_cookie.rs`, `cached_cookie.rs`, `browser_strict.rs`, `browser_lenient.rs`, `webkit_session.rs`. Each is a `Strategy` impl returning either a usable cookie header plus a source label, or an enum variant signaling fall through.
* `fetch.rs` running the `/api/usage-summary`, `/api/auth/me`, and (only when the legacy plan is detected) `/api/usage?user=<sub>` calls.
* `mapping.rs` implementing the six rule precedence ladder for the headline percent.

A2. Windows cookie path translation, exact paths:

* Edge: `%LOCALAPPDATA%\Microsoft\Edge\User Data\<Profile>\Network\Cookies`
* Chrome: `%LOCALAPPDATA%\Google\Chrome\User Data\<Profile>\Network\Cookies`
* Chrome Beta: `%LOCALAPPDATA%\Google\Chrome Beta\User Data\<Profile>\Network\Cookies`
* Chrome Canary: `%LOCALAPPDATA%\Google\Chrome SxS\User Data\<Profile>\Network\Cookies`
* Brave: `%LOCALAPPDATA%\BraveSoftware\Brave-Browser\User Data\<Profile>\Network\Cookies`
* Vivaldi: `%LOCALAPPDATA%\Vivaldi\User Data\<Profile>\Network\Cookies`
* Arc: `%LOCALAPPDATA%\Arc\User Data\<Profile>\Network\Cookies`
* Opera: `%APPDATA%\Opera Software\Opera Stable\Network\Cookies`
* Firefox: `%APPDATA%\Mozilla\Firefox\Profiles\<profile>.default*\cookies.sqlite`

The `Network\Cookies` subfolder is mandatory on every Chromium variant since the v96 store split. The Phase 4 Chrome reader for Claude already paths through `Network\Cookies`; verify and, if needed, lift the constant into `rust/src/host/cookies/chromium_paths.rs` so Cursor and Factory share it.

A3. Six rule precedence ladder for the headline percent, implemented exactly:

1. `plan.totalPercentUsed` if present, clamped to 0 through 100
2. arithmetic mean of `plan.autoPercentUsed` and `plan.apiPercentUsed` when both are present
3. either `plan.autoPercentUsed` or `plan.apiPercentUsed` alone when only one is present
4. `plan.used / plan.limit` from cents
5. `overall.used / overall.limit`
6. `teamUsage.pooled.used / teamUsage.pooled.limit`

Rule 1 wins outright. Rules 4 through 6 only fire when the percent based rules cannot produce a number. Auto and API percents are already percent units even when fractional (`0.36` means 0.36 percent, not 36 percent), so they must not be multiplied by 100.

A4. Cents based USD conversion. The `providerCost` field divides on demand cents by 100 with two decimal precision. The `Monthly` period label comes from `billingCycleEnd`. The headline cost block is only emitted when `individualUsage.onDemand.enabled == true`.

A5. Legacy request based plan switch. When `/api/usage` returns `gpt-4.maxRequestUsage != null`, replace the percent path with a `cursorRequests` field carrying `numRequestsTotal` and `maxRequestUsage`. The card shows `used / limit` as integers in this case.

A6. Optional WebKit session import. Read `%APPDATA%\CodexBar\cursor-session.json` when present and use its cookies as a fifth source. The JSON shape mirrors the Mac `cursor-session.json` so users migrating from a Mac install can drop the file in. Document the import location in `docs/windows/migration/cursor.md`.

A7. Cursor settings panel in `apps/desktop-tauri/src/providers/cursor/SettingsPanel.tsx` with: cookie source picker (`auto`, `manual`, `off`), cookie header textarea, manage cookie cache button (clears `%APPDATA%\CodexBar\cache\cookie.cursor.bin`), token accounts table, dashboard link.

A8. Popup card. Add a `ProviderCard` entry rendering primary, secondary, tertiary windows. Primary is Total, secondary is Auto, tertiary is API. Reset string uses the `MMM d at h:mma` POSIX format and is generated server side in Rust to keep locale stable.

### B. Copilot provider

B1. A new module `rust/src/providers/copilot/` with the standard layout: `descriptor.rs`, `branding.rs`, `models.rs`, `fetch.rs`, `device_flow.rs`, `host_normalize.rs`, `mapping.rs`.

B2. GitHub device flow with VS Code client id `Iv1.b507a08c87ecfe98`. The constant lives in `device_flow.rs`. Do not extract it to settings, do not template it, do not derive it. GitHub blocks every other client id from `/copilot_internal` so a typo here is fatal at runtime. Keep the constant inline with a comment pointing at spec 42 section 2.2.

B3. Bot detector header set, copied verbatim from spec 42 section 2.2. Every request to `*.github.com` and `api.<host>` carries:

```
Authorization: token <github_oauth_token>
Accept: application/json
Editor-Version: vscode/1.96.2
Editor-Plugin-Version: copilot-chat/0.26.7
User-Agent: GitHubCopilotChat/0.26.7
X-Github-Api-Version: 2025-04-01
```

These values are pinned. Bumping `Editor-Version` or `Editor-Plugin-Version` requires a sanity test against the bot detector. Drop the values into a single `const HEADERS: &[(&str, &str); 5]` so a single grep finds every reference.

B4. Enterprise host normalization. Accept any of `octocorp.ghe.com`, `https://octocorp.ghe.com`, `https://octocorp.ghe.com/login`. Strip scheme, drop the first slash and everything after it, lowercase, trim leading and trailing dots. Empty input maps to `github.com`. The API host is `api.<normalized>` (or `api.github.com` for default). Unit test the normalizer with at least these inputs:

* `""` returns `("github.com", "api.github.com")`
* `"octocorp.ghe.com"` returns `("octocorp.ghe.com", "api.octocorp.ghe.com")`
* `"https://octocorp.ghe.com/login"` returns `("octocorp.ghe.com", "api.octocorp.ghe.com")`
* `"OCTOCORP.GHE.COM."` returns `("octocorp.ghe.com", "api.octocorp.ghe.com")`
* `"api.octocorp.ghe.com"` returns `("api.octocorp.ghe.com", "api.octocorp.ghe.com")`

B5. Percent derivation when `percent_remaining` is absent. When `quota_snapshots.premium_interactions` returns `entitlement > 0` and `remaining` without `percent_remaining`, derive `percent_remaining = clamp(0, 100, remaining / entitlement * 100)`. Apply the same rule to `chat`. The decoder must accept Int, Double, and stringified numbers. Tests cover all three shapes.

B6. Identity dedupe with `github:user:<id>`. After the device flow returns a token, call `GET https://api.github.com/user` and read `id`. Build the external identifier `github:user:<id>`. When updating, match existing token accounts by:

1. exact match on `external_identifier`
2. case insensitive match on legacy `login`-based identifiers
3. username prefix match as a final fallback

On any match outside path 1, rewrite the persisted `external_identifier` to the stable form.

B7. Chat only secondary handling. When `premium_interactions` is missing or zero entitlement but `chat` has data, leave `primary = None` and place chat in `secondary`. The popup card must render correctly with an empty primary.

B8. Token storage. Tokens are written to a DPAPI protected blob at `%APPDATA%\CodexBar\secrets\copilot.tok`. Multi-account entries live in `config.json` under `providers.copilot.tokenAccounts[]` with the token field DPAPI sealed per account. Never log token values; the existing log redactor must mask `token `, `Bearer `, and `gho_`, `ghu_`, `ghs_` prefixes.

B9. Device flow UI. A Tauri dialog with the `user_code`, a Copy button (clipboard), an Open Browser button (`tauri::api::shell::open` to `verification_uri` or `verification_uri_complete`), and a Cancel button. The cancel button cancels the async polling task via a `tokio::sync::CancellationToken`. Polling rules:

* sleep `interval` seconds
* on `authorization_pending` continue
* on `slow_down` add 5 seconds to the sleep (not the configured interval)
* on `expired_token` raise as `URLError.timedOut` analog
* on 200 parse `access_token` and exit

B10. Settings panel and popup card for Copilot. The settings panel includes the enterprise host field, an `Add account` button, and a re-auth button per account. The popup card shows `Premium` and `Chat` bars without reset timers (Copilot does not return per quota reset).

### C. Gemini provider

C1. A new module `rust/src/providers/gemini/` with: `descriptor.rs`, `branding.rs`, `models.rs`, `creds_file.rs`, `oauth_extractor.rs`, `quota_fetch.rs`, `tier_map.rs`, `bucket_group.rs`.

C2. Credential file. Read `%USERPROFILE%\.gemini\oauth_creds.json`. Decode shape:

```
{
  "access_token": "ya29...",
  "refresh_token": "1//0g...",
  "id_token": "<JWT>",
  "expiry_date": 1731000000000
}
```

`expiry_date` is milliseconds since epoch. Divide by 1000 to compare against `SystemTime::now()`. `refresh_token` is optional in the file. If `refresh_token` is missing and `access_token` is expired, raise `ProviderError::NotLoggedIn`.

C3. `~/.gemini/settings.json` is read at `%USERPROFILE%\.gemini\settings.json` and parsed to inspect `security.auth.selectedType`. If the value is `api-key` or `vertex-ai`, hard fail before any network call with a tip telling the user to switch the CLI to "Login with Google". Any other value (including missing) proceeds.

C4. OAuth client id and secret extraction. Google does not publish these. The extractor reads the bundled `gemini.js` to harvest the constants. The algorithm:

1. Resolve the `gemini` binary via three steps in order:
   * `PATH` walk using `std::env::var("PATH")` split on `;`, joined with each `PATHEXT` entry, looking for `gemini.cmd`, `gemini.exe`, or `gemini.bat`
   * The shared `BinaryLocator` from Phase 3 (`rust/src/host/binary_locator.rs`)
   * `where.exe gemini` subprocess as a last resort
2. Resolve symlinks. On Windows the equivalent is `std::fs::canonicalize`. This unwraps `scoop` shims, `npm` shims, and `fnm` wrappers.
3. Walk known Windows layouts in this order. The first `oauth2.js` that yields matches wins:
   * scoop: `%USERPROFILE%\scoop\apps\gemini-cli\current\resources\app\node_modules\@google\gemini-cli-core\dist\src\code_assist\oauth2.js`
   * npm global: `%APPDATA%\npm\node_modules\@google\gemini-cli\node_modules\@google\gemini-cli-core\dist\src\code_assist\oauth2.js`
   * Bun: `<binary parent>\..\gemini-cli-core\dist\src\code_assist\oauth2.js`
   * fnm: invoke `fnm exec --using <ver> npm root -g` (timeout 10 seconds), then `<root>\@google\gemini-cli\node_modules\@google\gemini-cli-core\dist\src\code_assist\oauth2.js`
   * winget package: `%LOCALAPPDATA%\Microsoft\WinGet\Packages\Google.GeminiCLI_*\` walk
4. If no direct hit, walk parents of the binary up to 8 ancestors, looking for `package.json` with `"name": "@google/gemini-cli"`. Once found, recurse into `node_modules\@google\gemini-cli-core\dist\src\code_assist\oauth2.js`.
5. If still nothing, walk the `bundle\` directory of the package starting at `bundle\gemini.js`. Follow `import` statements with the regex `r#"(?:import|export)[^'\"]*['"](\\./[^'\"]+\\.js)['\"]"#`, then sweep any remaining `.js` files in `bundle\`.
6. Regex extract the constants:

```
(?:const|let|var)?\s*OAUTH_CLIENT_ID\s*=\s*['"]([\w\-\.]+)['"]\s*;
(?:const|let|var)?\s*OAUTH_CLIENT_SECRET\s*=\s*['"]([\w\-]+)['"]\s*;
```

7. Cache the extracted pair in memory for the process lifetime. On extraction failure, raise `ProviderError::Misconfigured("Gemini CLI install not recognized")` with a clear message naming the four expected install layouts.

C5. Tier map, copied verbatim from spec 42 section 3.6:

| `currentTier.id` | `hd` claim present | Display label |
|------------------|--------------------|---------------|
| `standard-tier`  | any                | `Paid`        |
| `free-tier`      | yes                | `Workspace`   |
| `free-tier`      | no                 | `Free`        |
| `legacy-tier`    | any                | `Legacy`      |
| missing          | any                | empty string  |

C6. Bucket grouping. Group `buckets[]` by `modelId`, keep the minimum `remainingFraction` per model (input and output share the same model id), then sort into three buckets:

* `flash-lite`: any model id containing `flash-lite`
* `flash`: any model id containing `flash` and not `flash-lite`
* `pro`: any model id containing `pro`

For each bucket take the model with the lowest fraction. `usedPercent = (1 - frac) * 100`. `windowMinutes = 1440`. `resetsAt` comes from the bucket's `resetTime` ISO 8601 string (accept fractional and non fractional seconds). `resetDescription` is `Resets in Xh Ym` from now to `resetsAt`, never negative. Snapshot mapping:

* Pro group -> primary
* Flash group -> secondary
* Flash Lite group -> tertiary

C7. Refresh sequence. When `expiry_date < now`, run an OAuth refresh against `https://oauth2.googleapis.com/token` with form encoded body `client_id`, `client_secret`, `refresh_token`, `grant_type=refresh_token`. On 200, write the new `access_token` and `expiry_date = now_ms + expires_in * 1000` to `oauth_creds.json` atomically (write to `oauth_creds.json.tmp`, then rename). On 401, raise `ProviderError::NotLoggedIn`. The in memory access token is still usable for the current run even if the write fails.

C8. Identity. Parse the `id_token` JWT middle segment with base64url decode. `email` claim is `identity.account_email`. Presence of the `hd` claim flips the tier label between `Workspace` and `Free`.

C9. Login flow. When the credential file is missing, the popup offers a button labeled "Open Terminal and log in". The action launches `wt.exe new-tab "gemini"` if Windows Terminal is installed (resolve via `where.exe wt`); otherwise `cmd.exe /K gemini`. Before launching, delete `%USERPROFILE%\.gemini\oauth_creds.json` and `%USERPROFILE%\.gemini\google_accounts.json` to force a fresh OAuth flow. Use the `notify` crate to watch `%USERPROFILE%\.gemini\` for create events on `oauth_creds.json` (poll fallback every 1 second, max 5 minutes). On detection, sleep 500 milliseconds and trigger a refresh.

C10. Settings panel and popup card. Settings is minimal: enable toggle, "Open log in terminal" button, "Reset credentials" button, dashboard link to `https://aistudio.google.com/`. Card shows Pro, Flash, Flash Lite bars with reset timers.

### D. OpenRouter provider

D1. A new module `rust/src/providers/openrouter/` with: `descriptor.rs`, `branding.rs`, `models.rs`, `fetch.rs`, `mapping.rs`.

D2. Bearer auth. The single token comes from one of three sources in order:

1. `$OPENROUTER_API_KEY` environment variable
2. `config.providers.openrouter.apiKey`
3. `tokenAccounts` (multi-account)

The token is trimmed of surrounding single or double quotes before use. Tokens look like `sk-or-v1-<random>`.

D3. Two endpoints with a 1 second race timeout on the secondary call. The fetch dispatches both in parallel via `tokio::join!`, but `/key` runs against a `tokio::time::timeout(Duration::from_secs(1), ...)`. The primary `/credits` call uses a 15 second timeout. If `/key` times out or errors, the snapshot still publishes; `keyQuotaStatus` becomes `unavailable`.

Headers on both calls:

```
Authorization: Bearer <api_key>
Accept: application/json
HTTP-Referer: <$OPENROUTER_HTTP_REFERER, optional>
X-Title: <$OPENROUTER_X_TITLE, default "CodexBar">
```

Base URL defaults to `https://openrouter.ai/api/v1`. Override via `$OPENROUTER_API_URL`. Trim quotes; fall back to default if not parseable.

D4. `keyQuotaStatus` tri state:

* `available`: `/key` returned 200, `limit > 0`, `usage >= 0`
* `noLimitConfigured`: `/key` returned 200 but `limit` is null or `<= 0`
* `unavailable`: `/key` errored, timed out, or never ran

D5. Balance label format. `identity.login_method` renders as `Balance: $X.XX` with two decimal digits in `en_US` locale. Compute `balance = max(0, total_credits - total_usage)`. The card surface shows the same string at the top.

D6. Snapshot mapping:

| Snapshot field | Source |
|----------------|--------|
| `primary.used_percent` | `min(100, max(0, key_usage / key_limit * 100))` when `available`; else `None` |
| `window_minutes` | `None` |
| `resets_at` / `reset_description` | `None` |
| `identity.login_method` | `Balance: $X.XX` |
| `open_router_usage` | full block (totals, balance, key limit, key usage, rate limit) for the menu card |

D7. Log redaction. Add `sk-or-v1-` and `Bearer ` prefix matchers to `rust/src/host/log_redactor.rs`. Honor `$CODEXBAR_DEBUG_OPENROUTER_ERROR_BODIES=1` to include full debug bodies, still redacted.

D8. Settings panel. Single API key field, validation that strips quotes, an env override hint when `$OPENROUTER_API_KEY` is set (read only), "Get a key" link to `https://openrouter.ai/settings/keys`, and a dashboard link to `https://openrouter.ai/settings/credits`. There is no login flow.

### E. Factory provider (Droid)

E1. A new module `rust/src/providers/factory/` with: `descriptor.rs`, `branding.rs`, `models.rs`, `fetch.rs`, `sources/` (eight files, one per source), `workos.rs`, `local_storage.rs`, `flexible_date.rs`, `retry_409.rs`, `mapping.rs`.

E2. Eight ordered fetch sources, exactly per spec 42 section 5.1:

1. Manual cookie header from settings (`providers.factory.cookieHeader`)
2. Cached header from `%APPDATA%\CodexBar\cache\cookie.factory.bin`
3. Stored session cookies from `%APPDATA%\CodexBar\factory-session.json` -> `cookies[]`
4. Stored bearer from `factory-session.json` -> `bearerToken`
5. Stored WorkOS refresh token from `factory-session.json` -> `refreshToken`, minted via WorkOS
6. Local storage WorkOS tokens from Chromium leveldb byte scans (Edge, Chrome, Brave, Arc, Vivaldi, Helium)
7. Browser cookies for `factory.ai`, `app.factory.ai`, `auth.factory.ai`
8. WorkOS cookies for `workos.com` -> minted via WorkOS

Each success writes back to `factory-session.json`. `invalid_grant` from WorkOS clears the stored refresh token. `notLoggedIn` from Factory clears the stored cookies (but not the bearer or refresh).

E3. WorkOS dual client ids, tried in order:

```
client_01HXRMBQ9BJ3E7QSTQ9X2PHVB7
client_01HNM792M5G5G1A2THWPXKFMXB
```

The first 200 wins. Each call uses one of two body shapes:

Body A (refresh token):

```
{ "client_id":"<wos>", "grant_type":"refresh_token",
  "refresh_token":"<rt>", "organization_id":"<optional>" }
```

Body B (cookies only, paired with a `Cookie:` header carrying workos.com cookies):

```
{ "client_id":"<wos>", "grant_type":"refresh_token",
  "useCookie": true, "organization_id":"<optional>" }
```

Body C (organization_id only) is not emitted by the Mac source; do not invent it. `HTTP 400` with body containing `"missing refresh token"` -> `noSessionCookie` (caller clears refresh storage).

E4. Five step 409 retry chain on `/api/app/auth/me`:

1. Drop the `Authorization` header (cookies only)
2. Drop `access-token` and `__recent_auth` cookies
3. Drop `session` and `wos-session` cookies
4. Drop both (the union of steps 2 and 3)
5. Keep only `__Secure-authjs.session-token`, `authjs.session-token`, and `__Host-authjs.csrf-token`

Each retry returns early on `notLoggedIn` (cookies are not the problem). The original 409 is re thrown if every retry continues to 409.

E5. `FlexibleFactoryDate` decoding rules. Implement a `FactoryDate` newtype that decodes from any of:

* `Double` interpreted as seconds since epoch when `< 1e12`
* `Double` interpreted as milliseconds when `>= 1e12`
* `String` parsed as a numeric value (recursive)
* `String` parsed as ISO 8601 with optional fractional seconds
* `Null` returning `None`

Unit test all five paths.

E6. Legacy vs token rate limits payload mapping. Branch on `/api/billing/limits.usesTokenRateLimitsBilling`:

Legacy shape (`/api/organization/subscription/usage`):

* primary = standard `usedRatio` if in `[-0.001, 1.001]`, else recomputed from `userTokens / totalAllowance`
* secondary = premium, same rules
* `resetsAt` = `usage.endDate` (ms since epoch)
* identity = `auth.organization.name` with login method `Factory <tier> - <planName>`, de duplicated when plan name already contains "Factory"

Token rate limits shape (`/api/billing/limits`):

* primary = `standard.fiveHour` with `windowMinutes = 300`
* secondary = `standard.weekly` with `windowMinutes = 10080`
* tertiary = `standard.monthly` with `windowMinutes = None`
* `extraRateWindows` = core five hour, weekly, monthly when `core.hasUsageData`
* `providerCost` = `extraUsageBalanceCents / 100` USD with period label `Extra usage balance`
* identity login method gets `Fallback: <overagePreference>` appended

For each window:

* `resetsAt` prefers `secondsRemaining` (`now + secs`); otherwise `windowEnd.date` if it is in the future
* Stale but not rolling: if `windowEnd` is present, `secondsRemaining` is None, and `resetsAt` is None, force `usedPercent = 0` (matching the web UI)
* Otherwise clamp `usedPercent` to `[0, 100]`

E7. Local storage WorkOS extraction. For each Chromium variant, enumerate profile dirs (`Default`, `Profile *`, `user-*`), find `<profile>\Local Storage\leveldb`, and byte scan all `.ldb` and `.log` files for:

```
workos:refresh-token[^A-Za-z0-9_-]*([A-Za-z0-9_-]{20,})
workos:access-token[^A-Za-z0-9_-]*([A-Za-z0-9_-]{20,})
```

Never use a bundled leveldb library; that would lock the database. Use plain file IO. Supported browsers on Windows: Chrome, Chrome Beta, Chrome Canary, Edge, Edge Beta, Edge Dev, Brave, Arc, Vivaldi, Helium. Helium does not use `User Data` as the subroot; document its alternate root in `local_storage.rs`.

E8. Session cookie names (any one counts):

```
wos-session
__Secure-next-auth.session-token, next-auth.session-token
__Secure-authjs.session-token, authjs.session-token, __Host-authjs.csrf-token
session
access-token
```

`access-token` doubles as the bearer when its value contains a dot (JWT shape). Cookie domains scanned: `factory.ai`, `app.factory.ai`, `auth.factory.ai`.

E9. Base URL rotation. Probe `app.factory.ai`, `api.factory.ai`, `auth.factory.ai` in order, deduped and re ordered so `auth.factory.ai` is first when the cookie set has cookies on that domain.

E10. Required headers on every Factory call:

```
Accept: application/json
Content-Type: application/json
Origin: https://app.factory.ai
Referer: https://app.factory.ai/
x-factory-client: web-app
Cookie: <if present>
Authorization: Bearer <token>  // if present
```

`x-factory-client: web-app` is load bearing; Factory rejects without it.

E11. Login flow. The popup offers two buttons: "Open in browser" (default browser to `https://app.factory.ai`) and "Open in app window". The second uses a Tauri WebView2 child window with non persistent storage; on close, grab cookies via `webview.cookies()` filtered to `*.factory.ai` and write them to `factory-session.json`.

E12. Settings panel and popup card. Settings has a cookie source picker, cookie header textarea, token accounts table, "Clear session" button (deletes `factory-session.json` and `cookie.factory.bin`), and dashboard link to `https://app.factory.ai`. Card shows three bars with reset timers; an extra row for "Extra usage balance" when present.

### F. Shared cross cutting work

F1. Refactor the cookie pipeline used by Phase 4 Claude into a shared helper module `rust/src/host/cookies/pipeline.rs`. Expose:

```rust
pub trait CookieSource {
    fn label(&self) -> &'static str;
    fn try_fetch(&self, ctx: &CookieFetchContext) -> Result<Option<CookieHeader>, CookieError>;
}

pub struct CookiePipeline {
    sources: Vec<Box<dyn CookieSource>>,
}

impl CookiePipeline {
    pub fn run(&self, ctx: &CookieFetchContext) -> Result<CookieResult, CookieError> { ... }
}
```

The pipeline iterates sources in order. `notLoggedIn` falls through; network or parse errors bubble up. Cursor and Factory both consume this. Claude's existing single source path is converted to a pipeline with one source so the call site is uniform.

F2. OAuth refresh helper. Lift Claude's OAuth refresh from `rust/src/providers/claude/oauth.rs` into `rust/src/host/oauth/refresh.rs`. Expose:

```rust
pub struct RefreshRequest<'a> {
    pub token_url: &'a str,
    pub client_id: &'a str,
    pub client_secret: Option<&'a str>,
    pub refresh_token: &'a str,
    pub extra_form: &'a [(&'a str, &'a str)],
}

pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub expires_in: Duration,
    pub raw: serde_json::Value,
}

pub async fn refresh(req: &RefreshRequest<'_>) -> Result<RefreshResponse, OAuthError>;
```

Gemini calls it with Google's token endpoint and the extracted client id and secret. Factory calls it with WorkOS, two client ids, and the two body shapes. Claude's call site stays unchanged in behavior but is rewritten to consume the helper. Error mapping copied from spec 42 section 4.3.

F3. Per provider settings descriptors. Use the descriptor framework from Phase 3 (`SettingsDescriptor::Toggle`, `Field`, `Picker`, `Action`, `TokenAccounts`). The five new providers register their settings in `rust/src/providers/<id>/settings.rs`, returning a `Vec<SettingsDescriptor>`. The TS side renders them generically through `ProviderSettingsPanel.tsx`. No new TS components unless a provider needs a custom action (the device flow modal is the only one this phase introduces).

F4. Popup cards. The default `ProviderCard.tsx` covers Cursor, Gemini, Factory. Copilot needs a tweak for "no reset timer" state. OpenRouter needs a custom row for "Balance: $X.XX" and a `keyQuotaStatus` chip. Both customizations land in `apps/desktop-tauri/src/providers/<id>/Card.tsx` and are opted into via `ProviderUIOverrides`.

F5. Tray icon assets. Add SVGs in `apps/desktop-tauri/src/assets/icons/`:

* `cursor.svg`
* `copilot.svg`
* `gemini.svg`
* `openrouter.svg`
* `factory.svg`

Each is a single color glyph that adapts to dark and light tray backgrounds via CSS currentColor.

F6. Strings. Add display strings to `apps/desktop-tauri/src/locales/en.json` and the existing translation files (`de`, `pt-BR`). The key shape is `providers.<id>.<field>`. Phase 6 does not add new languages; the Brazilian Portuguese commit `22c44848` is the baseline.

### G. Tests

G1. One fixture based unit test per provider for the response parser:

* `rust/src/providers/cursor/tests/parse_usage_summary.rs` covering Pro, Hobby, Team, Enterprise, legacy gpt-4 request plan, team pool fallback, and a sample where `plan.totalPercentUsed` is `0.36` (already a percent).
* `rust/src/providers/copilot/tests/parse_user.rs` covering Int, Double, and stringified numbers; missing `percent_remaining`; chat only; placeholder all zero.
* `rust/src/providers/gemini/tests/parse_quota.rs` covering Pro plus Flash plus Flash Lite buckets, missing tier, project as string vs object.
* `rust/src/providers/openrouter/tests/parse_credits.rs` covering `/credits` and `/key` happy paths, `noLimitConfigured`, `/key` 500, and zero total credits.
* `rust/src/providers/factory/tests/parse_billing.rs` covering legacy shape, token rate limits shape, both with stale window detection, and the `FlexibleFactoryDate` decoder.

Fixtures live under `rust/src/providers/<id>/tests/fixtures/*.json`. Each fixture is a real (redacted) response captured during exploratory testing.

G2. Integration tests, best effort live. One per provider, driven by a real account. These are gated behind `#[ignore]` and run manually by the engineer with environment variables set:

* `cursor_live`: requires `CURSOR_COOKIE_HEADER`. Runs the full fetch, asserts non zero `primary.used_percent` exists for any plan.
* `copilot_live`: requires `COPILOT_API_TOKEN`. Runs `/copilot_internal/user` plus `/user`, asserts identity has a stable `github:user:<id>`.
* `gemini_live`: requires `%USERPROFILE%\.gemini\oauth_creds.json` present. Runs the full path including OAuth refresh and bucket grouping.
* `openrouter_live`: requires `OPENROUTER_API_KEY`. Asserts both `/credits` and `/key` return parseable shapes within timeout.
* `factory_live`: requires `FACTORY_COOKIE_HEADER` or a populated `factory-session.json`. Asserts the eight source pipeline returns at least one snapshot path.

The integration target lives at `rust/tests/providers_live.rs` with one `#[ignore] #[test]` per provider. Document the run command in `docs/windows/dev/integration-tests.md`.

G3. UI smoke. A Playwright test in `apps/desktop-tauri/tests/popup-cards.spec.ts` opens the popup with mocked IPC responses and verifies all seven v1 provider cards render without console errors. The mock fixtures are the same JSON files used by the Rust parser tests, plus a `UsageSnapshotDTO` wrapper.

## Atomic commit tasks

Each commit below is a single atomic change. Each lists files touched, acceptance check, and the draft commit message. Commits follow conventional commit format with the scope being the provider id or the shared subsystem. Run the matching unit tests before pushing. Push after each commit.

### Cursor (provider sub group)

**Commit A1: feat(cursor): scaffold provider module and descriptor**

Files:
* `rust/src/providers/cursor/mod.rs`
* `rust/src/providers/cursor/descriptor.rs`
* `rust/src/providers/cursor/branding.rs`
* `rust/src/providers/cursor/registration.rs`
* `rust/src/providers/mod.rs` (re export)
* `apps/desktop-tauri/src/assets/icons/cursor.svg`

Acceptance:
* `cargo test -p codexbar providers::registry::validation` passes (registry validates the new descriptor)
* `cargo build` succeeds
* The popup shows a disabled Cursor card with the brand icon when launched

Draft commit message:

```
feat(cursor): scaffold provider descriptor and branding

Add the Cursor descriptor, branding, and inventory registration so the
provider shows up in the descriptor catalog. No fetch path yet.
```

**Commit A2: feat(cursor): response models for usage summary, auth me, legacy usage**

Files:
* `rust/src/providers/cursor/models.rs`
* `rust/src/providers/cursor/tests/fixtures/usage_summary_pro.json`
* `rust/src/providers/cursor/tests/fixtures/usage_summary_team.json`
* `rust/src/providers/cursor/tests/fixtures/usage_summary_legacy.json`
* `rust/src/providers/cursor/tests/parse_models.rs`

Acceptance:
* `cargo test -p codexbar providers::cursor::tests::parse_models` passes
* Cents fields decode as `i64`
* `billingCycleEnd` decodes with and without fractional seconds

Draft commit message:

```
feat(cursor): add response models and fixture parser tests

Decode usage-summary, auth/me, and legacy /api/usage payloads with
cents as i64 and ISO 8601 with or without fractional seconds.
```

**Commit A3: feat(cursor): six rule precedence ladder for headline percent**

Files:
* `rust/src/providers/cursor/mapping.rs`
* `rust/src/providers/cursor/tests/precedence.rs`

Acceptance:
* `cargo test -p codexbar providers::cursor::tests::precedence` passes for all six rules
* Rule 1 with `0.36` returns `0.36`, not `36.0`
* Rule 2 with auto `0.5` and api `1.5` returns `1.0`

Draft commit message:

```
feat(cursor): compute headline percent via six rule precedence

Implements plan.totalPercentUsed first, then mean of auto and api,
then either alone, then cents from plan, overall, and team pool.
```

**Commit A4: feat(cursor): strategy implementations for cookie sources**

Files:
* `rust/src/providers/cursor/strategies/manual_cookie.rs`
* `rust/src/providers/cursor/strategies/cached_cookie.rs`
* `rust/src/providers/cursor/strategies/browser_strict.rs`
* `rust/src/providers/cursor/strategies/browser_lenient.rs`
* `rust/src/providers/cursor/strategies/webkit_session.rs`

Acceptance:
* Each strategy compiles against the shared `CookieSource` trait from F1
* Unit tests for `webkit_session` parse a sample `cursor-session.json`
* Manual strategy strips both `"..."` and `'...'` wrappers

Draft commit message:

```
feat(cursor): wire five cookie sources into the shared pipeline

Manual header, cached blob, browser strict, browser lenient, and
WebKit session import. Each conforms to CookieSource from host/cookies.
```

**Commit A5: feat(cursor): fetch path, identity, and snapshot mapping**

Files:
* `rust/src/providers/cursor/fetch.rs`
* `rust/src/providers/cursor/mapping.rs` (extended)
* `rust/src/providers/cursor/tests/fetch_smoke.rs`

Acceptance:
* `cargo test -p codexbar providers::cursor::tests::fetch_smoke` passes against fixture HTTP server
* `providerCost` divides on demand cents by 100
* Legacy request plan switch triggers when `gpt-4.maxRequestUsage != null`

Draft commit message:

```
feat(cursor): full fetch and snapshot mapping

Run /api/usage-summary, /api/auth/me, and conditionally /api/usage.
Convert cents to USD, derive resetsAt and POSIX reset description.
```

**Commit A6: feat(cursor): settings panel and popup card**

Files:
* `rust/src/providers/cursor/settings.rs`
* `apps/desktop-tauri/src/providers/cursor/index.tsx`
* `apps/desktop-tauri/src/providers/cursor/SettingsPanel.tsx`
* `apps/desktop-tauri/src/locales/en.json` (cursor keys)
* `apps/desktop-tauri/src/locales/de.json`
* `apps/desktop-tauri/src/locales/pt-BR.json`

Acceptance:
* The Cursor card renders with three bars when a real cookie is present
* The settings panel saves cookie source, cookie header, and token accounts to `config.json`
* Strings localize across `en`, `de`, `pt-BR`

Draft commit message:

```
feat(cursor): settings panel and popup card

Cookie source picker, cookie header textarea, token accounts table,
manage cache button, dashboard link, and the standard three bar card.
```

### Copilot (provider sub group)

**Commit B1: feat(copilot): scaffold descriptor, branding, and models**

Files:
* `rust/src/providers/copilot/mod.rs`
* `rust/src/providers/copilot/descriptor.rs`
* `rust/src/providers/copilot/branding.rs`
* `rust/src/providers/copilot/models.rs`
* `rust/src/providers/copilot/registration.rs`
* `apps/desktop-tauri/src/assets/icons/copilot.svg`
* `rust/src/providers/copilot/tests/fixtures/user_business.json`
* `rust/src/providers/copilot/tests/fixtures/user_chat_only.json`
* `rust/src/providers/copilot/tests/parse_models.rs`

Acceptance:
* Decoder accepts Int, Double, and string numbers for `entitlement` and `remaining`
* Missing `percent_remaining` derives percent from `100 - remaining / entitlement * 100`
* Chat only fixture parses with `primary = None` and `secondary` populated

Draft commit message:

```
feat(copilot): scaffold descriptor and tolerant response models

Accept Int, Double, and string numbers in quota snapshots. Derive
percent_remaining when missing. Cover chat only and full responses.
```

**Commit B2: feat(copilot): enterprise host normalization**

Files:
* `rust/src/providers/copilot/host_normalize.rs`
* `rust/src/providers/copilot/tests/normalize.rs`

Acceptance:
* All five normalization cases from section B4 pass
* Empty input maps to `(github.com, api.github.com)`
* Already prefixed `api.octocorp.ghe.com` is preserved

Draft commit message:

```
feat(copilot): normalize enterprise hosts for API and login

Strip scheme, drop path, lowercase, trim dots. Build api.<host> for
API calls. Empty input falls back to github.com.
```

**Commit B3: feat(copilot): device flow with VS Code client id**

Files:
* `rust/src/providers/copilot/device_flow.rs`
* `rust/src/providers/copilot/tests/device_flow.rs`

Acceptance:
* The client id constant `Iv1.b507a08c87ecfe98` is referenced exactly once
* Polling handles `authorization_pending`, `slow_down` (adds 5 seconds, not the configured interval), `expired_token`, and 200
* Cancellation via `CancellationToken` aborts the polling task within 100 milliseconds

Draft commit message:

```
feat(copilot): GitHub device flow with VS Code client id

Pin Iv1.b507a08c87ecfe98 as the only working client id. Tolerate
authorization_pending and slow_down. Wire cancellation token.
```

**Commit B4: feat(copilot): fetch path with bot detector header set**

Files:
* `rust/src/providers/copilot/fetch.rs`
* `rust/src/providers/copilot/tests/headers.rs`

Acceptance:
* Every outbound request carries the six pinned headers from B3 above
* Header values are pinned in a single `const` slice
* Header test rejects any mutation of `Editor-Version`, `Editor-Plugin-Version`, `User-Agent`, `X-Github-Api-Version`

Draft commit message:

```
feat(copilot): outgoing request headers pin VS Code identity

Editor-Version vscode/1.96.2, Editor-Plugin-Version copilot-chat
0.26.7, UA GitHubCopilotChat/0.26.7, X-Github-Api-Version
2025-04-01. Tested against a header drift guard.
```

**Commit B5: feat(copilot): identity dedupe via github:user:<id>**

Files:
* `rust/src/providers/copilot/identity.rs`
* `rust/src/providers/copilot/tests/identity.rs`

Acceptance:
* Exact `external_identifier` match wins
* Legacy `login` based match is case insensitive and rewrites to the stable form
* Username prefix is the last fallback

Draft commit message:

```
feat(copilot): dedupe token accounts on github:user:<id>

Match exact identifier, then legacy login case insensitively, then
username prefix. Rewrite legacy identifiers on first match.
```

**Commit B6: feat(copilot): mapping plus settings panel and popup card**

Files:
* `rust/src/providers/copilot/mapping.rs`
* `rust/src/providers/copilot/settings.rs`
* `apps/desktop-tauri/src/providers/copilot/index.tsx`
* `apps/desktop-tauri/src/providers/copilot/Card.tsx` (no reset timer)
* `apps/desktop-tauri/src/providers/copilot/DeviceFlowDialog.tsx`
* `apps/desktop-tauri/src/locales/en.json`, `de.json`, `pt-BR.json`

Acceptance:
* Card renders without reset timers
* Device flow dialog copies the user code to clipboard and opens the browser
* Cancel cancels the polling task and leaves no orphan tokens

Draft commit message:

```
feat(copilot): mapping, settings panel, and device flow UI

Map premium and chat quota into primary and secondary. Add Tauri
device flow dialog with clipboard copy and cancel. Drop reset timers.
```

### Gemini (provider sub group)

**Commit C1: feat(gemini): scaffold descriptor, branding, models**

Files:
* `rust/src/providers/gemini/mod.rs`
* `rust/src/providers/gemini/descriptor.rs`
* `rust/src/providers/gemini/branding.rs`
* `rust/src/providers/gemini/models.rs`
* `rust/src/providers/gemini/registration.rs`
* `apps/desktop-tauri/src/assets/icons/gemini.svg`

Acceptance:
* Provider appears in `provider_descriptors` IPC response
* Card shows in popup as disabled until credentials are detected

Draft commit message:

```
feat(gemini): scaffold descriptor, branding, and response models

Add the Gemini provider scaffold with the brand SVG, descriptor, and
serde models for oauth_creds.json and the quota response.
```

**Commit C2: feat(gemini): credential file and settings.json reader**

Files:
* `rust/src/providers/gemini/creds_file.rs`
* `rust/src/providers/gemini/tests/creds.rs`
* `rust/src/providers/gemini/tests/fixtures/oauth_creds_valid.json`
* `rust/src/providers/gemini/tests/fixtures/oauth_creds_no_refresh.json`
* `rust/src/providers/gemini/tests/fixtures/settings_api_key.json`

Acceptance:
* `%USERPROFILE%\.gemini\oauth_creds.json` reader returns `NotLoggedIn` when refresh is missing and access is expired
* `settings.json` with `auth.selectedType = api-key` short circuits to a misconfigured error before any network call

Draft commit message:

```
feat(gemini): read oauth_creds.json and settings.json

Parse ms epoch expiry, route api-key and vertex-ai selections to
clear error states, and surface NotLoggedIn when refresh is missing.
```

**Commit C3: feat(gemini): OAuth client id and secret extractor**

Files:
* `rust/src/providers/gemini/oauth_extractor.rs`
* `rust/src/providers/gemini/tests/extractor.rs`
* `rust/src/providers/gemini/tests/fixtures/oauth2.js`
* `rust/src/providers/gemini/tests/fixtures/bundle/gemini.js`
* `rust/src/providers/gemini/tests/fixtures/bundle/sub/code_assist.js`

Acceptance:
* Direct hit on each known layout (scoop, npm global, Bun sibling, fnm via shim)
* Package root walker hits the npm layout from a fixture
* Bundle walker follows `import` statements through two levels of fixtures
* In memory cache returns the same pair on repeat call without re reading

Draft commit message:

```
feat(gemini): extract OAuth client id and secret from gemini.js

Walk scoop, winget, npm global, Bun, fnm layouts, then package root,
then bundle/gemini.js with import follow. Cache the result in memory.
```

**Commit C4: feat(gemini): OAuth refresh and quota fetch**

Files:
* `rust/src/providers/gemini/quota_fetch.rs`
* `rust/src/providers/gemini/tests/quota.rs`
* `rust/src/providers/gemini/tests/fixtures/quota_pro_flash.json`
* `rust/src/providers/gemini/tests/fixtures/load_code_assist.json`

Acceptance:
* Token refresh writes `oauth_creds.json` atomically (tmp file plus rename)
* In memory access token still works when the write fails
* `loadCodeAssist` `cloudaicompanionProject` decodes as string or object
* Project discovery fallback picks `gen-lang-client*` or a label match

Draft commit message:

```
feat(gemini): refresh OAuth and fetch retrieveUserQuota

Use the shared OAuth refresh helper, write oauth_creds.json
atomically, and tolerate project as string or object.
```

**Commit C5: feat(gemini): tier mapping and Pro/Flash/Flash-Lite grouping**

Files:
* `rust/src/providers/gemini/tier_map.rs`
* `rust/src/providers/gemini/bucket_group.rs`
* `rust/src/providers/gemini/mapping.rs`
* `rust/src/providers/gemini/tests/bucket.rs`

Acceptance:
* `standard-tier` returns `Paid`
* `free-tier` with `hd` claim returns `Workspace`; without, `Free`
* `legacy-tier` returns `Legacy`
* Group by model id, lowest fraction wins, three buckets land in primary, secondary, tertiary
* `windowMinutes = 1440` on every bucket

Draft commit message:

```
feat(gemini): tier labels and bucket grouping by model family

Map standard, free, legacy tiers. Group buckets into Pro, Flash,
and Flash Lite. Lowest fraction wins, 1440 minute windows.
```

**Commit C6: feat(gemini): login flow watcher and settings panel**

Files:
* `rust/src/providers/gemini/login_flow.rs`
* `rust/src/providers/gemini/settings.rs`
* `apps/desktop-tauri/src/providers/gemini/index.tsx`
* `apps/desktop-tauri/src/providers/gemini/SettingsPanel.tsx`
* `apps/desktop-tauri/src/locales/en.json`, `de.json`, `pt-BR.json`

Acceptance:
* Login button launches `wt.exe new-tab gemini` when Windows Terminal is installed; else `cmd.exe /K gemini`
* Pre launch deletes `oauth_creds.json` and `google_accounts.json`
* `notify` watcher (or 1 second poll fallback for 5 minutes) triggers a refresh on credential write

Draft commit message:

```
feat(gemini): launch terminal login flow and watch credentials

Use wt.exe when installed, cmd.exe fallback. Delete stale creds
before launch, then watch %USERPROFILE%\.gemini\ for the new file.
```

### OpenRouter (provider sub group)

**Commit D1: feat(openrouter): scaffold descriptor, branding, models**

Files:
* `rust/src/providers/openrouter/mod.rs`
* `rust/src/providers/openrouter/descriptor.rs`
* `rust/src/providers/openrouter/branding.rs`
* `rust/src/providers/openrouter/models.rs`
* `rust/src/providers/openrouter/registration.rs`
* `apps/desktop-tauri/src/assets/icons/openrouter.svg`

Acceptance:
* Provider appears in IPC catalog
* Card scaffold renders with the balance row placeholder

Draft commit message:

```
feat(openrouter): scaffold descriptor, branding, and response models

Decode /credits and /key responses with serde. Add the brand SVG
and register the provider in the inventory catalog.
```

**Commit D2: feat(openrouter): fetch path with 1 second race timeout**

Files:
* `rust/src/providers/openrouter/fetch.rs`
* `rust/src/providers/openrouter/tests/fetch.rs`

Acceptance:
* `/credits` and `/key` run in parallel via `tokio::join!`
* `/key` is wrapped in `tokio::time::timeout(1s, ...)`; slow `/key` does not block the primary
* Base URL override via `$OPENROUTER_API_URL` is applied, with quote stripping
* `HTTP-Referer` and `X-Title` headers honored

Draft commit message:

```
feat(openrouter): fetch /credits and /key with 1 second race

Race /key on a 1 second timeout so a slow secondary never blocks
the primary credits meter. Apply env var overrides for URL and titles.
```

**Commit D3: feat(openrouter): mapping plus keyQuotaStatus tri state**

Files:
* `rust/src/providers/openrouter/mapping.rs`
* `rust/src/providers/openrouter/tests/mapping.rs`

Acceptance:
* `available`, `noLimitConfigured`, and `unavailable` are distinguishable in the snapshot
* Balance string format is exactly `Balance: $X.XX`
* `keyUsedPercent` clamps to `[0, 100]`

Draft commit message:

```
feat(openrouter): map balance and key quota tri state

Compute balance from total credits minus total usage. Surface
keyQuotaStatus as available, noLimitConfigured, or unavailable.
```

**Commit D4: feat(openrouter): log redaction for sk-or-v1 and Bearer**

Files:
* `rust/src/host/log_redactor.rs`
* `rust/src/host/tests/redactor.rs`

Acceptance:
* `sk-or-v1-<random>` patterns redact in tracing output
* `Bearer <token>` redacts on every header line
* `CODEXBAR_DEBUG_OPENROUTER_ERROR_BODIES=1` includes redacted body in trace

Draft commit message:

```
feat(openrouter): redact sk-or-v1 and Bearer in logs

Extend the host log redactor to cover OpenRouter key prefixes and
Bearer auth headers. Honor the debug error bodies env override.
```

**Commit D5: feat(openrouter): settings panel and popup card**

Files:
* `rust/src/providers/openrouter/settings.rs`
* `apps/desktop-tauri/src/providers/openrouter/index.tsx`
* `apps/desktop-tauri/src/providers/openrouter/Card.tsx`
* `apps/desktop-tauri/src/providers/openrouter/SettingsPanel.tsx`
* `apps/desktop-tauri/src/locales/en.json`, `de.json`, `pt-BR.json`

Acceptance:
* Card shows balance, key meter when available, status chip otherwise
* Settings strips surrounding quotes from the API key
* Env var override is visible (read only) when present

Draft commit message:

```
feat(openrouter): settings panel and balance focused card

Render balance string, optional key meter, and a keyQuotaStatus chip.
Strip quotes from API key input. Show env override read only.
```

### Factory (provider sub group)

**Commit E1: feat(factory): scaffold descriptor, branding, response models**

Files:
* `rust/src/providers/factory/mod.rs`
* `rust/src/providers/factory/descriptor.rs`
* `rust/src/providers/factory/branding.rs`
* `rust/src/providers/factory/models.rs`
* `rust/src/providers/factory/registration.rs`
* `apps/desktop-tauri/src/assets/icons/factory.svg`

Acceptance:
* Provider appears in IPC catalog
* Models decode legacy and token rate limit shapes with serde

Draft commit message:

```
feat(factory): scaffold descriptor, branding, and dual shape models

Add the Factory provider scaffold and serde models for the legacy
subscription usage payload plus the new token rate limits payload.
```

**Commit E2: feat(factory): FlexibleFactoryDate decoder**

Files:
* `rust/src/providers/factory/flexible_date.rs`
* `rust/src/providers/factory/tests/flexible_date.rs`

Acceptance:
* All five paths from spec 42 section 5.9 decode (seconds, ms, numeric string, ISO 8601, null)
* `< 1e12` is seconds, `>= 1e12` is milliseconds

Draft commit message:

```
feat(factory): flexible date decoder for windowEnd and endDate

Decode Double seconds, Double ms, numeric strings, ISO 8601 with
or without fractional seconds, and null. Cover every path.
```

**Commit E3: feat(factory): WorkOS token minting**

Files:
* `rust/src/providers/factory/workos.rs`
* `rust/src/providers/factory/tests/workos.rs`

Acceptance:
* Body shape A (refresh token) sends `client_id`, `grant_type`, `refresh_token`, and optional `organization_id`
* Body shape B (useCookie) sends `useCookie: true` plus a `Cookie:` header
* Both WorkOS client ids are tried in order
* `400` with `"missing refresh token"` returns `noSessionCookie`

Draft commit message:

```
feat(factory): mint WorkOS tokens with dual client ids

Try client_01HXRMBQ9BJ3E7QSTQ9X2PHVB7 then
client_01HNM792M5G5G1A2THWPXKFMXB. Support refresh and useCookie
body shapes. Map missing refresh token to noSessionCookie.
```

**Commit E4: feat(factory): local storage WorkOS extraction**

Files:
* `rust/src/providers/factory/local_storage.rs`
* `rust/src/providers/factory/tests/local_storage.rs`
* `rust/src/providers/factory/tests/fixtures/leveldb_sample.ldb`

Acceptance:
* The byte scanner extracts `workos:refresh-token` and `workos:access-token` values longer than 20 chars
* Chromium variants enumerate `Default`, `Profile *`, `user-*`
* Helium's alternate root is supported
* No leveldb library is linked

Draft commit message:

```
feat(factory): byte scan Chromium leveldb for WorkOS tokens

Scan .ldb and .log files for workos:refresh-token and
workos:access-token markers without locking the database.
```

**Commit E5: feat(factory): eight source fetch pipeline**

Files:
* `rust/src/providers/factory/sources/manual.rs`
* `rust/src/providers/factory/sources/cached.rs`
* `rust/src/providers/factory/sources/session_cookies.rs`
* `rust/src/providers/factory/sources/session_bearer.rs`
* `rust/src/providers/factory/sources/session_refresh.rs`
* `rust/src/providers/factory/sources/local_storage.rs`
* `rust/src/providers/factory/sources/browser_cookies.rs`
* `rust/src/providers/factory/sources/workos_cookies.rs`
* `rust/src/providers/factory/fetch.rs`

Acceptance:
* All eight sources fire in order
* Each success writes back to `factory-session.json`
* `invalid_grant` clears the stored refresh token
* `notLoggedIn` from Factory clears the stored cookies but not the bearer

Draft commit message:

```
feat(factory): eight source fetch pipeline with write back

Order: manual, cached, stored cookies, stored bearer, stored
refresh, local storage, browser cookies, WorkOS cookies. Each
success caches its state into factory-session.json.
```

**Commit E6: feat(factory): 409 retry chain on /api/app/auth/me**

Files:
* `rust/src/providers/factory/retry_409.rs`
* `rust/src/providers/factory/tests/retry_409.rs`

Acceptance:
* Five retries in the order spec 42 section 5.8 prescribes
* `notLoggedIn` short circuits the chain
* The original 409 re raises after every retry continues to 409

Draft commit message:

```
feat(factory): five step 409 retry chain for stale tokens

Drop Authorization, then access-token and __recent_auth, then
session and wos-session, then both, then keep only authjs cookies.
Bail on notLoggedIn; re raise the 409 if every step fails.
```

**Commit E7: feat(factory): legacy and token rate limit snapshot mapping**

Files:
* `rust/src/providers/factory/mapping.rs`
* `rust/src/providers/factory/tests/mapping.rs`

Acceptance:
* Legacy `usedRatio` rules cover fraction, percent, and recomputed paths
* Token rate limit shape maps `fiveHour`, `weekly`, `monthly` into primary, secondary, tertiary
* Stale but not rolling forces `usedPercent = 0`
* `extraUsageBalanceCents / 100` lands in `providerCost`
* Identity login method de duplicates "Factory" when the plan name already contains it

Draft commit message:

```
feat(factory): map both legacy and token rate limit payloads

Branch on usesTokenRateLimitsBilling. Map five hour, weekly,
monthly to primary, secondary, tertiary. Handle stale window
detection. Format identity without duplicating "Factory".
```

**Commit E8: feat(factory): settings panel, popup card, and WebView2 login**

Files:
* `rust/src/providers/factory/settings.rs`
* `apps/desktop-tauri/src/providers/factory/index.tsx`
* `apps/desktop-tauri/src/providers/factory/Card.tsx`
* `apps/desktop-tauri/src/providers/factory/SettingsPanel.tsx`
* `apps/desktop-tauri/src/providers/factory/LoginWindow.tsx`
* `apps/desktop-tauri/src/locales/en.json`, `de.json`, `pt-BR.json`

Acceptance:
* "Open in app window" launches a Tauri WebView2 child window on `https://app.factory.ai`
* On close, cookies for `*.factory.ai` are captured and written to `factory-session.json`
* Card renders three bars plus an Extra usage balance row when present

Draft commit message:

```
feat(factory): settings panel, card, and in app WebView2 login

Add the cookie source picker, token accounts table, clear session
button, and the WebView2 login window with cookie capture on close.
```

### Shared cross cutting (commits)

**Commit F1: refactor(host): extract shared cookie pipeline**

Files:
* `rust/src/host/cookies/pipeline.rs`
* `rust/src/host/cookies/mod.rs`
* `rust/src/providers/claude/cookie_source.rs` (rewritten to consume the pipeline)
* `rust/src/host/cookies/tests/pipeline.rs`

Acceptance:
* Claude's existing cookie path passes the same tests it did before
* `CookieSource` trait, `CookiePipeline`, and `CookieFetchContext` types compile and pass `cargo test`
* The pipeline propagates parse errors and falls through on `notLoggedIn`

Draft commit message:

```
refactor(host): extract shared cookie pipeline from Claude

Lift the Phase 4 Claude cookie path into host/cookies/pipeline.rs.
Cursor and Factory now reuse it. No behavior change for Claude.
```

**Commit F2: refactor(host): shared OAuth refresh helper**

Files:
* `rust/src/host/oauth/refresh.rs`
* `rust/src/host/oauth/mod.rs`
* `rust/src/providers/claude/oauth.rs` (rewritten to consume the helper)
* `rust/src/host/oauth/tests/refresh.rs`

Acceptance:
* Claude OAuth refresh passes the same tests it did before
* Gemini and Factory can construct a `RefreshRequest` with their respective endpoints, client ids, and extra form fields
* Error mapping for `invalid_grant`, `unauthorized_client`, other 4xx, and network failures

Draft commit message:

```
refactor(host): generalize OAuth refresh from Claude

Lift the Phase 4 Claude refresh into host/oauth/refresh.rs. Gemini
and Factory consume the helper with provider specific endpoints.
```

**Commit F3: chore(providers): cookie path constants for Chromium**

Files:
* `rust/src/host/cookies/chromium_paths.rs`
* `rust/src/host/cookies/tests/paths.rs`

Acceptance:
* Edge, Chrome, Chrome Beta, Chrome Canary, Brave, Vivaldi, Arc, Opera, Helium paths return correctly for `Default` and named profiles
* Every path includes the `Network` subfolder where required
* Firefox path resolves the `.default*` profile glob

Draft commit message:

```
chore(providers): centralize Chromium cookie paths

Move every variant of the Network\Cookies path into one module so
Cursor, Factory, and Claude share the same lookup table.
```

**Commit F4: chore(strings): wire localization for the five new providers**

Files:
* `apps/desktop-tauri/src/locales/en.json`
* `apps/desktop-tauri/src/locales/de.json`
* `apps/desktop-tauri/src/locales/pt-BR.json`

Acceptance:
* No untranslated keys remain after the five provider PRs
* The Brazilian Portuguese baseline from commit `22c44848` extends without conflicts

Draft commit message:

```
chore(strings): add en, de, and pt-BR strings for the cohort

Localize provider display names, descriptions, settings labels, and
error messages for Cursor, Copilot, Gemini, OpenRouter, Factory.
```

**Commit F5: test(providers): integration test stubs (ignored by default)**

Files:
* `rust/tests/providers_live.rs`
* `docs/windows/dev/integration-tests.md`

Acceptance:
* Each `#[test] #[ignore]` runs against real credentials when invoked with `cargo test --test providers_live -- --ignored`
* The doc lists env vars per provider and the expected pass criteria

Draft commit message:

```
test(providers): add ignored live integration tests

One ignored test per provider that exercises the full fetch path
with real credentials. Document the env vars and run command.
```

**Commit F6: test(ui): popup smoke for the seven v1 cards**

Files:
* `apps/desktop-tauri/tests/popup-cards.spec.ts`
* `apps/desktop-tauri/tests/fixtures/*.json`

Acceptance:
* The Playwright run loads the popup with mocked IPC and asserts all seven cards mount without console errors
* Fixtures reuse the Rust parser fixtures so they cannot drift independently

Draft commit message:

```
test(ui): popup smoke covers all seven v1 provider cards

Mock the provider_descriptors and provider_snapshots IPC, render
the popup, and assert seven cards mount with no console errors.
```

## Per provider sub acceptance tests

These tests run on a developer machine and must pass before the phase is considered done. Each is the smallest test that proves the provider works end to end.

### Cursor

* C-S-01: Import cookies from Edge (`Default` profile) and render Cursor card with non zero `Total` percent.
* C-S-02: Import cookies from Chrome with a non default profile name (`Profile 1`). Same assertion.
* C-S-03: Manual paste of a `Cookie: ...` header in settings overrides browser cookies. Same assertion.
* C-S-04: Switch a Pro account into "Legacy plan" via account settings, refresh, observe `cursorRequests` populated and the card switching to `requests used / requests limit`.
* C-S-05: Open the popup with a Team plan account; the headline rounds within plus or minus 1 percent of the Cursor web "Total" tile.
* C-S-06: Force a 401 (paste a malformed cookie). The cache blob at `cookie.cursor.bin` is deleted and the card flips to "Not logged in".
* C-S-07: WebKit session import: drop a copy of a Mac `cursor-session.json` into `%APPDATA%\CodexBar\` and confirm the card lights up.

### Copilot

* P-S-01: Run the device flow against `github.com` against a developer GitHub account. The user code copies to clipboard and the dialog closes on success.
* P-S-02: Configure an enterprise host (`octocorp.ghe.com`), restart the device flow, complete it. Snapshot fetch hits `api.octocorp.ghe.com`.
* P-S-03: Cancel the device flow mid poll. The polling task aborts within 100 milliseconds and no token saves.
* P-S-04: Force GitHub to return `slow_down` via test harness. The next poll waits 5 seconds longer than `interval`.
* P-S-05: Save a token then re run the device flow; verify the existing account is rewritten with `github:user:<id>` as the external identifier and not duplicated.
* P-S-06: Account on the Chat only quota renders the card with no primary bar and a Chat secondary.
* P-S-07: Mutate `Editor-Version` to `vscode/0.1.0` in a test header and confirm the bot detector test fails (this is a deliberate guard).

### Gemini

* G-S-01: First launch detects a fresh Gemini CLI install via scoop and extracts the OAuth client id and secret on first call.
* G-S-02: An npm global install at `%APPDATA%\npm\node_modules\@google\gemini-cli` is also detected.
* G-S-03: An `fnm` managed install is detected via the `fnm exec --using <ver> npm root -g` fallback.
* G-S-04: The bundle walker resolves a `bundle/gemini.js` install when no `oauth2.js` is found directly.
* G-S-05: Setting `auth.selectedType = api-key` in `settings.json` short circuits the fetch with a clear error.
* G-S-06: Expired `access_token` triggers a refresh that writes `oauth_creds.json` atomically.
* G-S-07: A `standard-tier` account with the `hd` claim renders identity as `Paid` and shows three bars: Pro, Flash, Flash Lite.

### OpenRouter

* O-S-01: Paste a `sk-or-v1-<random>` key in settings. The card renders with a balance string.
* O-S-02: Set `$OPENROUTER_API_KEY` and observe the env value override the stored value in the settings hint.
* O-S-03: Force `/key` to time out (use a mock server). The credits meter still renders and the chip shows `unavailable`.
* O-S-04: An account with `limit = null` shows the `noLimitConfigured` chip and no key meter.
* O-S-05: Logs show `sk-or-v1-***` and `Bearer ***` rather than the raw token.

### Factory

* F-S-01: Manual cookie paste with a JWT shaped `access-token` cookie renders the card.
* F-S-02: A fresh login through the WebView2 child window populates `factory-session.json` and the card lights up on the next refresh.
* F-S-03: A token rate limits account shows 5h, weekly, and monthly bars plus an Extra usage balance row.
* F-S-04: A legacy account shows the standard and premium bars without an Extra usage balance row.
* F-S-05: The 409 retry chain runs through every step (verified via tracing) when the bearer is stale but cookies are fresh.
* F-S-06: WorkOS minting with body shape B (`useCookie: true`) succeeds when only `workos.com` cookies are available.
* F-S-07: A stale-but-not-rolling window forces `usedPercent = 0` to match the web UI.
* F-S-08: LevelDB byte scanner finds `workos:refresh-token` across at least Chrome, Edge, and Brave on a Windows 11 dev box.

## Phase acceptance tests (all seven providers)

The phase is complete when the following acceptance run passes on a fresh Windows 11 machine with:

* Edge plus Chrome installed with active sessions on cursor.com and factory.ai
* A Copilot account on `github.com`
* The Gemini CLI installed and authed via `gemini`
* An OpenRouter API key in `$OPENROUTER_API_KEY`
* The Phase 4 Claude OAuth and the Phase 5 Codex token both present

Test plan:

* PA-01: Cold start: popup opens within 500 milliseconds; all seven cards render with real data within 5 seconds.
* PA-02: Each card shows a non zero `usedPercent` value or an explicit "no data" state with a stable error string.
* PA-03: No provider leaks identity into another card. The dedupe rule from spec 30 section 2.2 holds: each `identity` field renders only on its owning card.
* PA-04: Tray icon merge logic surfaces the highest usage across the seven providers within plus or minus 1 percent of the Mac reference build.
* PA-05: Refresh button forces a re fetch on every provider in under 3 seconds total.
* PA-06: Settings panel saves and round trips for every provider; no panel crashes or surfaces a console error.
* PA-07: Disable a provider via the toggle; its card disappears from the popup and its background fetch task stops.
* PA-08: Reboot the machine and observe all seven providers re hydrate within 5 seconds.
* PA-09: Log output does not contain any raw secret string for any provider (grep `cookies\|api[_-]?key\|token`, expect only redacted forms).

## CI gates

These gates run on every PR to `main`. They are non negotiable.

* CI-01: `cargo fmt --check` clean across the workspace.
* CI-02: `cargo clippy --workspace --all-targets -- -D warnings` clean. Per provider lints (clippy `pedantic`) opt in via `#[allow(...)]` only when justified inline.
* CI-03: `cargo test --workspace` green. All five new providers' parser tests run.
* CI-04: `cargo deny check` green: no new license violations, no vulnerable transitive deps, especially around `reqwest` and `tokio`.
* CI-05: `pnpm --filter desktop-tauri test` green: Playwright popup smoke passes.
* CI-06: `pnpm --filter desktop-tauri typecheck` green: TS strict mode, no `any`.
* CI-07: Log redactor self test: a fixture log line containing `Bearer sk-or-v1-...` and `Cookie: ...` is asserted to redact correctly.
* CI-08: Provider catalog validation: `ProviderCatalog::build` succeeds at boot; no duplicate ids, no missing icons.

The `cargo test --test providers_live` target is `#[ignore]` and runs only by hand. CI does not exercise it.

## Risks per provider

### Cursor

* R-CUR-01: Chromium v20 App Bound Encryption. Newer Chrome installs apply an additional ABE layer to cookie blobs. We document the failure mode and fall back to manual paste. Phase 6 does not attempt to decrypt v20. Severity medium.
* R-CUR-02: WorkOS may rename session cookies again. The domain only second pass mitigates this, but if WorkOS moves to a non cookie auth surface we lose the auto path. Severity low for v1.
* R-CUR-03: Locale stable reset string requires a POSIX formatter; the Rust `chrono` formatter must be pinned to `en_US_POSIX` style strings, not the system locale. Severity low.

### Copilot

* R-COP-01: GitHub bumps the bot detector. We pin `Editor-Version`, `Editor-Plugin-Version`, and `User-Agent`. If any field drifts, every account 401s. The header drift guard test catches accidental mutations. Severity medium.
* R-COP-02: Device flow requires a clipboard and a browser open call. On locked down machines, both can fail; the dialog should surface a manual copy fallback. Severity low.
* R-COP-03: The `copilot_internal` endpoint is undocumented and can change. We tolerate Int, Double, and string number shapes and accept missing `percent_remaining`. Severity medium.

### Gemini

* R-GEM-01: The OAuth client id and secret extractor is the riskiest part of the phase. Upstream layout changes break us. We mitigate with five fallback layouts plus a bundle walker. Document the failure mode and prepare a hotfix lane. Severity high.
* R-GEM-02: `notify` crate on Windows does not fire for some atomic file replacements. The 1 second poll fallback compensates. Severity low.
* R-GEM-03: `fnm exec --using` requires a working shell PATH. On a fresh Windows install with no fnm we skip the path. Severity low.

### OpenRouter

* R-OR-01: The `/key` endpoint has had multi second latency spikes in the past. The 1 second race timeout is the entire mitigation. Document that this can produce false `unavailable` chips. Severity low.
* R-OR-02: API URL override via env var can be a footgun for users who paste with quotes. Quote stripping covers the common case. Severity low.

### Factory

* R-FAC-01: LevelDB byte scanning is fragile. Chrome can rewrite the `.ldb` files at any moment and we may miss the token. We mitigate by scanning `.log` files too and accept that some refreshes will fail when Chrome is mid compaction. Severity medium.
* R-FAC-02: WorkOS dual client id behavior could change. We try both, so a single id rotation is survivable; a dual rotation is not. Severity low.
* R-FAC-03: The `usesTokenRateLimitsBilling` toggle could ship to all users with no warning. Both code paths must coexist forever per spec note. Severity low.
* R-FAC-04: Helium's alternate root layout is poorly documented. If Helium moves we may lose that browser. Severity very low.
* R-FAC-05: Stale window detection (windowEnd present, secondsRemaining None, expired) requires a tight clock; if the system clock skews more than a few minutes, the meter may render incorrectly. Severity very low.

The riskiest provider in this phase is Gemini, driven by R-GEM-01 (the OAuth client id and secret extraction depends on a packaged JS file shape we do not control). Factory is second by virtue of source count and the LevelDB byte scan.

## Time estimate

| Provider sub group | Estimated dev days |
|--------------------|--------------------|
| Cursor             | 4                  |
| Copilot            | 3                  |
| Gemini             | 5                  |
| OpenRouter         | 1.5                |
| Factory            | 6                  |
| Shared (F1 through F6) | 3              |
| Tests (G1 plus G3) | 1.5                |
| Buffer for live integration debugging | 2 |

Total: 26 dev days for one engineer, or roughly 5 calendar weeks at 5 productive days a week. With one helper engineer the cross cutting work (F1, F2, F3, F4) can run in parallel with provider work and the calendar drops to 3.5 to 4 weeks.

## Open questions

1. Do we ship a Mac to Windows migration importer that reads the old Keychain blobs (via the user's iCloud Drive copy of `~/Library/Application Support/CodexBar/`) and seeds `config.json`? The spec 60 catalog implies "yes eventually" but does not commit a phase. Defer to Phase 7 unless a beta user blocks on it.
2. Does the Gemini login flow need a fallback to a non Terminal path on Windows 11 S mode (no console)? Probably yes via a browser opened to `https://accounts.google.com/o/oauth2/auth?...` with the extracted client id; needs upstream confirmation that the constants are scoped to allow that.
3. Should Factory's WebView2 login window persist storage between launches? Spec says non persistent, but a developer convenience flag might be worth adding behind a feature toggle. Defer until a tester complains.
4. OpenRouter's `HTTP-Referer` header: do we set the CodexBar website URL by default, or leave it unset? Spec says optional; defaulting reduces friction but identifies CodexBar to OpenRouter. Recommendation: leave unset unless user opts in.
5. Cursor's POSIX reset string format ("Resets MMM d at h:mma"): do we localize this or keep it stable? Mac keeps it stable. Recommendation: keep stable for parity, document in the strings file.
6. Does the integration test target need a CI gate at all, even nightly? It would require shared credentials, which we cannot store. Recommendation: do not gate. Document as a manual gate per release.

## Definition of done

Phase 6 is done when:

* All commits in this plan have landed on `main` and pushed
* `cargo test --workspace` is green
* `pnpm --filter desktop-tauri test` is green
* The phase acceptance test plan (PA-01 through PA-09) passes on a clean Windows 11 install
* Each provider's sub acceptance test list passes
* The risks table is updated with any new failure modes discovered during testing
* The Phase 7 plan (cost scan, status feeds, notifications) can begin without back references to unfinished Phase 6 work
