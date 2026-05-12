# 43 — Long-tail Providers Catalog

This is a uniform reference catalog of every CodexBar provider **not** covered in the Tier-1 specs (Claude, Codex, Cursor, Copilot, Gemini, Vertex AI, Factory, OpenRouter). The goal is to port these to a Tauri 2 + React + shared Rust crate stack on Windows.

Each entry lists the behavior, auth surfaces, endpoints, settings, and reset semantics so the Rust/TS engineer can re-implement the provider without reading the Swift source. The cross-cutting matrices at the end are the high-value artifact — start there for an at-a-glance map of port complexity.

Conventions used in this doc:

- **Mode** = `auto | web | api | cli | oauth` (the source-mode picker in CodexBar's Preferences → Providers UI).
- **Cookie sources** = `auto | manual | off` (when a provider uses browser cookies, this picker controls where they come from).
- **Settings keys** are stored in `~/.codexbar/config.json` (`providers[id]`) on macOS. The Windows port should map this to `%APPDATA%\CodexBar\config.json` (see spec 04).
- All providers use the shared `ProviderTokenAccountSelection` system for multi-account; only providers with `requiresManualCookieSource` are flagged below.
- Reset windows are mapped into the snapshot's `primary` (top bar), `secondary` (middle bar), and `tertiary` (bottom bar) `RateWindow`s — the labels per slot are listed in the entry header.

---

## Abacus AI

- **ID / name / tagline**: `abacus` / "Abacus AI" / ChatLLM/RouteLLM compute-credit dashboard.
- **Status**: Stable.
- **Auth source(s)** (order):
  1. Manual `Cookie:` header (user-pasted).
  2. Cached browser cookie header (Keychain cache `cookie.abacus`).
  3. Browser auto-import — Chrome first, then full browser order fallback.
- **Endpoints**:
  - `GET https://apps.abacus.ai/api/_getOrganizationComputePoints` — compute points balance.
  - `POST https://apps.abacus.ai/api/_getBillingInfo` (body `{}`) — billing tier + next billing date (optional; bounded 5 s timeout, soft-fail).
  - Dashboard URL: `https://apps.abacus.ai/chatllm/admin/compute-points-usage`.
- **Cookie shapes**: domains `abacus.ai`, `apps.abacus.ai`. Required session cookie name in: `sessionid | session_id | session_token | auth_token | access_token` (exact match); fallback substring search for `session/auth/sid/jwt` excluding `csrf*`, `_ga`, `_gid`, analytics. CSRF cookies are explicitly excluded.
- **Login flow**: none — user opens `apps.abacus.ai`, signs in normally, CodexBar reads the cookie.
- **Reset windows**: monthly (billing cycle). Mapped as primary only.
- **Settings keys**:
  - `abacusCookieSource` (default `auto`).
  - `abacusCookieHeader` (manual mode, secret).
- **Cost/credit semantics**: opaque "compute points". UI shows used vs. total, pace tick, and next billing date.
- **Edge cases**:
  - Billing-info fetch is bounded so a slow billing endpoint never blocks credits.
  - Body-level error strings `"expired" | "session" | "login" | "authenticate" | "unauthorized" | "forbidden"` re-map a 200 body to `unauthorized`.
  - Cached cookies that fail with `shouldClearCachedCookie` errors are cleared before retry.
- **Mac→Win notes**: Safari path drops. Chrome-Beta/Canary/Arc supported via the standard chromium-fork importer (see spec 22 cookie subsystem).

---

## Alibaba Coding Plan

- **ID / name / tagline**: `alibaba` / "Alibaba" / Bailian/Model Studio coding-plan quota.
- **Status**: Stable (web baseline); API mode partial.
- **Auth source(s)** (order):
  - Web (preferred): manual cookie → env `ALIBABA_CODING_PLAN_COOKIE` → Keychain cache → automatic browser import.
  - API: `ALIBABA_CODING_PLAN_API_KEY` → `ALIBABA_QWEN_API_KEY` → `DASHSCOPE_API_KEY` → config `apiKey`.
- **Endpoints**:
  - Quota (web/API): `POST /data/api.json?action=zeldaEasy.broadscope-bailian.codingPlan.queryCodingPlanInstanceInfoV2&product=broadscope-bailian&api=queryCodingPlanInstanceInfoV2`.
  - International host: `https://modelstudio.console.alibabacloud.com` (region `ap-southeast-1`, console site `MODELSTUDIO_ALIBABACLOUD`).
  - China mainland: `https://bailian.console.aliyun.com` (region `cn-beijing`, console site `BAILIAN_ALIYUN`).
  - Console RPC fallback: `https://bailian-singapore-cs.alibabacloud.com` / `https://bailian-cs.console.aliyun.com`.
  - Override env: `ALIBABA_CODING_PLAN_HOST`, `ALIBABA_CODING_PLAN_QUOTA_URL`.
- **Cookie shapes**: domains include `bailian-singapore-cs.alibabacloud.com`, `bailian.console.aliyun.com`, `modelstudio.console.alibabacloud.com`, `account.aliyun.com`, `signin.aliyun.com`, `passport.alibabacloud.com`. Required cookie names: `login_aliyunid_ticket` **AND** one of `login_aliyunid_pk | login_current_pk | login_aliyunid`.
- **API headers** (when using API key): `Authorization: Bearer <key>`, `x-api-key: <key>`, `X-DashScope-API-Key: <key>`.
- **Login flow**: open `https://modelstudio.console.alibabacloud.com/ap-southeast-1/?tab=coding-plan` (or `https://bailian.console.aliyun.com/cn-beijing/?tab=model`) — auto-import on next refresh.
- **Reset windows surfaced**: 5-hour (primary), Weekly (secondary), Monthly (tertiary) from `per5HourUsedQuota`, `perWeekUsedQuota`, `perBillMonthUsedQuota` with matching `*NextRefreshTime`.
- **Settings keys**:
  - `alibabaCodingPlanCookieSource` (default `auto`).
  - `alibabaCodingPlanCookieHeader`.
  - `alibabaCodingPlanAPIToken`.
  - `alibabaCodingPlanAPIRegion` (`intl` | `cn`, default `intl`).
- **Cost/credit semantics**: quota units (no $ equivalence surfaced).
- **Edge cases**:
  - International region can fall back to China mainland once on credential/host errors.
  - China-mainland API keys can return `ConsoleNeedLogin` even with a valid API key — surfaced as an explicit API-path limitation; web mode required.
  - Includes a hand-rolled Chromium-cookie-store reader (SQLite + Keychain `Safe Storage` AES-CBC) that decrypts cookies directly when the standard importer fails on locked DBs.
- **Mac→Win notes**: The Chromium-fork direct-DB fallback uses macOS Keychain Safe Storage. On Windows, the Chromium equivalent is DPAPI-encrypted in `Local State` (see spec 22). Two regions must be exposed in settings.

---

## Amp (Sourcegraph)

- **ID / name / tagline**: `amp` / "Amp" / Sourcegraph Amp free-tier daily quota.
- **Status**: Stable.
- **Auth source(s)** (order): manual cookie → browser auto-import (any of: Safari, Chromium, Firefox).
- **Endpoints**:
  - `GET https://ampcode.com/settings` — HTML page is scraped for the embedded `freeTierUsage` JSON.
  - Dashboard: `https://ampcode.com/settings`.
- **Cookie shapes**: domain `ampcode.com` / `www.ampcode.com`. Single session cookie name: `session`.
- **Login flow**: sign in at `ampcode.com`.
- **Reset windows surfaced**: "Amp Free" rolling — time-to-full is computed from hourly replenishment rate (no fixed window).
- **Settings keys**:
  - `ampCookieSource` (default `auto`).
  - `ampCookieHeader`.
- **Cost/credit semantics**: free-tier usage points; UI shows remaining and "Resets in …".
- **Edge cases**:
  - Login redirect detection (`/login`, `/signin`, `/auth/sign-in?returnTo=…`) cancels the request and reports invalid creds.
  - Custom `URLSession` delegate strips/reattaches cookies across redirects so cookies are only sent to `ampcode.com` hosts.
  - HTML may change — emit raw HTML snippets and `freeTierUsage` presence hints on parse failure for diagnostic dumps.
- **Mac→Win notes**: HTML scraping — keep parsing tolerant. No SQLite or PTY needed.

---

## Antigravity (Google)

- **ID / name / tagline**: `antigravity` / "Antigravity" / Google Antigravity (Claude + Gemini Pro + Gemini Flash quotas via Google CodeAssist).
- **Status**: Experimental.
- **Auth source(s)** (order): `cli` (local LSP process probe) → `oauth` (Google OAuth) under `auto`.
- **Endpoints**:
  - **Local LSP probe** (CLI mode):
    - Detect `language_server_macos` process matching `--app_data_dir antigravity` or path `/antigravity/`.
    - Extract `--csrf_token` and `--extension_server_port` flags.
    - `lsof -nP -iTCP -sTCP:LISTEN -p <pid>` for listening port discovery.
    - `POST https://127.0.0.1:<port>/exa.language_server_pb.LanguageServerService/GetUnleashData` with `X-Codeium-Csrf-Token` (self-signed cert, insecure-TLS allowed).
    - `POST .../GetUserStatus` (primary) → `clientModelConfigs[].quotaInfo.{remainingFraction, resetTime}`.
    - `POST .../GetCommandModelConfigs` (fallback).
  - **OAuth (Google CodeAssist)**:
    - `POST https://cloudcode-pa.googleapis.com/v1internal:loadCodeAssist`
    - `POST .../v1internal:onboardUser` (when projectID missing)
    - `POST .../v1internal:fetchAvailableModels` → primary; on 403 fallback to:
    - `POST .../v1internal:retrieveUserQuota` → bucket aggregation.
    - Token refresh: `POST https://oauth2.googleapis.com/token` (form body) with `client_id/client_secret/refresh_token` from credentials file or env (`ANTIGRAVITY_OAUTH_CLIENT_ID`, `ANTIGRAVITY_OAUTH_CLIENT_SECRET`).
- **Token shape**: OAuth `access_token` (Bearer) + `refresh_token` from `AntigravityOAuthCredentialsStore` (file at `~/.antigravity/oauth_creds.json` or equivalent).
- **Login flow** (interactive):
  1. Browser opens Google OAuth consent page.
  2. User authorizes.
  3. Callback delivers `code` → exchanged for tokens → stored to file.
  4. UI auto-refreshes.
- **Reset windows surfaced**: per-model quotas. Snapshot maps to: Claude (primary), Gemini Pro (secondary), Gemini Flash (tertiary).
- **Settings keys**:
  - `antigravityUsageDataSource` (`auto | oauth | cli`).
- **Cost/credit semantics**: per-model remaining fraction (no $ amounts). UI: percent left + ISO reset time.
- **Edge cases**:
  - Local probe requires `lsof` and `ps` on macOS — on Windows, neither exists; this mode must be re-implemented with PowerShell `Get-NetTCPConnection -OwningProcess` or `netstat -ano`.
  - JWT `id_token` is decoded locally (base64url) to extract `email` and `hd` (hosted domain).
  - When `GetUserStatus` works but quotas are empty, return an identity-only snapshot.
- **Mac→Win notes**: The "local LSP probe" mode is *the hardest port piece* in the whole catalog — requires PowerShell or Win32 API for process/port discovery + a Windows installer of Antigravity for testing. Consider dropping CLI mode and shipping OAuth-only on first cut.

---

## Augment Code

- **ID / name / tagline**: `augment` / "Augment" / Augment Code credit usage and subscription cycle.
- **Status**: Stable.
- **Auth source(s)** (order under `auto`):
  1. **Auggie CLI** preferred (when `auggie` binary on PATH) — runs `auggie account status` to avoid browser prompts.
  2. **Web (cookies)** — manual cookie → Keychain cache → browser import (custom order: Safari, Chrome, Chrome Beta, Chrome Canary, Edge, Edge Beta, Brave, Arc, Dia, Arc Beta, Firefox) → stored session cookies (Application Support JSON file).
- **Endpoints**:
  - `GET https://app.augmentcode.com/api/credits` (required).
  - `GET https://app.augmentcode.com/api/subscription` (optional — soft-failure for plan/email/billing cycle).
  - Dashboard: `https://app.augmentcode.com/account/subscription`.
- **Cookie shapes**: domain `*.augmentcode.com`. Recognized session cookies: `_session`, `auth0`, `auth0.is.authenticated`, `a0.spajs.txs`, `__Secure-next-auth.session-token`, `next-auth.session-token`, `__Host-authjs.csrf-token`, `authjs.session-token`.
- **Login flow**: built-in "Refresh Session" menu action + "Open Augment (Log Out & Back In)" link on session-expired errors.
- **Reset windows surfaced**: monthly billing cycle (primary), reset = `billingPeriodEnd` from `subscription`.
- **Settings keys**:
  - `augmentCookieSource` (default `auto`).
  - `augmentCookieHeader`.
- **Cost/credit semantics**: `usageUnitsRemaining` / `usageUnitsConsumedThisBillingCycle` / `usageUnitsAvailable`. UI shows used vs. limit, percent, and plan name.
- **Edge cases**:
  - `AugmentSessionStore` persists cookies to disk (`~/Library/Application Support/CodexBar/augment-session.json`) and reloads on launch — second-line fallback after browser import fails.
  - Keepalive runtime: pings `/api/auth/session` every 1 minute, refreshes 5 min before expiry, min 1 min between refreshes (see `AugmentSessionKeepalive`).
  - 401 → `sessionExpired`; 403 on `/credits` is treated as a transient network error; 403 on `/subscription` → `notLoggedIn`.
  - RFC 6265 cookie-domain filtering applied per-request.
- **Mac→Win notes**: The on-disk session JSON works portably (Application Support → `%APPDATA%`). Keepalive timer is just a tokio task in Rust. CLI fallback needs the Windows `auggie.exe` binary detected via `where auggie`.

---

## Codebuff

- **ID / name / tagline**: `codebuff` / "Codebuff" / Credit balance + weekly rate limits.
- **Status**: Stable.
- **Auth source(s)** (order): `CODEBUFF_API_KEY` env → config `apiKey` (Settings → Codebuff) → `~/.config/manicode/credentials.json` (`default.authToken` then top-level `authToken`).
- **Endpoints**:
  - `POST https://www.codebuff.com/api/v1/usage` (body `{"fingerprintId":"codexbar-usage"}`) — required.
  - `GET https://www.codebuff.com/api/user/subscription` — optional, only fetched when token came from `authFile` (CLI session). Bounded by 2 s grace period after `/usage` completes.
  - Override base URL: `CODEBUFF_API_URL`.
  - Dashboard: `https://www.codebuff.com/usage`.
- **Token shape**: opaque API key (Bearer).
- **Login flow**: `codebuff login` populates the CLI credentials file.
- **Reset windows surfaced**: monthly credit balance (primary, reset at `next_quota_reset`); weekly rate limit (secondary, 7-day window from `rateLimit.weeklyResetsAt`).
- **Settings keys**:
  - `codebuffAPIToken`.
- **Cost/credit semantics**: opaque credits + USD billing-period end. UI shows balance, tier (e.g. "Pro"), auto-top-up state.
- **Edge cases**:
  - Status codes: 401/403 → unauthorized, 404 → endpointNotFound, 5xx → serviceUnavailable.
  - Subscription endpoint can be slow — concurrent fetch with 2 s grace period.
  - Parser tolerates `usage|used` and `quota|limit` key variants; epoch numbers >10¹⁰ are treated as ms.
- **Mac→Win notes**: `~/.config/manicode/credentials.json` → `%USERPROFILE%\.config\manicode\credentials.json` on Windows.

---

## Command Code

- **ID / name / tagline**: `commandcode` / "Command Code" / Monthly USD credits from Command Code billing.
- **Status**: Stable.
- **Auth source(s)** (order): manual cookie → automatic browser import (better-auth session cookies).
- **Endpoints**:
  - `https://api.commandcode.ai` billing endpoints (`/internal/billing/credits`, `/internal/billing/subscriptions`) — used by the descriptor's fetcher.
  - Dashboard: `https://commandcode.ai/studio`. Subscription dashboard: `https://commandcode.ai/sixhobbits/settings/billing`.
- **Cookie shapes**: domain `commandcode.ai` / `www.commandcode.ai`. Uses better-auth session cookies.
- **Static plan catalog** (USD/month):
  - `individual-go` → "Go" $10/month
  - `individual-pro` → "Pro" $30/month
  - `individual-max` → "Max" $150/month
  - `individual-ultra` → "Ultra" $300/month
- **Login flow**: sign in at `commandcode.ai`.
- **Reset windows surfaced**: monthly billing cycle (primary). The `credits` endpoint returns remaining; the plan total is hard-coded by `planId` from the static catalog (the API does not expose plan total).
- **Settings keys**:
  - `commandcodeCookieSource` (default `auto`).
  - `commandcodeCookieHeader`.
- **Cost/credit semantics**: monthly USD credits remaining; plan total from static catalog. UI: $X / $Y used.
- **Edge cases**: Plan-total lookup is offline — adding a new plan tier requires shipping a new build with an updated `CommandCodePlanCatalog`.
- **Mac→Win notes**: Static catalog is pure data, port directly.

---

## Crof

- **ID / name / tagline**: `crof` / "Crof" / Request quota + USD credit balance.
- **Status**: Stable.
- **Auth source(s)** (order): `CROF_API_KEY` env → `CROFAI_API_KEY` env → config `apiKey`.
- **Endpoints**:
  - `GET https://crof.ai/usage_api/` (Bearer auth) → `{credits, requests_plan, usable_requests}`.
  - Dashboard: `https://crof.ai/dashboard`.
- **Token shape**: opaque API key.
- **Reset windows surfaced**: daily request quota (primary). Reset is *inferred*: next `America/Chicago` midnight (Crof support said quota resets around midnight Central time; DST → GMT-5 or GMT-6 automatically).
- **Settings keys**:
  - `crofAPIToken` (config-only, no env-override exposure in UI).
- **Cost/credit semantics**: requests (primary), USD credits (secondary; floored to cents to avoid micro-cent overstatement).
- **Edge cases**:
  - Remaining percent is *floored* so 998/1000 doesn't round up to 100% left.
  - SVG icon rendered as template image.
- **Mac→Win notes**: Pure HTTP + JSON. Trivial port.

---

## DeepSeek

- **ID / name / tagline**: `deepseek` / "DeepSeek" / API credit balance.
- **Status**: Stable.
- **Auth source(s)** (order): `DEEPSEEK_API_KEY` / `DEEPSEEK_KEY` env → config `apiKey` → selected token account.
- **Endpoints**:
  - `GET https://api.deepseek.com/user/balance` → `{is_available, balance_infos: [{currency, total_balance, granted_balance, topped_up_balance}]}`.
  - Dashboard: `https://platform.deepseek.com/usage`. Status: `https://status.deepseek.com`.
- **Token shape**: API key (Bearer).
- **Reset windows surfaced**: none — DeepSeek is a balance-only provider. Primary `RateWindow` has no reset, `resetDescription` holds the formatted balance string.
- **Settings keys**:
  - `deepseekAPIToken` (and token-account selector).
- **Cost/credit semantics**: USD or CNY ($/¥); UI shows "$X.XX (Paid: $Y.YY / Granted: $Z.ZZ)". Prefers USD entry when funded; else first nonzero; else first USD; else first.
- **Edge cases**:
  - If `is_available=false` but balance>0 → "Balance unavailable for API calls".
  - If `total_balance=0` → "$0.00 — add credits at platform.deepseek.com".
- **Mac→Win notes**: Trivial.

---

## Doubao (Volcengine Ark / ByteDance)

- **ID / name / tagline**: `doubao` / "Doubao" / Volcengine Ark request limits.
- **Status**: Stable.
- **Auth source(s)** (order): `ARK_API_KEY` / `VOLCENGINE_API_KEY` / `DOUBAO_API_KEY` env → config `apiKey`.
- **Endpoints**:
  - `POST https://ark.cn-beijing.volces.com/api/coding/v3/chat/completions` — probing chat endpoint to read rate-limit headers.
  - Probe models tried in order: `doubao-seed-2.0-code`, `doubao-1.5-pro-32k`, `doubao-lite-32k` (different key types have different access).
  - Dashboard (`subscribe` tab): `https://console.volcengine.com/ark/region:ark+cn-beijing/openManagement?LLM=%7B%7D&advancedActiveKey=subscribe`.
- **Token shape**: API key (Bearer).
- **Reset windows surfaced**: rate-limit window (primary). `x-ratelimit-reset-requests` decoded as ISO8601, `2d3h4m5s`-style duration, or epoch seconds.
- **Settings keys**:
  - `doubaoAPIToken`.
- **Cost/credit semantics**: requests (`x-ratelimit-remaining-requests` / `x-ratelimit-limit-requests`).
- **Edge cases**:
  - Sends a `max_tokens: 1` "hi" message — the request is real and *will consume one request* against your quota. Document this clearly in Windows settings UI.
  - HTTP 429 is treated as success (key valid, just rate-limited).
  - When rate-limit headers are missing but the key is valid → snapshot shows "Active — check dashboard for details".
- **Mac→Win notes**: Trivial. Consider warning user that "Refresh now" costs one request.

---

## JetBrains AI

- **ID / name / tagline**: `jetbrains` / "JetBrains AI" / Local IDE quota file scrape.
- **Status**: Stable.
- **Auth source(s)**: local filesystem only — no API or OAuth.
- **Endpoints**: none (pure local file read).
- **Data source**:
  - macOS: `~/Library/Application Support/JetBrains/<IDE>/options/AIAssistantQuotaManager2.xml`.
  - macOS Android Studio: `~/Library/Application Support/Google/AndroidStudio*/options/AIAssistantQuotaManager2.xml`.
  - Linux: `~/.config/JetBrains/...` / `~/.config/Google/...`.
  - **Windows** (must add): `%APPDATA%\JetBrains\<IDE>\options\AIAssistantQuotaManager2.xml` and `%APPDATA%\Google\AndroidStudio*\options\AIAssistantQuotaManager2.xml`.
- **Format**: XML with HTML-encoded JSON attributes. `quotaInfo` attribute: `{type, current, maximum, tariffQuota.available, until}`. `nextRefill` attribute: `{type, next (ISO), tariff.amount, tariff.duration (e.g. "PT720H")}`.
- **Selection**: most recently modified `AIAssistantQuotaManager2.xml` across all detected IDEs.
- **Supported IDEs**: IntelliJ IDEA, PyCharm, WebStorm, GoLand, CLion, DataGrip, RubyMine, Rider, PhpStorm, RustRover, Android Studio, Fleet, Aqua, DataSpell.
- **Reset windows surfaced**: monthly tariff (primary); reset from `nextRefill.next` (not `quotaInfo.until`).
- **Settings keys**:
  - `jetbrainsIDEBasePath` (string; empty = auto-detect).
- **Login flow**: none — alert tells the user to launch a JetBrains IDE and ensure AI Assistant has been used at least once.
- **Cost/credit semantics**: tariff tokens. UI shows percent + IDE name/version as identity.
- **Edge cases**:
  - File only appears after AI Assistant has been used.
  - Internal format; may change between IDE versions.
- **Mac→Win notes**: IDE base-path detection must use `%APPDATA%` paths. The XML parser is simple and portable. **No** cookie or HTTP work.

---

## Kilo

- **ID / name / tagline**: `kilo` / "Kilo" / app.kilo.ai credit balance + Kilo Pass.
- **Status**: Stable.
- **Auth source(s)** (modes: `auto | api | cli`):
  - API: `KILO_API_KEY` env → config `apiKey`.
  - CLI: reads `~/.local/share/kilo/auth.json` → `kilo.access` (token field). Windows path: `%LOCALAPPDATA%\kilo\auth.json` (must add).
  - Auto: API first; CLI fallback only on `missingCredentials` or `unauthorized`.
- **Endpoints**:
  - tRPC batch: `GET https://app.kilo.ai/api/trpc/user.getCreditBlocks,kiloPass.getState,user.getAutoTopUpPaymentMethod?batch=1&input=<encoded>`
  - Override base: env `KILO_API_URL`.
  - Dashboard: `https://app.kilo.ai/usage`.
- **Token shape**: API key (Bearer).
- **Reset windows surfaced**: credit balance (primary); Kilo Pass subscription window (secondary, reset = `nextBillingAt | nextRenewalAt | renewsAt`).
- **Settings keys**:
  - `kiloUsageSource` (`auto | api | cli`).
  - `kiloAPIToken`.
- **Cost/credit semantics**: USD credit blocks (`amount_mUsd` / `balance_mUsd` in millionths of USD). Kilo Pass shows `$used / $base (+ $bonus bonus)`.
- **Plan-name mapping** (`tier_19` → "Starter", `tier_49` → "Pro", `tier_199` → "Expert").
- **Edge cases**:
  - `user.getAutoTopUpPaymentMethod` is optional — errors on this procedure don't fail the snapshot.
  - tRPC batch shape: response can be array or `{0:…, 1:…, 2:…}` map; both supported.
  - Special-cased zero-balance: empty `creditBlocks` + `totalBalance_mUsd: 0` → explicit "0/0 credits" snapshot.
- **Mac→Win notes**: Path differs on Windows. tRPC parsing is pure JSON, portable.

---

## Kimi (Moonshot AI — Coding console)

- **ID / name / tagline**: `kimi` / "Kimi" / Kimi For Coding weekly quota + 5-hour rate limit.
- **Status**: Stable.
- **Auth source(s)** (order):
  1. Manual JWT in settings (`kimi-auth` cookie value).
  2. Env `KIMI_AUTH_TOKEN`.
  3. Browser auto-import (Chromium-fork order: Arc, Chrome, Safari, Edge, Brave, Chromium).
- **Endpoints**:
  - `POST https://www.kimi.com/apiv2/kimi.gateway.billing.v1.BillingService/GetUsages` with body `{"scope":["FEATURE_CODING"]}`.
  - Dashboard: `https://www.kimi.com/code/console`.
- **Cookie shape**: `kimi-auth=<JWT>`. The JWT's `device_id`, `ssid`, `sub` claims are decoded locally and sent as `x-msh-device-id`, `x-msh-session-id`, `x-traffic-id` request headers.
- **Token shape**: JWT (sent as Bearer **and** as Cookie).
- **Reset windows surfaced**: weekly quota (primary, from `detail.resetTime`), 5-hour rate limit (secondary, from `limits[0].detail.resetTime`, window 300 min).
- **Settings keys**:
  - `kimiCookieSource` (default `auto`).
  - `kimiCookieHeader` / manual token field.
- **Cost/credit semantics**: requests. Tier→quota mapping documented (Andante 1,024 req/wk ¥49, Moderato 2,048 req/wk ¥99, Allegretto 7,168 req/wk ¥199). All tiers cap at 200 req/5h.
- **Edge cases**:
  - Many Connect-RPC headers required (`connect-protocol-version`, `x-msh-platform: web`, `x-language: en-US`, `r-timezone: <TZ>`) — a missing header causes 401/403 noise.
  - JWT decoded with base64url padding fix.
- **Mac→Win notes**: HTTP only — port directly. Browser priority should drop Safari on Windows.

---

## Kimi K2 (legacy `kimi-k2.ai`)

- **ID / name / tagline**: `kimik2` / "Kimi K2" / Legacy `kimi-k2.ai` credit endpoint.
- **Status**: Stable (legacy — Moonshot provider is the modern surface).
- **Auth source(s)**: `KIMI_K2_API_KEY` / `KIMI_API_KEY` / `KIMI_KEY` env → config `apiKey`.
- **Endpoints**:
  - `GET https://kimi-k2.ai/api/user/credits` (Bearer).
  - Dashboard: `https://kimi-k2.ai/my-credits`.
- **Token shape**: API key.
- **Reset windows surfaced**: none — credit balance only.
- **Settings keys**:
  - `kimik2APIToken`.
- **Cost/credit semantics**: credits remaining; identity line "Credits: <remaining>".
- **Edge cases**: Parser is liberal — scans many path/key variants (`total_credits_consumed | totalCreditsConsumed | …`, `credits_remaining | creditsRemaining | …`), falls back to header `x-credits-remaining`.
- **Mac→Win notes**: Trivial. Consider migrating users to the unified Moonshot provider in the future.

---

## Kiro (AWS)

- **ID / name / tagline**: `kiro` / "Kiro" / AWS Builder ID coding plan via `kiro-cli`.
- **Status**: Stable.
- **Auth source(s)**: CLI only. Requires `kiro-cli` binary on PATH (probes via `TTYCommandRunner.which`).
- **Endpoints**: none (CLI text scrape).
- **Login flow**: `kiro-cli login` (AWS Builder ID OAuth flow handled by the CLI itself).
- **Probe flow**:
  1. `kiro-cli whoami` (5 s timeout) — checks logged in. Errors `"not logged in" | "login required"` → `notLoggedIn`.
  2. `kiro-cli chat --no-interactive "/usage"` (20 s wall timeout, 10 s idle cutoff after first output).
  3. Strip ANSI escape codes from output.
  4. Regex-parse:
     - Plan name: `\| KIRO \w+` (legacy) or `Plan:\s*(.+)` (kiro-cli 1.24+).
     - Percent: `█+\s*(\d+)%`.
     - Credits: `\((\d+\.?\d*)\s+of\s+(\d+)\s+covered`.
     - Reset: `resets on (\d{2}/\d{2})`.
     - Bonus: `Bonus credits:\s*(\d+\.?\d*)/(\d+)`, `expires in (\d+) days?`.
     - Managed plan: `managed by admin | managed by organization`.
  - Version detector: `kiro-cli --version` → strips `kiro-cli ` prefix.
  - Dashboard: `https://app.kiro.dev/account/usage`. Status link: AWS Health Dashboard.
- **Reset windows surfaced**: monthly credits (primary, reset = parsed MM/DD assuming current or next year), bonus credits (secondary, expiry = `now + Nd`).
- **Settings keys**: none — CLI driven.
- **Cost/credit semantics**: opaque "covered in plan" credits.
- **Edge cases**:
  - Process management is custom-rolled: thread-safe `ActivityState` lock, readability handlers, idle-timeout cutoff.
  - Managed plans (admin-managed) skip usage parsing and just show plan name.
  - "Not logged in" patterns also include `failed to initialize auth portal`, `kiro-cli login`, `oauth error`.
- **Mac→Win notes**: **Hard port** — the CLI runner uses Foundation `Process` + `Pipe` + `readabilityHandler`. On Windows, use Rust `tokio::process::Command` with `stdin: Stdio::null()`, separate stdout/stderr pipes, and a tokio interval ticker for idle-timeout. ANSI stripping is portable. Verify `kiro-cli.exe` exists for Windows (AWS does ship one).

---

## Manus

- **ID / name / tagline**: `manus` / "Manus" / manus.im credit balance + monthly + daily refresh.
- **Status**: Stable.
- **Auth source(s)** (order):
  1. Manual cookie (must contain `session_id=`).
  2. Keychain cache (`cookie.manus`).
  3. Browser auto-import.
  4. Env `MANUS_SESSION_TOKEN` / `MANUS_SESSION_ID` (raw token), or `MANUS_COOKIE` / `manus_cookie` (full header).
- **Endpoints**:
  - `POST https://api.manus.im/user.v1.UserService/GetAvailableCredits` with body `{}` (Bearer token = `session_id` cookie value).
  - Dashboard: `https://manus.im`.
- **Cookie shape**: `session_id=<token>` from domain `manus.im`.
- **Reset windows surfaced**: Monthly credits (primary, derived from `proMonthlyCredits − periodicCredits`); Daily refresh (secondary, reset = `nextRefreshTime`).
- **Settings keys**:
  - `manusCookieSource` (default `auto`).
  - `manusCookieHeader`.
- **Cost/credit semantics**: opaque credits. Identity line: `Balance: <N> credits`.
- **Edge cases**:
  - Response tolerates `{data: {…}} | {result: {…}} | {response: {…}} | {availableCredits: {…}}` envelopes.
  - Empty/unrelated 200 responses are explicitly rejected — must contain at least one known credit key.
  - Custom `decodeLossyDoubleIfPresent` for fields that come as int/double/string.
- **Mac→Win notes**: HTTP only — straightforward.

---

## MiMo (Xiaomi)

- **ID / name / tagline**: `mimo` / "Xiaomi MiMo" / Xiaomi MiMo platform balance + monthly token plan.
- **Status**: Stable.
- **Auth source(s)** (order): manual cookie → Keychain cache → browser auto-import (Chrome, Chrome Beta, Chrome Canary).
- **Endpoints**:
  - `GET https://platform.xiaomimimo.com/api/v1/balance` (required) → `{balance, currency}`.
  - `GET https://platform.xiaomimimo.com/api/v1/tokenPlan/detail` (optional) → `{planCode, currentPeriodEnd, expired}`.
  - `GET https://platform.xiaomimimo.com/api/v1/tokenPlan/usage` (optional) → `{monthUsage.items[]: {used, limit, percent}}`.
  - Override base: `MIMO_API_URL`.
  - Dashboard: `https://platform.xiaomimimo.com/#/console/balance`.
- **Cookie shape**: required cookies `api-platform_serviceToken` AND `userId` from `platform.xiaomimimo.com`; optional `api-platform_ph`, `api-platform_slh`.
- **Reset windows surfaced**: token plan window (primary, percent + `currentPeriodEnd`).
- **Settings keys**:
  - `mimoCookieSource` (default `auto`).
  - `mimoCookieHeader`.
- **Cost/credit semantics**: balance shown with currency symbol; monthly token plan shown as percent + period end.
- **Edge cases**:
  - Custom 5-tier cookie validation; missing `api-platform_serviceToken` or `userId` → explicit error.
  - Sequential retry: cached cookie → fresh import; only retries on credential / login / parse failures.
- **Mac→Win notes**: Standard HTTP + browser cookies.

---

## MiniMax

- **ID / name / tagline**: `minimax` / "MiniMax" / Coding Plan prompts/window.
- **Status**: Stable.
- **Auth source(s)** (modes: `auto | web | api`):
  - API: `MINIMAX_CODING_API_KEY` (preferred over `MINIMAX_API_KEY` — coding-plan `sk-cp-*` wins over standard `sk-api-*`).
  - Web: manual cookie + browser auto-import + Chromium **localStorage** access tokens.
- **Endpoints**:
  - Coding Plan page: `https://platform.minimax.io/user-center/payment/coding-plan?cycle_type=3` (global) or `https://platform.minimaxi.com/...` (CN).
  - Remains API: `POST .../v1/api/openplatform/coding_plan/remains`.
  - API path equivalent: `https://api.minimax.io/v1/api/openplatform/coding_plan/remains` / `https://api.minimaxi.com/...`.
  - Override env: `MINIMAX_HOST`, `MINIMAX_CODING_PLAN_URL`, `MINIMAX_REMAINS_URL`.
- **Cookie shape**: includes `HERTZ-SESSION` (used as Bearer token candidate). Auth tokens may come from Chromium **localStorage** rather than cookies — `MiniMaxLocalStorageImporter` extracts access tokens and group IDs from leveldb.
- **Token shape**: API key kinds — `sk-cp-*` (Coding Plan, **standard kind cookie-only**), `sk-api-*` (Standard).
- **Reset windows surfaced**: 5-hour prompts (primary) + window (secondary). Plan/tier in identity.
- **Settings keys**:
  - `minimaxCookieSource` (default `auto`).
  - `minimaxCookieHeader`.
  - `minimaxAPIToken`.
  - `minimaxAPIRegion` (`global | cn`, default `global`).
- **Cost/credit semantics**: prompts (request counts).
- **Edge cases**:
  - **Two auth modes coexisting** — when an API token is present, cookies are skipped (`MiniMaxAuthMode.apiToken.allowsCookies == false`).
  - The web strategy iterates candidate (cookieHeader, accessToken) pairs from cookies + localStorage tokens + HERTZ-SESSION cookie; retries on credential / parse errors.
  - Standard `sk-api-*` keys are recognized but the API endpoint is *not* used for them — they fall through to web mode.
  - Custom "Provider-specific UI feature": MiniMax has a region picker + dual cookie/token visibility logic.
- **Mac→Win notes**: localStorage extraction from Chromium leveldb (`Local Storage/leveldb/*.log`) — needs a leveldb reader on Windows (use `rusqlite`-style or `rust-leveldb` crate). This is non-trivial. See also Windsurf which uses the same mechanism.

---

## Mistral

- **ID / name / tagline**: `mistral` / "Mistral" / Monthly billing usage + cost aggregation.
- **Status**: Stable.
- **Auth source(s)** (order under `auto`): manual cookie → Keychain cache → browser auto-import.
- **Endpoints**:
  - `GET https://admin.mistral.ai/api/billing/v2/usage?month=<m>&year=<y>` — billing breakdown.
  - Dashboard: `https://admin.mistral.ai/organization/usage`. Status: `https://status.mistral.ai`.
- **Cookie shape**: required cookie name `ory_session_*` (Ory Identity prefix). Optional `csrftoken` (sent as `X-CSRFTOKEN` header).
- **Reset windows surfaced**: monthly only (primary, reset = end of month). No secondary/tertiary.
- **Settings keys**:
  - `mistralCookieSource` (default `auto`).
  - `mistralCookieHeader`.
- **Cost/credit semantics**: aggregates token costs across `completion`, `ocr`, `connectors`, `audio`, `libraries_api.{pages,tokens}`, `fine_tuning.{training,storage}` using a price index keyed by `(billingMetric, billingGroup)`. Currency `EUR` by default with `€` symbol.
- **Edge cases**:
  - On 401/403 in `auto` mode, the cache is cleared and a fresh browser import retried.
  - CSRF token extracted from cookie pairs is sent as a header (not a cookie).
- **Mac→Win notes**: HTML/JSON only — easy port.

---

## Moonshot / Kimi API

- **ID / name / tagline**: `moonshot` / "Moonshot / Kimi API" / Moonshot API account balance.
- **Status**: Stable.
- **Auth source(s)**: `MOONSHOT_API_KEY` / `MOONSHOT_KEY` env → config `apiKey`.
- **Endpoints**:
  - International: `GET https://api.moonshot.ai/v1/users/me/balance`.
  - China: `GET https://api.moonshot.cn/v1/users/me/balance`.
  - Region env override: `MOONSHOT_REGION`.
  - Dashboard: `https://platform.moonshot.ai/console/account`.
- **Token shape**: API key (Bearer).
- **Reset windows surfaced**: none — balance-only. Identity line shows balance and deficit.
- **Settings keys**:
  - `moonshotAPIToken`.
  - `moonshotAPIRegion` (`international | china`).
- **Cost/credit semantics**: `available_balance`, `voucher_balance`, `cash_balance` (USD or CNY). When `cash_balance < 0`, surfaces deficit.
- **Edge cases**: response wrapper must satisfy `code == 0 && status == true`.
- **Mac→Win notes**: Trivial. Documented as the modern replacement for the legacy `kimi-k2.ai` provider.

---

## Ollama (Cloud)

- **ID / name / tagline**: `ollama` / "Ollama" / Ollama Cloud session/weekly usage.
- **Status**: Stable.
- **Auth source(s)** (order under `auto`): manual cookie (must contain recognized session cookie) → browser auto-import (Chrome-first, then full order if no recognized session).
- **Endpoints**:
  - `GET https://ollama.com/settings` — HTML page scrape with cookies.
  - Dashboard: `https://ollama.com/settings`.
- **Cookie shapes**: domain `ollama.com` / `www.ollama.com`. Recognized session cookie names: `session`, `__Secure-session`, `ollama_session`, `__Host-ollama_session`, `__Secure-next-auth.session-token`, `next-auth.session-token`, plus chunked variants `*.0`, `*.1`, etc.
- **Reset windows surfaced**: session (primary), weekly (secondary). Resets from `data-time` ISO attributes on "Resets in …" elements.
- **Settings keys**:
  - `ollamaCookieSource` (default `auto`).
  - `ollamaCookieHeader`.
- **Cost/credit semantics**: percent used. Plan tier badge (Free/Pro/Max) parsed from `Cloud Usage` header.
- **Edge cases**:
  - Cookie attached only to `ollama.com` redirect targets — explicit allow-list.
  - Multi-candidate retry: tries each browser/profile session and retries on `notLoggedIn | invalidCredentials | missingUsageData` via `ProviderCandidateRetryRunner`.
- **Mac→Win notes**: HTML scraping, portable.

---

## OpenAI API (platform.openai.com)

- **ID / name / tagline**: `openai` / "OpenAI API" / Legacy `credit_grants` balance.
- **Status**: Stable (note: endpoint is legacy and project keys may not have access).
- **Auth source(s)**: `OPENAI_API_KEY` env → config `apiKey`.
- **Endpoints**:
  - `GET https://api.openai.com/v1/dashboard/billing/credit_grants` (Bearer).
  - Dashboard: `https://platform.openai.com/settings/organization/billing/overview`. Status: `https://status.openai.com`.
- **Token shape**: API key.
- **Reset windows surfaced**: API credits (primary, `nextGrantExpiry` from earliest non-expired grant); identity line shows balance.
- **Settings keys**:
  - `openaiAPIToken`.
- **Cost/credit semantics**: USD `{total_granted, total_used, total_available}` plus per-grant `expires_at` epoch seconds.
- **Edge cases**:
  - HTTP 403 → explicit "Use a legacy/user API key with billing access; project keys may not expose credit grants." — surface this in Windows UI.
  - `ProviderCostSnapshot` written so cost-summary card displays without per-model breakdown.
- **Mac→Win notes**: Trivial. Document the 403/legacy-key caveat in Settings.

---

## OpenCode

- **ID / name / tagline**: `opencode` / "OpenCode" / OpenCode rolling 5-hour + weekly usage.
- **Status**: Stable.
- **Auth source(s)** (order under `auto`): manual cookie → Keychain cache → browser auto-import.
- **Endpoints**:
  - `GET/POST https://opencode.ai/_server?id=<id>&args=<json>` — internal "server function" RPC (TanStack Start-style).
  - Workspace IDs server fn ID: `def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f`.
  - Subscription server fn ID: `7abeebee372f304e050aaaf92be863f4a86490e382f8c79db68fd94040d691b4`.
  - Workspace override env: `CODEXBAR_OPENCODE_WORKSPACE_ID` (accepts `wrk_…` or full `https://opencode.ai/workspace/…` URL).
  - Dashboard: `https://opencode.ai`.
- **Cookie shape**: opaque sign-in cookies from `opencode.ai`. Domain-restricted to `opencode.ai` for header sending.
- **Reset windows surfaced**: rolling 5-hour (primary, reset = `now + rollingUsage.resetInSec`); weekly (secondary).
- **Settings keys**:
  - `opencodeCookieSource` (default `auto`).
  - `opencodeCookieHeader`.
  - `opencodeWorkspaceID` (override, optional).
- **Cost/credit semantics**: percent used.
- **Edge cases**:
  - Responses are `text/javascript` (TanStack Start serialized objects), parsed via a mix of JSON parse + regex (`rollingUsage[^}]*?usagePercent\s*:\s*([0-9]+...)`).
  - GET → POST retry sequence for both endpoints.
  - Multi-strategy parse: JSON dictionary → nested usage object → "candidate window" scan (heuristic best-match for `rolling/hour/5h` vs `weekly/week` keys).
  - "Signed out" detection looks at body text for `"login" | "sign in" | "auth/authorize" | "not associated with an account" | "actor of type \"public\""`.
  - Explicit `null` payload for a missing-subscription state surfaces as `apiError` with workspace context.
- **Mac→Win notes**: Standard HTTP — port directly. Server fn IDs are hard-coded and will need updating when OpenCode rotates them.

---

## OpenCode Go

- **ID / name / tagline**: `opencodego` / "OpenCode Go" / OpenCode Go usage (5-hour / weekly / monthly).
- **Status**: Stable.
- **Auth source(s)**: Same as OpenCode (manual + browser).
- **Endpoints**:
  - Workspace fetch: same `_server` workspaces fn ID as OpenCode.
  - Usage page scrape: `GET https://opencode.ai/workspace/<wrk_id>/go` — HTML page with embedded TanStack Start payload.
  - Workspace override env: `CODEXBAR_OPENCODEGO_WORKSPACE_ID`.
  - Dashboard: `https://opencode.ai` (workspace-specific link computed dynamically).
- **Cookie shape**: same as OpenCode.
- **Reset windows surfaced**: rolling 5-hour (primary), weekly (secondary), **monthly (tertiary)** — Opus slot is used here.
- **Settings keys**:
  - `opencodegoCookieSource`.
  - `opencodegoCookieHeader`.
  - `opencodegoWorkspaceID`.
- **Cost/credit semantics**: percent used.
- **Edge cases**: Same parser-tolerance approach as OpenCode (JSON path + regex fallback + candidate-scan).
- **Mac→Win notes**: Port together with OpenCode — they share the cookie support code (`OpenCodeWebCookieSupport`).

---

## Perplexity

- **ID / name / tagline**: `perplexity` / "Perplexity" / Account credit balance + bonus + purchased.
- **Status**: Stable.
- **Auth source(s)** (order):
  1. Manual cookie (specific cookie name like `__Secure-next-auth.session-token`).
  2. Keychain cache (`cookie.perplexity`).
  3. Browser auto-import.
  4. Env `PERPLEXITY_SESSION_TOKEN` (raw token) or `PERPLEXITY_COOKIE` (cookie header).
- **Endpoints**:
  - `GET https://www.perplexity.ai/rest/billing/credits?version=2.18&source=default` (auth via `Cookie: <name>=<token>` — varies by source).
  - Dashboard: `https://www.perplexity.ai/account/usage`. Status: `https://status.perplexity.com/`.
- **Cookie shape**: opaque `__Secure-next-auth.session-token` or similar (the override carries `requestCookieNames` list — tried in order).
- **Reset windows surfaced**: Credits (primary), Bonus credits (secondary, `weeklyLabel: "Bonus credits"`), Purchased (tertiary, opus slot relabeled).
- **Settings keys**:
  - `perplexityCookieSource`.
  - `perplexityCookieHeader`.
- **Cost/credit semantics**: cents-denominated (`balanceCents`, `totalUsageCents`).
- **Edge cases**:
  - Cookie name varies — `PerplexityCookieOverride` carries multiple candidate cookie names to try.
  - Cached cookie cleared on `invalidToken`; missing-token vs invalid-token surfaced distinctly.
- **Mac→Win notes**: Standard cookie + HTTP. Port directly.

---

## StepFun (阶跃星辰)

- **ID / name / tagline**: `stepfun` / "StepFun" / 5-hour + weekly rate limit.
- **Status**: Stable.
- **Auth source(s)** (order):
  1. Manual Oasis-Token (paste in Settings).
  2. Cached Oasis-Token (`cookie.stepfun`).
  3. Username + password (Settings) → 3-step login flow.
  4. Env `STEPFUN_TOKEN` (raw token).
  5. Env `STEPFUN_USERNAME` + `STEPFUN_PASSWORD` → 3-step login.
- **Endpoints**:
  - Rate limit: `POST https://platform.stepfun.com/api/step.openapi.devcenter.Dashboard/QueryStepPlanRateLimit`.
  - Plan status: `POST .../GetStepPlanStatus`.
  - Login flow:
    1. `GET https://platform.stepfun.com` → captures `INGRESSCOOKIE` from `Set-Cookie`.
    2. `POST https://platform.stepfun.com/passport/proto.api.passport.v1.PassportService/RegisterDevice` (with INGRESSCOOKIE) → anonymous access+refresh token pair.
    3. `POST .../passport/proto.api.passport.v1.PassportService/SignInByPassword` with `{username, password}` → authenticated Oasis-Token.
  - Dashboard: `https://platform.stepfun.com/plan-usage`.
- **Token shape**: `Oasis-Token` (cookie); combined as `<access>...<refresh>` internally. Custom static headers: `oasis-appid: 10300`, `oasis-platform: web`, `oasis-webid: c8a1002d2c457e758785a9979832217c7c0b884c`.
- **Reset windows surfaced**: 5-hour (primary, 300 min window), weekly (secondary, 10 080 min window). Rates expressed as `*_usage_left_rate`; used% = `(1 - left_rate) * 100`.
- **Settings keys**:
  - `stepfunCookieSource` (`auto | manual | off`).
  - `stepfunUsername`.
  - `stepfunPassword`.
  - `stepfunManualToken` (manual mode).
- **Cost/credit semantics**: percent left; plan name (e.g. "Plus", "Mini") via `GetStepPlanStatus`.
- **Edge cases**:
  - The 3-step login is performed inside CodexBar (not via a CLI). On Windows this requires a way to securely store the password and run the same flow.
  - INGRESSCOOKIE extraction reads both Set-Cookie headers and `HTTPCookieStorage.shared` — Rust needs equivalent `Set-Cookie` parsing.
  - Custom `StepFunFlexibleNumber` decoder accepts both int and float; `StepFunFlexibleTimestamp` accepts both string and int.
- **Mac→Win notes**: Storing a *password* (not just a token) requires DPAPI/credential manager on Windows. Consider only supporting Manual + token paths in v1.

---

## Synthetic (synthetic.new)

- **ID / name / tagline**: `synthetic` / "Synthetic" / Rolling 5-hour + weekly + search hourly quotas.
- **Status**: Stable.
- **Auth source(s)**: `SYNTHETIC_API_KEY` env → config `apiKey`.
- **Endpoints**:
  - `GET https://api.synthetic.new/v2/quotas` (Bearer).
  - No dashboardURL (set to `nil`).
- **Token shape**: API key.
- **Reset windows surfaced**: Rolling 5-hour (primary slot 0), Weekly tokens (secondary slot 1), Search hourly (tertiary slot 2 = opus slot relabeled). The snapshot uses a **slotted** quota array so a missing lane stays nil instead of promoting the next lane into the wrong label.
- **Settings keys**:
  - `syntheticAPIToken`.
- **Cost/credit semantics**: tokens (Synthetic uses `messageLimit/messages` and currency-style `maxCredits/remainingCredits` in cents). Provider cost is reported as USD weekly.
- **Edge cases**:
  - Parser is extremely tolerant — scans many key aliases for percent/used/limit/remaining (`messageLimit | maxRequests | quota | …`).
  - Duration parsing recognizes `5hr`, `30min`, `2 days` (and more); suffix table sorted longest-first to avoid mis-matches.
  - Known response keys are slot-mapped: `rollingFiveHourLimit` → slot 0, `weeklyTokenLimit` → slot 1, `search.hourly` → slot 2.
  - Reset description is left nil when a `resetsAt` date is known, so the UI rebuilds the countdown each render (avoids "in Xm" freezing at parse time).
- **Mac→Win notes**: Pure JSON. Port directly.

---

## Venice

- **ID / name / tagline**: `venice` / "Venice" / Venice API balance (DIEM + USD).
- **Status**: Stable.
- **Auth source(s)**: `VENICE_API_KEY` / `VENICE_KEY` env → config `apiKey`.
- **Endpoints**:
  - `GET https://api.venice.ai/api/v1/billing/balance` (Bearer).
  - Dashboard: `https://venice.ai/settings/api`.
- **Token shape**: API key.
- **Reset windows surfaced**: none — balance only. Primary `RateWindow` uses `resetDescription` for the balance string. When `consumptionCurrency != USD` and `diemEpochAllocation > 0`, the primary shows percent used.
- **Settings keys**:
  - `veniceAPIToken`.
- **Cost/credit semantics**: USD balance (`balances.usd`) **and** DIEM credits (`balances.diem`, with optional `diemEpochAllocation` for percent computation). `consumptionCurrency` field picks the active denomination.
- **Edge cases**:
  - When `canConsume=false`, balance is hidden behind "Balance unavailable for API calls".
  - Flexible numeric decoder accepts both Double and "0.0"-style strings.
- **Mac→Win notes**: Trivial.

---

## Warp

- **ID / name / tagline**: `warp` / "Warp" / Warp Terminal request credits + bonus grants.
- **Status**: Stable.
- **Auth source(s)**: `WARP_API_KEY` / `WARP_TOKEN` env → config `apiKey`.
- **Endpoints**:
  - GraphQL: `POST https://app.warp.dev/graphql/v2?op=GetRequestLimitInfo` with query (see below).
  - Reference: `https://docs.warp.dev/reference/cli/api-keys` (token creation).
- **Headers** (required to avoid edge limiter 429):
  - `Authorization: Bearer <key>`
  - `x-warp-client-id: warp-app`
  - `x-warp-os-category: macOS`, `x-warp-os-name: macOS`, `x-warp-os-version: <semver>`
  - `User-Agent: Warp/1.0` (**must match** — edge limiter rejects others as "Rate exceeded.").
- **GraphQL fields**: `requestLimitInfo.{isUnlimited, nextRefreshTime, requestLimit, requestsUsedSinceLastRefresh}`, `bonusGrants[]`, `workspaces[].bonusGrantsInfo.grants[]` (each with `requestCreditsGranted`, `requestCreditsRemaining`, `expiration`).
- **Token shape**: `wk-*` API key.
- **Reset windows surfaced**: monthly credits (primary, reset = `nextRefreshTime`); combined user + workspace bonus credits (secondary, with earliest-expiring batch surfaced).
- **Settings keys**:
  - `warpAPIToken`.
- **Cost/credit semantics**: requests. `isUnlimited=true` shows "Unlimited" badge.
- **Edge cases**:
  - **The User-Agent string must say "Warp/1.0"** — anything else gets HTTP 429 from the edge limiter. On Windows, keep `User-Agent: Warp/1.0` but send `x-warp-os-name: Windows` and `x-warp-os-version` from `RtlGetVersion`.
  - Errors come back as either GraphQL `errors[]` array on 200 OK, or HTTP status with JSON body — both summarized into a single message.
  - `bonusGrants` aggregated across user-level + every workspace; earliest expiry surfaced as a one-line "X credits expires on D".
- **Mac→Win notes**: Adjust `x-warp-os-*` headers but keep `User-Agent: Warp/1.0`.

---

## Windsurf

- **ID / name / tagline**: `windsurf` / "Windsurf" / Daily/weekly quota + flow actions/messages.
- **Status**: Stable (web is preferred; local SQLite is a stale fallback).
- **Auth source(s)** (modes: `auto | web | cli`):
  - Web: manual JSON session bundle OR browser localStorage import (Chromium leveldb).
  - CLI/local: SQLite at `~/Library/Application Support/Windsurf/User/globalStorage/state.vscdb`, key `windsurf.settings.cachedPlanInfo` in `ItemTable`. Windows path: `%APPDATA%\Windsurf\User\globalStorage\state.vscdb`.
- **Endpoints**:
  - `POST https://windsurf.com/_backend/exa.seat_management_pb.SeatManagementService/GetPlanStatus` — ConnectRPC over protobuf.
  - Protobuf request: `{auth_token: string, include_top_up_status: bool}`.
  - Required headers: `Content-Type: application/proto`, `Connect-Protocol-Version: 1`, `Origin: https://windsurf.com`, `Referer: https://windsurf.com/profile`, `x-auth-token`, `x-devin-session-token`, `x-devin-auth1-token`, `x-devin-account-id`, `x-devin-primary-org-id`.
  - Dashboard: `https://windsurf.com/subscription/usage`.
- **Cookie / localStorage shape**:
  - **localStorage keys** (origin `https://windsurf.com`): `devin_session_token`, `devin_auth1_token`, `devin_account_id`, `devin_primary_org_id` — all four required.
  - Manual mode: paste a JSON bundle with the same four keys.
- **Login flow** (manual mode):
  1. Open `windsurf.com/profile`, sign in.
  2. DevTools console: run the documented JS snippet (reads the four `devin_*` keys from localStorage and copies JSON to clipboard).
  3. Paste into Settings → Windsurf → Manual.
- **Reset windows surfaced** (web): daily (primary, reset `daily_quota_reset_at_unix`), weekly (secondary, reset `weekly_quota_reset_at_unix`). Local fallback shows messages (primary) and flow actions (secondary).
- **Settings keys**:
  - `windsurfCookieSource` (default `auto`).
  - `windsurfCookieHeader` (manual JSON bundle).
- **Cost/credit semantics**: percent quota. Plan name + `plan_end` shown as identity.
- **Edge cases**:
  - **Protobuf**: requires generated Rust protobuf bindings (`prost` or `protobuf` crate) for `exa.seat_management_pb`. Code-gen at build time.
  - **localStorage extraction** from Chromium leveldb is the hardest piece — Windsurf is the canonical example of this technique.
  - Local SQLite uses VSCode's `state.vscdb` format; value is BLOB but may be UTF-8 or UTF-16 LE. The parser tries both and verifies the result re-parses as JSON.
- **Mac→Win notes**: SQLite path differs (Windows path above). LevelDB extractor required. Add protobuf code-gen pipeline. **Hard port** overall.

---

## z.ai

- **ID / name / tagline**: `zai` / "z.ai" / Token limit + MCP window + 5-hour window.
- **Status**: Stable.
- **Auth source(s)**: `Z_AI_API_KEY` env → config `apiKey`.
- **Endpoints**:
  - `GET https://api.z.ai/api/monitor/usage/quota/limit` (global).
  - `GET https://open.bigmodel.cn/api/monitor/usage/quota/limit` (China mainland).
  - Override host: `Z_AI_API_HOST=open.bigmodel.cn`; override full URL: `Z_AI_QUOTA_URL=https://open.bigmodel.cn/api/coding/paas/v4`.
  - Dashboard: `https://z.ai/manage-apikey/subscription`.
- **Token shape**: API key (Bearer).
- **Reset windows surfaced**:
  - `TOKENS_LIMIT` → primary (tokens).
  - `TIME_LIMIT` → secondary (MCP window — only when tokens limit also present).
  - Repurposes opus slot label "5-hour" for tertiary.
  - Resets from `nextResetTime` (epoch ms).
- **Settings keys**:
  - `zaiAPIToken`.
  - `zaiAPIRegion` (`global | bigmodel-cn`).
- **Cost/credit semantics**: tokens. Plan label parsed from `data.{planName | plan | plan_type | packageName}`.
- **Edge cases**: Window units accept minutes/hours/days via `data.limits[].window`. `usageDetails[]` per model exposes MCP usage.
- **Mac→Win notes**: Trivial. Two regions in Settings.

---

# Cross-cutting tables

## 1. Auth-type matrix

Rows = providers (long-tail only). Columns: OAuth, Cookies, API key, CLI, Local config/file. ✓ = supported; ✓P = primary/preferred path; — = not used.

| Provider       | OAuth | Cookies | API key | CLI    | Local config/file                |
|----------------|:-----:|:-------:|:-------:|:------:|:--------------------------------:|
| Abacus         | —     | ✓P      | —       | —      | —                                |
| Alibaba        | —     | ✓P      | ✓       | —      | Env vars                         |
| Amp            | —     | ✓P      | —       | —      | —                                |
| Antigravity    | ✓P    | —       | —       | ✓ (local LSP) | `~/.antigravity/oauth_creds.json` |
| Augment        | —     | ✓P      | —       | ✓ (`auggie`) | App Support session JSON      |
| Codebuff       | —     | —       | ✓P      | —      | `~/.config/manicode/credentials.json` |
| CommandCode    | —     | ✓P      | —       | —      | Static plan catalog              |
| Crof           | —     | —       | ✓P      | —      | —                                |
| DeepSeek       | —     | —       | ✓P      | —      | —                                |
| Doubao         | —     | —       | ✓P      | —      | —                                |
| JetBrains      | —     | —       | —       | —      | ✓P XML file                      |
| Kilo           | —     | —       | ✓P      | ✓      | `~/.local/share/kilo/auth.json`  |
| Kimi           | —     | ✓P      | —       | —      | Env `KIMI_AUTH_TOKEN`            |
| Kimi K2        | —     | —       | ✓P      | —      | —                                |
| Kiro           | —     | —       | —       | ✓P     | AWS Builder ID via CLI           |
| Manus          | —     | ✓P      | —       | —      | Env tokens                       |
| MiMo           | —     | ✓P      | —       | —      | —                                |
| MiniMax        | —     | ✓       | ✓P      | —      | localStorage tokens              |
| Mistral        | —     | ✓P      | —       | —      | —                                |
| Moonshot       | —     | —       | ✓P      | —      | —                                |
| Ollama         | —     | ✓P      | —       | —      | —                                |
| OpenAI API     | —     | —       | ✓P      | —      | —                                |
| OpenCode       | —     | ✓P      | —       | —      | —                                |
| OpenCode Go    | —     | ✓P      | —       | —      | —                                |
| Perplexity     | —     | ✓P      | —       | —      | Env session                      |
| StepFun        | —     | ✓ (Oasis-Token cookie) | — | — | Username + password login flow |
| Synthetic      | —     | —       | ✓P      | —      | —                                |
| Venice         | —     | —       | ✓P      | —      | —                                |
| Warp           | —     | —       | ✓P      | —      | —                                |
| Windsurf       | —     | ✓ (localStorage bundle) | — | — | ✓ SQLite `state.vscdb`         |
| z.ai           | —     | —       | ✓P      | —      | —                                |

## 2. Cookie-source matrix

Per provider × browser. Safari is dropped on Windows; Edge / Brave / Chrome forks (Beta/Canary/Arc/Dia/Vivaldi/Chromium) all use the Chromium-fork importer.

| Provider     | Safari* | Chrome | Chrome forks (Beta/Canary/Arc/Dia/Brave/Edge/Vivaldi) | Firefox | Notes |
|--------------|:-------:|:------:|:----:|:-------:|-------|
| Abacus       | ✓ → drop | ✓P  | default order | ✓ | Chrome-first, then full fallback |
| Alibaba      | ✓ → drop | ✓P  | Chrome → Beta → Brave → Edge → Arc → Firefox → Safari | ✓ | Custom order; needs Chromium DB fallback for locked DBs |
| Amp          | ✓ → drop | ✓   | default order | ✓ | Single cookie `session` |
| Augment      | ✓ → drop | ✓P  | Custom: Safari → Chrome → Chrome Beta → Chrome Canary → Edge → Edge Beta → Brave → Arc → Dia → Arc Beta → Firefox | ✓ | Auth0 / NextAuth / AuthJS cookies |
| CommandCode  | ✓ → drop | ✓   | default order | ✓ | better-auth cookies |
| Kimi         | ✓ → drop | ✓P  | Arc → Chrome → Safari → Edge → Brave → Chromium | — | JWT cookie `kimi-auth` |
| Manus        | ✓ → drop | ✓   | default order | ✓ | `session_id` cookie |
| MiMo         | ✓ → drop | ✓P  | Custom: Chrome → Chrome Beta → Chrome Canary | — | Chromium-only |
| MiniMax      | ✓ → drop | ✓   | default order | ✓ | Also uses localStorage from Chromium |
| Mistral      | ✓ → drop | ✓   | default order | ✓ | `ory_session_*` cookies |
| Ollama       | ✓ → drop | ✓P  | default fallback | ✓ | Chrome-first; recognized session names list |
| OpenCode     | ✓ → drop | ✓   | default order | ✓ | TanStack Start session |
| OpenCode Go  | ✓ → drop | ✓   | default order | ✓ | Same auth as OpenCode |
| Perplexity   | ✓ → drop | ✓   | default order | ✓ | NextAuth-style cookies |
| Windsurf     | ✓ → drop | ✓P  | Chrome / Edge / Brave / Arc / Vivaldi / Chromium | — | **localStorage** not cookies |

\* Safari is macOS-only — drop on Windows port. All providers that supported Safari should keep importing from Chrome/Chromium-fork/Edge/Firefox on Windows.

## 3. Reset-window matrix

Per provider × window. ✓ where mapped; — where not surfaced.

| Provider     | Session (5h) | Daily | Weekly | Monthly | Annual | Balance-only / Lifetime |
|--------------|:-----:|:-----:|:------:|:-------:|:------:|:----:|
| Abacus       | —     | —     | —      | ✓ (billing) | —  | —    |
| Alibaba      | ✓     | —     | ✓      | ✓       | —      | —    |
| Amp          | rolling | rolling | — | — | — | —    |
| Antigravity  | per-model | per-model | per-model | per-model | — | — |
| Augment      | —     | —     | —      | ✓ (billing) | —  | —    |
| Codebuff     | —     | —     | ✓      | ✓ (billing) | —  | —    |
| CommandCode  | —     | —     | —      | ✓       | —      | —    |
| Crof         | —     | ✓ (CST midnight) | — | — | — | ✓ ($ balance) |
| DeepSeek     | —     | —     | —      | —       | —      | ✓    |
| Doubao       | ✓ (header-driven) | — | — | — | — | — |
| JetBrains    | —     | —     | —      | ✓ (tariff) | —   | —    |
| Kilo         | —     | —     | —      | ✓ (Kilo Pass) | — | ✓ ($ balance) |
| Kimi         | ✓ (5h)| —     | ✓      | —       | —      | —    |
| Kimi K2      | —     | —     | —      | —       | —      | ✓    |
| Kiro         | —     | —     | —      | ✓       | —      | bonus expiry |
| Manus        | —     | ✓ refresh | — | ✓ Pro monthly | — | ✓ ($ balance) |
| MiMo         | —     | —     | —      | ✓ token plan | —  | ✓ ($ balance) |
| MiniMax      | ✓ (5h)| —     | —      | —       | —      | —    |
| Mistral      | —     | —     | —      | ✓ (billing) | —  | —    |
| Moonshot     | —     | —     | —      | —       | —      | ✓    |
| Ollama       | ✓ session | — | ✓ | —     | —      | —    |
| OpenAI API   | —     | —     | —      | ✓ (grant expiry) | — | ✓ |
| OpenCode     | ✓ rolling-5h | — | ✓ | — | — | —    |
| OpenCode Go  | ✓ rolling-5h | — | ✓ | ✓ | — | —    |
| Perplexity   | —     | —     | —      | ✓ (billing) | —  | ✓ (purchased) |
| StepFun      | ✓ (5h)| —     | ✓      | —       | —      | —    |
| Synthetic    | ✓ rolling-5h | — | ✓ tokens | — | — | hourly search |
| Venice       | —     | —     | —      | ✓ (epoch alloc) | — | ✓ |
| Warp         | —     | —     | —      | ✓       | —      | bonus expiry |
| Windsurf     | —     | ✓     | ✓      | —       | —      | plan_end |
| z.ai         | ✓ (5h)| —     | —      | —       | —      | tokens window |

## 4. Refresh-complexity ranking — port effort estimate

### Easy (pure HTTP + JSON; port in hours per provider)

Abacus, Amp (HTML scrape), Codebuff, CommandCode, Crof, DeepSeek, Doubao, Kimi K2, Manus, MiMo, Mistral, Moonshot, Ollama (HTML scrape), OpenAI API, Perplexity, Synthetic, Venice, Warp, z.ai.

### Medium (multi-step or tRPC/GraphQL/protobuf-light)

- **Alibaba**: dual region + console RPC fallback + custom Chromium-DB fallback for cookies (uses macOS Keychain Safe Storage; replace with DPAPI on Windows).
- **Augment**: dual transport (CLI + cookies), custom keepalive scheduler, on-disk session JSON.
- **JetBrains**: local XML scrape across many IDE paths. Pure file IO, but path enumeration is fiddly.
- **Kilo**: tRPC batch with optional procedures + CLI auth-file fallback.
- **Kimi**: HTTP + JWT decode + many required Connect-RPC headers.
- **OpenCode** / **OpenCode Go**: text/javascript responses parsed with regex; hard-coded server fn IDs may rotate.
- **MiniMax**: cookies + API tokens + Chromium localStorage extraction.
- **Mistral**: standard cookies + CSRF.

### Hard (PTY/CLI/sqlite/scraping/leveldb; days per provider)

- **Antigravity**: local LSP HTTP probe with `lsof` and `ps` (Windows needs `Get-NetTCPConnection`/`netstat`). Two OAuth client modes. Self-signed TLS.
- **Kiro**: subprocess PTY-like usage (Foundation `Process` + readability handlers + idle timeout). Output is ANSI-decorated and regex-parsed. Watchdog logic required.
- **StepFun**: in-process 3-step login flow that includes plaintext username + password — needs DPAPI secret storage on Windows. INGRESSCOOKIE extraction.
- **Windsurf**: leveldb (Chromium localStorage) reader **plus** ConnectRPC + protobuf request/response. Local SQLite fallback. Hardest single provider in this set.
- **Augment** (keepalive runtime, secondary CLI mode).

## 5. Provider-specific UI features

Listed only when a provider departs from the standard "3 RateWindows + identity line + cookie/API-token settings" UI:

| Provider     | Custom UI element                                                                 |
|--------------|------------------------------------------------------------------------------------|
| Abacus       | Trailing text "Cached: <source> • <relative-time>" under cookie picker.            |
| Alibaba      | API region picker (intl/cn) with dynamic auto-fallback on credential errors.       |
| Augment      | Menu actions: "Refresh Session", "Open Augment (Log Out & Back In)" on errors.     |
| Antigravity  | Interactive Google OAuth login flow + waiting-on-browser status phase.             |
| Codebuff     | Plan tier badge ("Pro") + auto-top-up state in identity line.                      |
| JetBrains    | IDE picker dropdown (auto-detect / installed IDE list) + custom-path field.        |
| Kimi         | Tier mapping table (Andante / Moderato / Allegretto) documented in card help.      |
| Kiro         | Plan-name parsing handles managed plans without usage metrics.                     |
| Manus        | Identity line shows raw credit count + monthly/refresh detail strings.             |
| MiniMax      | **Dual-mode UI** — region picker + auth-mode-aware cookie/token field visibility.  |
| Moonshot     | API region picker (international/china).                                           |
| OpenCode/Go  | Workspace-ID override field; dashboardURL is computed from workspace ID.           |
| Perplexity   | Tertiary lane relabeled "Purchased" (opus slot reused).                            |
| StepFun      | **Username + password fields** (rare!) with manual-token override + login state.   |
| Synthetic    | Tertiary lane relabeled "Search hourly" (opus slot reused).                        |
| Warp         | "Unlimited" badge when `isUnlimited=true`; bonus-expiry line below primary bar.    |
| Windsurf     | Manual mode requires a *JSON bundle* (4 keys) — not a cookie header. Console snippet helper documented.       |
| z.ai         | Two regions + tertiary lane relabeled "5-hour" (opus slot reused).                 |

---

# Implementation notes for the Windows port

## File-path mapping

| macOS                                                                  | Windows (`%APPDATA%` = `C:\Users\<u>\AppData\Roaming`)                     |
|------------------------------------------------------------------------|----------------------------------------------------------------------------|
| `~/.codexbar/config.json`                                              | `%APPDATA%\CodexBar\config.json`                                           |
| `~/Library/Application Support/CodexBar/`                              | `%APPDATA%\CodexBar\`                                                      |
| `~/Library/Application Support/CodexBar/augment-session.json`          | `%APPDATA%\CodexBar\augment-session.json`                                  |
| `~/Library/Application Support/Windsurf/User/globalStorage/state.vscdb`| `%APPDATA%\Windsurf\User\globalStorage\state.vscdb`                        |
| `~/Library/Application Support/JetBrains/<IDE>/options/AIAssistantQuotaManager2.xml` | `%APPDATA%\JetBrains\<IDE>\options\AIAssistantQuotaManager2.xml` |
| `~/.config/manicode/credentials.json` (Codebuff)                       | `%USERPROFILE%\.config\manicode\credentials.json`                          |
| `~/.local/share/kilo/auth.json`                                        | `%LOCALAPPDATA%\kilo\auth.json` (verify in Kilo CLI source)                |
| `~/Library/Cookies/Cookies.binarycookies` (Safari)                     | n/a — drop                                                                 |
| Chromium `Cookies` SQLite + Safe Storage Keychain                      | Chromium `Cookies` SQLite + DPAPI-encrypted `Local State` v10/v11 keys     |

## Common patterns to extract once

- **`ProviderTokenResolver`** — env var → config → token-account lookup precedence. The exact env var names per provider are listed under "Auth source(s)" in each entry. A central Rust function `resolve_provider_token(provider, env, settings) -> Option<String>` keeps individual fetchers small.
- **`CookieHeaderCache`** — Keychain-backed JSON cache `{cookieHeader, sourceLabel, storedAt}` keyed by provider. Used by ~12 providers. On Windows: encrypt to `%APPDATA%\CodexBar\cookies\<provider>.json` with DPAPI per-user.
- **`CookieHeaderNormalizer.normalize`** / `pairs` — strip `Cookie: ` prefix, trim, split on `;` and parse `name=value` pairs. Implement once.
- **`ProviderCandidateRetryRunner`** — try a list of cookie candidates with `(error) -> shouldRetry` predicate; on `noCandidates`, surface the original error.
- **Browser cookie import order** — `ProviderBrowserCookieDefaults.defaultImportOrder` (Safari → Chrome → Chrome forks → Firefox). On Windows: Chrome → Edge → Brave → Chrome Beta → Chromium → Arc → Vivaldi → Firefox. A few providers override this (Alibaba, Augment, MiMo, Ollama, Kimi).

## Things worth dropping in v1 (Windows)

- **Antigravity local LSP mode** — until there's a Windows Antigravity install to test against, ship OAuth-only.
- **StepFun password storage** — drop password-based auth in v1, ship Manual-token + env-var paths only.
- **Augment session keepalive** — the secondary "stored cookies" file on disk is enough.
- **Kiro CLI** — the entire provider is CLI-gated; defer until you have a Windows `kiro-cli.exe` to test.

## Order of porting (recommended)

1. **Easy batch (1–2 days total)**: Crof, DeepSeek, Kimi K2, Moonshot, Venice, Warp, OpenAI API, Synthetic, z.ai, Codebuff, Doubao — pure HTTP + JSON, no cookies.
2. **HTML/JSON-cookie batch**: Abacus, Amp, Ollama, Mistral, CommandCode, Augment (cookie path only), Perplexity, Kimi, Manus, MiMo.
3. **Multi-region + tRPC**: Alibaba, MiniMax, Kilo (API path only).
4. **Local file scrapers**: JetBrains (XML), Windsurf (SQLite path only).
5. **Hard ports** (only if scope allows): Windsurf full (leveldb + protobuf), Antigravity OAuth, Kiro CLI, StepFun full login flow, OpenCode/Go (TanStack Start regex).

---

# Notable cross-provider observations

- **Manual-cookie escape hatch is universal** — every cookie-using provider supports pasting a `Cookie:` header so users can self-rescue when browser import fails.
- **macOS Keychain cache is the second line of defense** — `CookieHeaderCache` is used by Abacus, Alibaba, Augment, CommandCode, Kimi, Manus, MiMo, MiniMax, Mistral, Ollama, OpenCode, OpenCode Go, Perplexity, StepFun. Each entry stores `{cookieHeader, sourceLabel, storedAt}`. **This is the single biggest portability decision for Windows** — use DPAPI per-user with a one-line wrapper.
- **Several providers send synthetic chat requests as a probe** (Doubao: `max_tokens=1` "hi") — Windows UI should warn users about cost where applicable.
- **TanStack Start "server functions"** (`/_server?id=<hash>`) used by OpenCode + OpenCode Go are response-format-fragile — the server-fn IDs are hard-coded and will need to be updatable at runtime.
- **Connect-RPC headers** (`Connect-Protocol-Version: 1`) appear in Kimi, Manus, Windsurf, Antigravity. Build a shared HTTP client wrapper.
- **`User-Agent`-pinned providers**: Warp (must be `Warp/1.0`), Kimi, Ollama, Amp, Manus, MiMo, MiniMax — all send a desktop-Chrome-style UA. Centralize the UA string and version.
- **localStorage extraction** (Chromium leveldb) is used by Windsurf (`devin_*` keys) and MiniMax (access tokens). Build it once.
- **Two providers reuse the opus slot** as a relabeled "tertiary" lane: Synthetic ("Search hourly"), Perplexity ("Purchased"), z.ai ("5-hour"), OpenCode Go ("Monthly"). The UI treats all three slots uniformly; the labels come from `ProviderMetadata.opusLabel`.
