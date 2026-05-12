---
title: "Phase 5: Codex Provider"
status: planning
audience: Rust + TypeScript engineer implementing the Codex provider on Tauri 2 + React
related_specs:
  - docs/windows/spec/41-provider-codex.md
  - docs/windows/spec/30-provider-system-architecture.md
  - docs/windows/spec/60-auth-cookies-secrets.md
phase: 5
predecessors: [phase-0, phase-1, phase-2, phase-3, phase-4]
successors: [phase-6]
length_target: 700 to 1400 lines
---

# Phase 5: Codex Provider (end to end)

## Why this phase exists

Codex is the most architecturally complex provider in CodexBar. Three independent backends (OAuth API, local CLI over JSON-RPC, OpenAI web dashboard) must be layered, a multi-account promotion flow must safely rewrite the live system `auth.json` without ever clobbering work, and a JWT identity model must scope every credit, every history bucket, and every cookie jar to the right human. We cannot fake this. If the Codex card is wrong, the whole app feels wrong because Codex is the namesake provider.

The phase delivers Codex end to end so that:

1. A user who has installed the OpenAI Codex CLI and run `codex login` sees a live Codex card with session and weekly bars within one refresh tick of opening the app.
2. A user who has not installed the CLI but has cookies for `chatgpt.com` in Chrome or Edge sees the same card, populated via the web scraper.
3. A user with multiple Codex accounts can switch which one drives the local `codex` command without losing the previous account's auth.
4. The credits-history chart, code-review row, and Buy Credits CTA all light up exactly as they do on macOS.

This phase is large. It is split into atomic commits per subsystem so that each lands independently, with its own tests and acceptance check, and so the reviewer never has to read more than a few hundred lines per commit.

## Dependencies

This phase assumes the following are complete and merged on `main`:

- Phase 0: workspace bootstrap, Tauri shell, Rust crate skeleton (`rust/src/providers/`), settings store, tray icon plumbing.
- Phase 1: `host` services (`http`, `keyring`, `dpapi`, `conpty`, `jsonl-scanner`, `locale`, `log`) all implemented and unit-tested.
- Phase 2: provider framework (`ProviderDescriptor`, `ProviderFetchPlan`, `Strategy` trait, `UsageStore`, `ProviderRuntime`) per spec 30.
- Phase 3: settings IPC bridge, `ProviderSettingsSnapshot`, `SettingsDescriptor` rendering on the TS side.
- Phase 4: Claude provider shipped (validates the Strategy trait under real conditions, validates DPAPI secret storage, validates `reqwest::cookie::Jar` per account).

If any of those are not in place, halt and fix that phase first. Codex will trip every weak edge in the framework.

## Deliverables

By the end of Phase 5, the following must exist and pass acceptance tests:

### A. OAuth API path

1. ChatGPT OAuth client id wired (`app_EMoamEEZ73f0CkXaXp7hrann`).
2. Endpoints: refresh at `POST https://auth.openai.com/oauth/token`, usage at `GET https://chatgpt.com/backend-api/wham/usage`, alt at `GET {chatgpt_base_url}/api/codex/usage`.
3. JWT identity extraction with the exact three-fallback ladder per spec 41 §3.3.
4. 8-day refresh trigger (`now - last_refresh > 8 days`).
5. Tolerant decode: per-window decode failures are isolated; partial results returned.
6. Credential storage: DPAPI-wrapped at `%APPDATA%\CodexBar4Windows\secrets\codex.json` plus the canonical live file at `%USERPROFILE%\.codex\auth.json`. Shape compatible with the Mac `credentials.json` (snake_case write, both casings on read).

### B. Codex CLI integration

1. JSON-RPC 2.0 over `stdin`/`stdout` of `codex -s read-only -a untrusted app-server`. Line-delimited framer. No PTY.
2. Method order: `initialize` then `account/read` then `account/rateLimits/read`.
3. Timeouts: ~5 to 8 s for `initialize`, ~2 to 3 s for subsequent reads. Kill child on timeout.
4. ConPTY (via `portable_pty::native_pty_system()`) reserved only for the manual `/status` diagnostic.
5. Subprocess lifecycle: `tokio::process::Command` plus Win32 `JobObject` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so children die when the parent dies.

### C. OpenAI web dashboard extras

1. Cookie source ladder per spec 41 §5.3.
2. Chromium v10 cookie decryption via DPAPI per spec 60 §4.5.1. Hard skip on v20 (App-Bound Encryption) with a clear cooldown and a manual-paste banner.
3. Per-account `reqwest::cookie::Jar` serialized to `%LOCALAPPDATA%\CodexBar\openai-dashboard-jars\<email-uuid>.json`.
4. Optional WebView2 first-time interactive login pointed at `https://chatgpt.com/codex/cloud/settings/analytics#usage`, cookies extracted via `ICoreWebView2_2.CookieManager`.
5. Scrape script: the macOS `openAIDashboardScrapeScript` shipped verbatim as a Rust string constant. Returned dict shape preserved exactly.
6. Regex parsers (locale-aware: US `1,234.56`, EU `1.234,56`, thin spaces `U+202F` and `U+00A0`).
7. Surfaces: code review remaining, usage breakdown rows (with `skillusage:*` filter), credits-history chart data, plan, signed-in email.
8. Buy Credits flow: a Tauri-managed WebView2 window pointed at `dashboardSnapshot.creditsPurchaseURL` (fallback `https://chatgpt.com/codex/cloud/settings/usage`).

### D. Account promotion flow

1. Vocabulary fixed: managed accounts, unmanaged accounts (the live system account is "live", not "unmanaged"), adjacent multi-account veto.
2. Five-file split: `CodexAccountPromotionCoordinator`, `Service`, `Preparation`, `Planning`, `Execution`.
3. Pure planner decision matrix returning one of `none / reject / importNew / refreshExisting / repairExisting`.
4. Safety contract: the executor never swaps live auth. The service does the swap last.
5. Verbatim error string table per spec 41 §6.10.
6. State machine diagram baked into source comments and docs.

### E. Managed accounts

1. v1 to v2 schema migration. Idempotent.
2. Sandbox validation: every write or delete inside `%LOCALAPPDATA%\CodexBar\managed-codex-homes\<UUID>` goes through `validate_managed_home_for_deletion`.
3. Canonical key scheme per spec 41 §7.4.
4. `hasAdjacentMultiAccountVeto` flag plumbed through `CodexOwnershipContext`.

### F. History ownership

1. `CodexHistoryOwnership::belongs_to_target_continuity` mirrors the Mac semantics.
2. Legacy key classification (`canonical`, `legacyEmailHash`, `legacyOpaqueScoped`, `legacyUnscoped`).
3. Cross-account leakage impossible by construction (every store write is keyed by canonical key).

### G. UI

1. Codex provider card with session, weekly, and (when present) tertiary bars; credits balance row; monthly spend row; Buy Credits CTA; code-review remaining row when present; usage breakdown disclosure.
2. Settings pane: Codex usage source picker (`auto` / `oauth` / `cli`), OpenAI dashboard extras toggle, managed-account list editor, manual cookie textarea.
3. Multi-account UI: account switcher bar OR stacked cards, capped at 6, governed by Preferences > Advanced > Display.

### H. Tests

1. Fixture-based JWT parsing tests (one fixture per ladder cell in spec 41 §3.3).
2. Planner decision matrix unit tests, one per cell.
3. Promotion executor safety test: simulate every failure path, assert live auth was not swapped.
4. Migration test: v1 schema in, v2 schema out, idempotent (running twice yields the same bytes).

---

## Atomic commit task list

Each task is a single commit (or, where called out, a pair of commits when the diff would otherwise exceed roughly 400 lines). Every task lists: title, files touched, acceptance check, draft conventional commit message.

> Branch policy reminder: per `CLAUDE.md` we commit directly to `main` after each atomic task. Each commit must compile, must pass `cargo test -p codexbar-core` if applicable, and must not introduce a TypeScript compile error. Conventional commit format. No em dashes in commit messages or in code comments.

### Group 1: OAuth path

#### Task 1.1: Codex credential file shape + read or write

- Files:
  - `rust/src/providers/codex/auth/credentials.rs` (new)
  - `rust/src/providers/codex/auth/mod.rs` (new)
  - `rust/src/providers/codex/mod.rs` (new module shell)
- Acceptance:
  - Read accepts both snake_case (`access_token`) and camelCase (`accessToken`).
  - Write always emits snake_case.
  - `OPENAI_API_KEY`-only files load as a degraded `ApiKeyOnly` variant.
  - File path resolves to `%CODEX_HOME%\auth.json` if `CODEX_HOME` is set, else `%USERPROFILE%\.codex\auth.json`.
  - Atomic write: stage to `auth.json.tmp.<uuid>`, set owner-only ACL, then `tokio::fs::rename`.
  - Unit test: round-trip a credentials struct, then read back; assert byte-identical output for the second write.
- Commit:
  ```
  feat(codex): add credential file reader and writer

  Reads and writes %USERPROFILE%\.codex\auth.json with both snake and camel
  case field names. Atomic write via staged file plus rename. Owner-only ACL
  applied before rename so the final file inherits restricted permissions.
  ```

#### Task 1.2: JWT identity extractor

- Files:
  - `rust/src/providers/codex/auth/jwt.rs` (new)
  - `rust/src/providers/codex/auth/identity.rs` (new, the `CodexIdentity` enum)
  - `rust/src/providers/codex/auth/tests/jwt_fixtures/` (new dir, six fixtures)
- Acceptance:
  - Three fallback paths for email: `payload.email`, then `payload["https://api.openai.com/profile"].email`, then `None`.
  - Three fallback paths for plan: `payload["https://api.openai.com/auth"].chatgpt_plan_type`, then `payload.chatgpt_plan_type`, then `None`.
  - Three fallback paths for account id: `tokens.account_id`, then `payload["https://api.openai.com/auth"].chatgpt_account_id`, then `payload.chatgpt_account_id`.
  - Trim and lowercase email. Normalize account id (lowercased, trimmed).
  - Tolerant decode: a malformed JWT produces `CodexIdentity::Unresolved`, never a panic.
  - Six unit-test fixtures, one per ladder cell.
- Commit:
  ```
  feat(codex): extract identity from id_token JWT

  Three-fallback ladder for email, plan, and account id matching the macOS
  reference. Tolerant decode: per-field failures isolated, no panics on
  malformed payloads. Six fixture-driven unit tests cover every ladder cell.
  ```

#### Task 1.3: Refresh flow with error mapping

- Files:
  - `rust/src/providers/codex/auth/refresh.rs` (new)
  - `rust/src/providers/codex/auth/errors.rs` (new)
- Acceptance:
  - POST body matches spec 41 §3.4 exactly (`client_id`, `grant_type`, `refresh_token`, `scope`).
  - Trigger predicate: `now - last_refresh > Duration::from_secs(8 * 24 * 3600)`.
  - 30 s timeout.
  - On 401, parse `error.code`, fallback to `error`, fallback to `code`. Map to `RefreshError::Expired`, `Reused`, `Revoked` per the table.
  - Unknown code maps to `RefreshError::Expired` (terminal).
  - On success, persist new triple plus `last_refresh = now.iso8601_with_fractional()`.
  - Unit test: mock `httpmock` server, exercise each 401 code branch.
- Commit:
  ```
  feat(codex): implement 8-day OAuth refresh against auth.openai.com

  POST /oauth/token with the documented body and client id. Maps 401 error
  codes (refresh_token_expired, refresh_token_reused, invalid_grant,
  refresh_token_invalidated) to the RefreshError enum. Unknown codes treated
  as terminal expired. httpmock unit tests cover every mapped branch.
  ```

#### Task 1.4: Usage API call and tolerant decode

- Files:
  - `rust/src/providers/codex/oauth/usage.rs` (new)
  - `rust/src/providers/codex/oauth/wham_response.rs` (new)
- Acceptance:
  - Request headers: `Authorization: Bearer <token>`, `Accept: application/json`, `Accept-Language: en-US,en;q=0.9`, `User-Agent: CodexBar`, `ChatGPT-Account-Id: <id>` if known.
  - Timeout 10 s.
  - URL resolution: default `https://chatgpt.com/backend-api/wham/usage`, alt path when `chatgpt_base_url` lacks `/backend-api`.
  - `plan_type` open enum: known tiers plus `Unknown(String)`.
  - Per-window decode failures isolated: if `primary_window` fails, primary becomes `None`, `secondary_window` and `credits` still returned. Telemetry flag `primary_window_decode_failed` set.
  - Partial results (credits-only) returned without escalating to CLI fallback.
- Commit:
  ```
  feat(codex): call wham/usage and decode tolerantly

  Sends the exact headers (Authorization, Accept-Language, User-Agent,
  ChatGPT-Account-Id) per spec. Window decode failures isolated so a
  partial response still produces a usable snapshot. Unknown plan tiers
  map to Unknown(String). Path-style URL resolution covers the alt
  /api/codex/usage endpoint.
  ```

#### Task 1.5: should_fallback predicate

- Files:
  - `rust/src/providers/codex/oauth/fallback.rs` (new)
  - `rust/src/providers/codex/oauth/tests/fallback_truth_table.rs` (new)
- Acceptance:
  - Function signature: `pub fn should_fallback(err: &CodexOAuthError, mode: SourceMode) -> bool`.
  - Returns true only for `Unauthorized`, `CredentialsNotFound`, `CredentialsMissingTokens`, `RefreshExpired`, `RefreshRevoked`, `RefreshReused` and only when `mode == SourceMode::Auto`.
  - Returns false for `InvalidResponse`, `ServerError`, `NetworkError`, `DecodeFailed`, `RefreshNetworkError`, `RefreshInvalidResponse`.
  - Truth-table test covers every variant.
- Commit:
  ```
  feat(codex): add should_fallback predicate with truth-table tests

  Mirrors the macOS predicate exactly: fall back only on recoverable auth
  states the CLI can actually fix. Hard mode pins (oauth or cli) never
  fall back. Truth-table test asserts every CodexOAuthError variant.
  ```

#### Task 1.6: DPAPI-wrapped credentials mirror

- Files:
  - `rust/src/providers/codex/auth/dpapi_mirror.rs` (new)
- Acceptance:
  - Write to `%APPDATA%\CodexBar4Windows\secrets\codex.json` wraps the credential JSON via `CryptProtectData` (user scope, no entropy, `CRYPTPROTECT_UI_FORBIDDEN`).
  - On boot, if the live `~/.codex/auth.json` is missing but the DPAPI mirror exists, the mirror seeds a fresh live file (after a one-time user prompt).
  - Drop-in compatibility test: a Mac `credentials.json` byte stream is accepted by the reader without modification.
- Commit:
  ```
  feat(codex): mirror credentials into a DPAPI-wrapped sidecar

  Writes %APPDATA%\CodexBar4Windows\secrets\codex.json next to the canonical
  ~/.codex/auth.json so that uninstall does not lose tokens and so that
  Mac users dropping in a credentials.json file are accepted unchanged.
  ```

### Group 2: CLI integration

#### Task 2.1: Binary locator

- Files:
  - `rust/src/providers/codex/cli/binary_locator.rs` (new)
- Acceptance:
  - Lookup order: `%PATH%` first, then `%LOCALAPPDATA%\Programs\codex\codex.exe`, then `%USERPROFILE%\.bun\bin\codex.exe`.
  - Returns `BinaryNotFound` with a precise message when nothing is found.
  - Unit test mocks the filesystem.
- Commit:
  ```
  feat(codex): locate the codex binary on Windows

  Searches PATH, the Programs install folder, and the Bun global folder
  before reporting BinaryNotFound. Unit test covers each lookup arm.
  ```

#### Task 2.2: JSON-RPC framer

- Files:
  - `rust/src/providers/codex/cli/rpc_framer.rs` (new)
- Acceptance:
  - Encodes outgoing `Request` as one JSON object per line, UTF-8, LF-terminated.
  - Decodes incoming lines as `Response` or `Notification`.
  - Handles partial reads correctly (line buffer).
  - Unit test: feed a stream of bytes split mid-message; assert reassembly.
- Commit:
  ```
  feat(codex): add line-delimited JSON-RPC framer

  Encodes one JSON object per line over stdio. Decoder buffers partial
  reads and reassembles split frames. Test feeds a byte-split stream
  and asserts complete message recovery.
  ```

#### Task 2.3: Subprocess lifecycle with JobObject

- Files:
  - `rust/src/host/process_group.rs` (new)
  - `rust/src/providers/codex/cli/subprocess.rs` (new)
- Acceptance:
  - Spawns `codex -s read-only -a untrusted app-server` with stdin and stdout piped.
  - Assigns the child to a `JobObject` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
  - Parent exit kills the entire process tree.
  - Test: spawn a dummy long-running child, kill the parent, assert grandchild dies within 2 s.
- Commit:
  ```
  feat(codex): wrap CLI subprocess in a Win32 JobObject

  Assigns spawned children to a job object with KILL_ON_JOB_CLOSE so the
  whole process tree dies when CodexBar exits. Generalizes into a host
  utility so other providers can reuse it.
  ```

#### Task 2.4: RPC strategy (initialize, account/read, rateLimits/read)

- Files:
  - `rust/src/providers/codex/cli/rpc_strategy.rs` (new)
- Acceptance:
  - Method order respected.
  - Long startup budget for `initialize` (configurable, default 6 s).
  - Short read budget for the others (configurable, default 2.5 s).
  - On timeout, kill the child via the JobObject so the reader unwinds; no orphans.
  - App-server error containing a `wham/usage` JSON blob is parsed and used; otherwise the error is terminal for this strategy.
  - Integration test: a fake server replays a known transcript.
- Commit:
  ```
  feat(codex): implement CLI RPC strategy over stdio

  Calls initialize, account/read, account/rateLimits/read in order with
  the documented timeout budgets. On timeout, kills the child via the
  process group so the stdout reader unwinds cleanly. App-server errors
  embedding a wham/usage blob are parsed as a recoverable usage response.
  ```

#### Task 2.5: ConPTY /status diagnostic

- Files:
  - `rust/src/providers/codex/cli/pty_diagnostic.rs` (new)
  - `rust/src/providers/codex/cli/status_probe.rs` (new)
- Acceptance:
  - Uses `portable_pty::native_pty_system()` (ConPTY on Windows 10+).
  - Steps mirror spec 41 §4.2 exactly: drain 400 ms, answer cursor-position queries (`ESC [ 6 n` to `ESC [ 1;1 R`), dismiss update prompt, send `/status\r` (up to 2 resends, up to 6 Enter retries), wait for marker (`Credits:`, `5h limit`, `5-hour limit`, `Weekly limit`), drain 2 s after marker.
  - Regex parsers per spec 41 §4.3 table.
  - Update-needed detection: any of `Update available!`, `Run bun install -g @openai/codex`, `0.60.1 ->` triggers "CLI update needed".
  - Only invoked from Preferences > Debug > Run /status diagnostic; never from automatic refresh.
- Commit:
  ```
  feat(codex): add ConPTY /status diagnostic for debug menu

  Runs codex in a real ConPTY only for the manual diagnostic entry in
  Preferences > Debug. Auto-dismisses the update prompt, answers cursor
  position queries to keep the TUI happy, sends /status, waits for the
  marker, and parses the rendered panel via regex.
  ```

### Group 3: Web dashboard extras

#### Task 3.1: Cookie source state machine

- Files:
  - `rust/src/providers/codex/web/cookie_source.rs` (new)
- Acceptance:
  - Enum: `Off`, `Auto`, `Manual`.
  - `Off` disables web extras entirely; also forced when `openAIWebAccessEnabled = false`.
  - Stored under `providers.codex.cookieSource` in `%LOCALAPPDATA%\CodexBar\settings.json`.
- Commit:
  ```
  feat(codex): persist OpenAI cookie source mode

  Tri-state setting (off, auto, manual) stored in the settings JSON and
  exposed via the provider settings descriptor. Off is the secure default.
  ```

#### Task 3.2: Chromium v10 cookie decryption

- Files:
  - `rust/src/host/cookies/chromium_v10.rs` (new)
  - `rust/src/host/cookies/local_state.rs` (new)
- Acceptance:
  - Parses `os_crypt.encrypted_key` from `Local State`.
  - Strips the leading 5-byte `DPAPI` prefix.
  - Calls `CryptUnprotectData` with `CRYPTPROTECT_UI_FORBIDDEN`, no entropy, user scope.
  - For each `encrypted_value` starting with `b"v10"`: 12-byte nonce, then `ciphertext || 16-byte GCM tag`, AES-256-GCM decrypt.
  - Empty `encrypted_value`: falls back to plaintext `value`.
  - Legacy non-v10 non-empty: raw `CryptUnprotectData` (rare path).
- Commit:
  ```
  fix(host): decrypt Chromium v10 cookies via DPAPI plus AES-256-GCM

  Reads os_crypt.encrypted_key from Local State, strips the DPAPI prefix,
  and unwraps via CryptUnprotectData. Each v10 cookie blob decoded as
  12-byte nonce plus ciphertext plus 16-byte GCM tag. Legacy and empty
  blobs handled per spec 60.
  ```

#### Task 3.3: Chromium v20 detection and cooldown

- Files:
  - `rust/src/host/cookies/v20_guard.rs` (new)
- Acceptance:
  - Detect `b"v20"` prefix.
  - When every target cookie for the relevant domain is v20, abort with `BrowserCookieError::accessDeniedHint("App-Bound Encryption (v20) — requires manual cookie paste")`.
  - Open a 6 h cooldown for that browser, persisted to `%LOCALAPPDATA%\CodexBar\state\browser_cooldowns.json`.
  - Emit a one-time toast event: `Chrome 127+ requires a manual cookie paste for this provider. Open settings > paste cookie header > done.`
- Commit:
  ```
  feat(host): detect Chromium v20 cookies and cool down cleanly

  v20 (App-Bound Encryption) is intentionally hostile to non-Chrome
  processes. We do not attempt unwrap. Instead we emit the documented
  error string, open a 6h cooldown, and toast the user once with a
  pointer to the manual cookie paste field.
  ```

#### Task 3.4: Firefox cookie reader

- Files:
  - `rust/src/host/cookies/firefox.rs` (new)
- Acceptance:
  - Profile root: `%APPDATA%\Mozilla\Firefox\Profiles\`.
  - Picks `*.default*` directories.
  - Copies `cookies.sqlite` plus any `-wal` and `-shm` siblings to a temp dir before opening read-only.
  - Schema: `moz_cookies` with `host, name, path, value, expiry, isSecure, isHttpOnly, sameSite, originAttributes`.
  - `expiry` is seconds since epoch.
- Commit:
  ```
  feat(host): read Firefox cookies from the Windows profile path

  Copies cookies.sqlite to a temp dir to avoid Firefox's exclusive lock,
  then queries moz_cookies. Plain SQLite, no decryption needed.
  ```

#### Task 3.5: Per-account cookie jar persistence

- Files:
  - `rust/src/providers/codex/web/account_jars.rs` (new)
- Acceptance:
  - One `reqwest::cookie::Jar` per account.
  - Identifier: SHA-256 of normalized lowercased email, truncated to 16 bytes, masked into a v4 UUID.
  - Serialized to `%LOCALAPPDATA%\CodexBar\openai-dashboard-jars\<email-uuid>.json`.
  - Round-trip test: write a jar, restart, read back; assert cookies preserved.
- Commit:
  ```
  feat(codex): isolate OpenAI web cookies per account on disk

  Each account gets its own reqwest::cookie::Jar keyed by a UUID derived
  from the SHA-256 of the lowercased email. Jars serialize to JSON under
  %LOCALAPPDATA%\CodexBar\openai-dashboard-jars and restore on boot.
  ```

#### Task 3.6: Cookie cache via Credential Manager

- Files:
  - `rust/src/providers/codex/web/cookie_cache.rs` (new)
- Acceptance:
  - Uses `keyring` crate with service `CodexBar.cache` and target `cookie.codex` (or `cookie.codex.account-<uuid>` for scoped variants).
  - Payload: `{ source_label, stored_at, cookie_header }`.
  - On `manualCookieHeaderInvalid`, `noMatchingAccount`, or `dashboardStillRequiresLogin`: clear the cache entry and retry browser import.
- Commit:
  ```
  feat(codex): cache validated cookie headers in Credential Manager

  Stores the working Cookie header per account under CodexBar.cache so
  the next refresh skips browser scanning. Invalidated on the three
  documented error paths and a fresh import is retried.
  ```

#### Task 3.7: Scrape script payload

- Files:
  - `rust/src/providers/codex/web/scrape_script.rs` (new, ships the JS as a string constant)
  - `rust/src/providers/codex/web/scrape_result.rs` (new, the deserialized dict shape)
- Acceptance:
  - JS payload is ported verbatim from the macOS `openAIDashboardScrapeScript`.
  - Result dict keys: `loginRequired`, `workspacePicker`, `cloudflareInterstitial`, `href`, `bodyText`, `bodyHTML`, `signedInEmail`, `creditsPurchaseURL`, `rows[]`, `usageBreakdownJSON`, `usageBreakdownDebug`, `usageBreakdownError`, `scrollY`, `scrollHeight`, `viewportHeight`, `creditsHeaderPresent`, `creditsHeaderInViewport`, `didScrollToCredits`.
  - Deserialization test against a fixture captured from real DOM.
- Commit:
  ```
  feat(codex): embed the OpenAI dashboard scrape script verbatim

  Ships the macOS scrape JS as a Rust string constant. Result dict
  deserializes into a strongly-typed ScrapeResult struct. Fixture-driven
  decode test guards every key.
  ```

#### Task 3.8: Locale-aware number parser

- Files:
  - `rust/src/providers/codex/web/text_parsing.rs` (new)
- Acceptance:
  - Handles US `1,234.56`, EU `1.234,56` (heuristic: presence of "crédit" triggers comma-decimal), thin spaces `U+202F` and `U+00A0`.
  - Regex parsers per spec 41 §5.6 table.
  - Unit tests with locale fixtures (US, FR, DE).
- Commit:
  ```
  feat(codex): parse localized credit numbers and reset times

  Shared text_parsing module handles US, EU, and French credit-number
  formats plus unicode thin-space groupings. Reset-time parser handles
  "today", "tomorrow", weekday names, and absolute dates per spec 41 5.6.
  ```

#### Task 3.9: Headless reqwest preflight then WebView2 fallback

- Files:
  - `rust/src/providers/codex/web/api_preflight.rs` (new)
  - `rust/src/providers/codex/web/webview2_fallback.rs` (new)
- Acceptance:
  - Preflight: `GET https://chatgpt.com/backend-api/me` with 10 s timeout, BFS-scan JSON for first `email` key containing `@`.
  - On preflight failure or hydration-only data needed: launch off-screen Tauri WebView2 window pointed at the analytics URL, eval the scrape script, return the result.
  - WebView2 user-data folder: `%LOCALAPPDATA%\CodexBar\webview2\<email-uuid>\`.
  - Failure modes: `loginRequired`, `cloudflareInterstitial`, `noDashboardData(body sample)`.
- Commit:
  ```
  feat(codex): preflight via reqwest then fall back to a WebView2 scrape

  Most of the dashboard data is plain JSON or DOM text. Try reqwest first
  (no WebView spin-up cost). If hydration-only fields (code_review,
  usage_breakdown, credit_events) are still missing, open an off-screen
  WebView2 with the per-account user-data folder and eval the scrape
  payload. Failure modes mapped to the documented error variants.
  ```

#### Task 3.10: Usage breakdown filter

- Files:
  - `rust/src/providers/codex/web/usage_breakdown.rs` (new)
- Acceptance:
  - `removing_skill_usage_services` strips any service whose name starts with `skillusage:` (lowercased, trimmed).
  - Round-trip test: input with one `skillusage:foo` and one real service; output has only the real service.
- Commit:
  ```
  feat(codex): filter skillusage services from the usage breakdown

  Internal markers prefixed skillusage: are stripped before the chart
  renders. Test asserts the real service survives and the marker does not.
  ```

#### Task 3.11: Diagnostics to UI mapping

- Files:
  - `rust/src/providers/codex/web/diagnostics.rs` (new)
- Acceptance:
  - Maps each error variant to the verbatim string from spec 41 §5.9:
    - `loginRequired` -> `OpenAI web access requires login.`
    - `cloudflareInterstitial` -> `OpenAI web refresh hit a Cloudflare challenge.`
    - `noMatchingAccount(found)` -> `OpenAI web session does not match Codex account. Found: <list>.`
    - `dashboardStillRequiresLogin` -> `Browser cookies imported, but dashboard still requires login.`
    - `manualCookieHeaderInvalid` -> `Manual cookie header is missing a valid OpenAI session cookie.`
    - `browserAccessDenied(details)` -> `Browser cookie access denied. <hint>`
    - `noDashboardData(body)` -> `OpenAI dashboard data not found. Body sample: ...`
  - Snapshot tests pin each string byte-for-byte.
- Commit:
  ```
  feat(codex): map web diagnostics to verbatim user-facing strings

  CodexUIErrorMapper-equivalent table for the OpenAI web path. Snapshot
  tests pin every string from spec 41 5.9 byte for byte so future edits
  surface as test diffs.
  ```

#### Task 3.12: Buy Credits WebView2 window

- Files:
  - `rust/src/providers/codex/web/buy_credits_window.rs` (new)
  - `apps/desktop-tauri/src/providers/codex/BuyCreditsButton.tsx` (new)
- Acceptance:
  - Resolves URL: prefer `dashboardSnapshot.creditsPurchaseURL`, fall back to `https://chatgpt.com/codex/cloud/settings/usage`.
  - Window dimensions: 980x760, capped at 92% width and 88% height of the visible screen.
  - Title: `Buy Credits`.
  - User-data folder: the same per-account folder used for scraping so the session is already logged in.
  - Auto-start JS (when requested) walks the DOM (including shadow roots and same-origin iframes) for buttons matching `(credit AND (buy|add|purchase|top up))` or "Add more"; polls every 500 ms (max 90 attempts) for the dialog Next button.
  - Logs forwarded to `%TEMP%\codexbar-buy-credits.log`.
- Commit:
  ```
  feat(codex): open a WebView2 window for Buy Credits

  Sized 980x760 (capped to 92% by 88% of the screen), shares the per-account
  user-data folder so the session is already authenticated. Optional auto-
  start JS walks the DOM and advances through the purchase dialog. All
  console logs forwarded to %TEMP%\codexbar-buy-credits.log.
  ```

### Group 4: Account promotion flow

#### Task 4.1: Vocabulary types and shared structs

- Files:
  - `rust/src/providers/codex/promotion/mod.rs` (new)
  - `rust/src/providers/codex/promotion/types.rs` (new)
- Acceptance:
  - Types defined per spec 41 §6.1 and §6.4: `CodexActiveSource`, `ManagedCodexAccount`, `PreparedStoredManagedAccount`, `PreparedLiveAccount`, `PreparedAuthMaterial`, `PreparedIdentity`, `PreparedPromotionContext`.
  - `homeState` enums: `Readable(AuthMaterial) | Missing(HomeUrl) | Unreadable(HomeUrl)` for stored; `Missing | Unreadable | ApiKeyOnly(AuthMaterial) | Readable(AuthMaterial)` for live.
  - `CodexIdentityMatcher::matches(a, b) -> bool` compares by provider id first, email second.
- Commit:
  ```
  feat(codex): scaffold promotion vocabulary and prepared-context types

  Lays out the type system shared by Preparation, Planning, and Execution.
  Identity matcher compares by provider id first and falls back to email
  match, matching the macOS reference exactly.
  ```

#### Task 4.2: Preparation (read everything from disk)

- Files:
  - `rust/src/providers/codex/promotion/preparation.rs` (new)
- Acceptance:
  - Reads the target managed account home, the live `auth.json`, every other managed home, every JWT identity, and the workspace catalog.
  - Builds a `PreparedPromotionContext`.
  - Filesystem I/O is **only** in this module. Planning and Execution must read inputs from the prepared context, never from disk.
  - Errors mapped to `PromotionError::targetManagedAccountAuthMissing` or `targetManagedAccountAuthUnreadable` per the failure mode.
- Commit:
  ```
  feat(codex): build PreparedPromotionContext from disk inputs

  All filesystem reads required by promotion happen here. Planning and
  Execution downstream are pure functions over the prepared context.
  Failure modes mapped to PromotionError variants.
  ```

#### Task 4.3: Converged no-op detection

- Files:
  - `rust/src/providers/codex/promotion/converged.rs` (new)
- Acceptance:
  - Logic per spec 41 §6.5:
    1. If `live.authIdentity` exists, compare to `target.authIdentity ?? target.persistedIdentity` via `CodexIdentityMatcher::matches`. On match: pick `liveSystem` when email is present, else `managedAccount(target.id)`.
    2. If `live.authIdentity` is absent but `snapshot.liveSystemAccount` exists and matches target: `liveSystem`.
    3. Otherwise: not converged.
  - On converged: write the resolved `CodexActiveSource`, kick a scoped refresh, return `convergedNoOp` with `didMutateLiveAuth = false`.
- Commit:
  ```
  feat(codex): detect converged no-op before mutating live auth

  When the target managed account already matches the live system account,
  promotion is a settings-only update. No bytes are written to disk and
  the result is convergedNoOp with didMutateLiveAuth = false.
  ```

#### Task 4.4: Planning (pure decision function)

- Files:
  - `rust/src/providers/codex/promotion/planning.rs` (new)
  - `rust/src/providers/codex/promotion/tests/planner_matrix.rs` (new)
- Acceptance:
  - Pure function: no filesystem side effects, no clock, no RNG.
  - Returns one of `Plan::None(reason) | Plan::Reject(reason) | Plan::ImportNew(reason) | Plan::RefreshExisting(destination, reason) | Plan::RepairExisting(destination, reason)`.
  - Live-state branch table per spec 41 §6.6:
    - `missing` -> `none(liveMissing)`
    - `unreadable` -> `reject(liveUnreadable)`
    - `apiKeyOnly` -> `reject(liveAPIKeyOnlyUnsupported)`
    - `readable` without identity -> `reject(liveIdentityMissingForPreservation)`
    - `readable` matching target -> `none(targetMatchesLiveAuthIdentity)`
    - `readable` distinct -> search managed destinations in this priority order:
      1. `refreshExisting` (`readableHomeIdentityMatch` or `readableHomeIdentityMatchUsingPersistedEmailFallback`).
      2. `reject(conflictingReadableManagedHome)`.
      3. `repairExisting` (`persistedProviderMatchWithMissingHome`, `persistedProviderMatchWithUnreadableHome`, or `persistedLegacyEmailMatch`).
      4. `importNew(noExistingManagedDestination)`.
  - Decision-matrix unit tests exercise every cell.
- Commit:
  ```
  feat(codex): implement pure planner for promotion disposition

  No filesystem side effects: every input comes from the PreparedPromotion
  Context. Returns one of none, reject, importNew, refreshExisting, or
  repairExisting. Decision-matrix tests cover every branch per spec 41 6.6.
  ```

#### Task 4.5: Execution (safety contract enforced)

- Files:
  - `rust/src/providers/codex/promotion/execution.rs` (new)
  - `rust/src/providers/codex/promotion/home_factory.rs` (new)
- Acceptance:
  - **Safety contract**: the executor never swaps live auth. That happens only in the service after both planning and execution succeed.
  - `importNew`: create `<root>/<UUID>/`, mkdir -p, write displaced-live `auth.json` atomically with owner-only ACL, append to catalog, re-read store to resolve disposition.
  - `refreshExisting` and `repairExisting`: validate destination is inside the managed-homes root via `home_factory::validate_managed_home_for_deletion`, mkdir -p (idempotent), write displaced-live `auth.json`, atomic-replace in catalog (preserve `id` and `createdAt`).
  - On `importNew` failure: best-effort cleanup of the freshly created home, **only if inside the sandbox**. Outside the sandbox: refuse to delete; surface error.
- Commit:
  ```
  feat(codex): execute promotion plans without swapping live auth

  Executor handles importNew, refreshExisting, and repairExisting. Live
  auth.json is never touched here. Sandbox validation gates every write
  and delete against the managed-homes root.
  ```

#### Task 4.6: Service (orchestrator) and final live swap

- Files:
  - `rust/src/providers/codex/promotion/service.rs` (new)
  - `rust/src/providers/codex/promotion/live_swap.rs` (new)
- Acceptance:
  - Service flow: build prepared context, check converged no-op, plan, execute, then final live swap, then write `codexActiveSource = liveSystem`, then `refresh_scoped(true)`.
  - Live swap: write `auth.json.codexbar-staged-<UUID>` with owner-only ACL, then atomic `tokio::fs::rename`. On cross-volume failure: `fs::copy` plus explicit `sync_all` plus `fs::remove_file`.
  - Errors mapped via `liveAuthSwapFailed`.
- Commit:
  ```
  feat(codex): orchestrate promotion and atomically swap live auth

  Service composes Preparation, Planning, and Execution. Final step
  writes a staged auth.json with owner-only ACL and renames atomically.
  Cross-volume fallback uses copy plus sync_all plus remove.
  ```

#### Task 4.7: Coordinator and interaction guards

- Files:
  - `rust/src/providers/codex/promotion/coordinator.rs` (new)
- Acceptance:
  - State flags: `isAuthenticatingLiveAccount`, `isPromotingSystemAccount`, `userFacingError`.
  - Interaction guards per spec 41 §6.11: refuse when any flag is already set, or when `hasConflictingManagedAccountOperationInFlight` is true.
  - On block: surface `Finish the current managed account change before switching the system account.`
- Commit:
  ```
  feat(codex): coordinate promotion state and block concurrent changes

  Coordinator owns the spinner flags and the user-facing error. Refuses
  to start a promotion while another is in flight, while a managed-account
  add or remove is in flight, or while live-account authentication is in
  flight. Surfaces the exact spec-mandated block message.
  ```

#### Task 4.8: Verbatim error string table

- Files:
  - `rust/src/providers/codex/promotion/errors.rs` (new)
  - `rust/src/providers/codex/promotion/tests/error_strings.rs` (new)
- Acceptance:
  - Every `CodexAccountPromotionError` variant maps to exactly one string from spec 41 §6.10. The Windows variant of `liveAccountUnreadable` uses `on this PC` (instead of macOS `on this Mac`).
  - Snapshot test pins every mapping. The error strings are:
    - `targetManagedAccountNotFound` -> `That account is no longer available in CodexBar. Refresh the account list and try again.`
    - `targetManagedAccountAuthMissing` -> `CodexBar could not find saved auth for that account. Re-authenticate it and try again.`
    - `targetManagedAccountAuthUnreadable` -> `CodexBar could not read saved auth for that account. Re-authenticate it and try again.`
    - `liveAccountUnreadable` -> `CodexBar could not read the current system account on this PC.`
    - `liveAccountMissingIdentityForPreservation` -> `CodexBar could not safely preserve the current system account before switching.`
    - `liveAccountAPIKeyOnlyUnsupported` -> `CodexBar can't replace a system account that is signed in with an API key only setup.`
    - `displacedLiveManagedAccountConflict` -> `CodexBar found another managed account that already uses the current system account. Resolve duplicate first.`
    - `displacedLiveImportFailed` -> `CodexBar could not save the current system account before switching.`
    - `managedStoreCommitFailed` -> `CodexBar could not update managed account storage.`
    - `liveAuthSwapFailed` -> `CodexBar could not replace the live Codex auth on this PC.`
    - Interaction blocked -> `Finish the current managed account change before switching the system account.`
  - All grouped under the alert title `Could not switch system account`.
- Commit:
  ```
  feat(codex): pin promotion error strings to spec 41 verbatim

  Every CodexAccountPromotionError variant maps to a fixed user-facing
  string. Snapshot test asserts each mapping byte for byte. macOS "on
  this Mac" wording adapted to "on this PC" on Windows.
  ```

#### Task 4.9: State machine documentation

- Files:
  - `rust/src/providers/codex/promotion/STATE_MACHINE.md` (new)
- Acceptance:
  - ASCII diagram baked into the source tree per spec 41 §6.9.
  - Includes every transition: click -> coordinator -> prepared context -> converged check -> plan -> execute -> live swap -> active source write -> refresh -> toast.
- Commit:
  ```
  docs(codex): describe the promotion state machine in source

  ASCII diagram captures every transition from menu click to toast. Lives
  in source alongside the promotion modules so reviewers see it inline.
  ```

### Group 5: Managed accounts

#### Task 5.1: Catalog schema v2 and dual-version reader

- Files:
  - `rust/src/providers/codex/managed/catalog.rs` (new)
  - `rust/src/providers/codex/managed/migration.rs` (new)
- Acceptance:
  - v2 fields: `id, email, providerAccountID, workspaceLabel, workspaceAccountID, managedHomePath, createdAt, updatedAt, lastAuthenticatedAt`.
  - v1 to v2 migration: hydrate `providerAccountID` from each row's local JWT once. Idempotent (running twice yields the same bytes).
  - Sanitization: unique by `id`, by `(email, providerAccountID)` when id known, by `email` for legacy.
  - File path: `%LOCALAPPDATA%\CodexBar\managed-codex-accounts.json`, owner-only ACL.
- Commit:
  ```
  feat(codex): migrate managed-account catalog from v1 to v2

  Reads v1 rows, hydrates providerAccountID from the local auth.json JWT,
  writes a v2 file under %LOCALAPPDATA%. Idempotent: a second run yields
  byte-identical output. Owner-only ACL applied on write.
  ```

#### Task 5.2: Managed home factory and sandbox validation

- Files:
  - `rust/src/providers/codex/managed/home_factory.rs` (new)
- Acceptance:
  - `make_home_url() -> PathBuf` allocates `%LOCALAPPDATA%\CodexBar\managed-codex-homes\<UUID>\`.
  - `validate_managed_home_for_deletion(path)` returns `Ok(())` only if the canonical path starts with the managed-homes root.
  - Symlink resolution: canonicalize before comparison.
  - Refuses to delete `%USERPROFILE%`, `~/.codex`, or any path containing a `..`.
  - Tests pin each refusal case.
- Commit:
  ```
  feat(codex): enforce sandbox for every managed-home write or delete

  All file mutations under managed-codex-homes go through home_factory.
  Canonicalize-then-compare ensures symlinks cannot escape the sandbox.
  Tests pin refusal cases for user home, .codex, and parent traversal.
  ```

#### Task 5.3: Managed account add or remove service

- Files:
  - `rust/src/providers/codex/managed/service.rs` (new)
  - `rust/src/providers/codex/managed/login_runner.rs` (new)
- Acceptance:
  - `authenticate_managed_account(existing_id, timeout_ms)`: allocate a new managed home, spawn `codex.exe login` with `CODEX_HOME=<home>`, capture the resulting `auth.json`, optionally show workspace picker, persist.
  - Subprocess uses JobObject (group kill on timeout).
  - Default timeout: 120 s.
  - `remove_managed_account(id)`: validate via `home_factory::validate_managed_home_for_deletion`, remove catalog row, delete home dir.
- Commit:
  ```
  feat(codex): add and remove managed accounts via codex login

  Spawns codex.exe login with CODEX_HOME pointed at a fresh managed home.
  120s default timeout enforced via the JobObject so a hung login does
  not leak processes. Removal sandbox-validates the path before deletion.
  ```

#### Task 5.4: Workspace picker

- Files:
  - `rust/src/providers/codex/managed/workspace_resolver.rs` (new)
  - `apps/desktop-tauri/src/providers/codex/WorkspacePickerDialog.tsx` (new)
- Acceptance:
  - Calls OpenAI workspaces API with the freshly logged-in OAuth token.
  - If `> 1` workspace: open a small Tauri WebView dialog with a `<select>`, an "Add Workspace" button, and a "Cancel" button.
  - On confirm: write `tokens.account_id = <workspaceID>` back into the managed home's `auth.json`.
  - On cancel: return `workspaceSelectionCancelled` and cleanup the partially created managed home.
- Commit:
  ```
  feat(codex): present a workspace picker after managed login

  Lists workspaces from the OpenAI API, persists the chosen account id
  into the managed home's auth.json, and cleans up partial homes on
  cancel. Single-workspace logins skip the picker.
  ```

### Group 6: History ownership

#### Task 6.1: Canonical key scheme

- Files:
  - `rust/src/providers/codex/history/keys.rs` (new)
- Acceptance:
  - `ProviderAccount(id)` -> `codex:v1:provider-account:<normalized-id>`.
  - `EmailOnly(email)` -> `codex:v1:email-hash:<sha256(normalized_email)>`.
  - `Unresolved` -> no key (history does not load).
  - Legacy classification: `Canonical(key)`, `LegacyEmailHash(hash)`, `LegacyOpaqueScoped(key)`, `LegacyUnscoped`.
- Commit:
  ```
  feat(codex): define canonical history keys plus legacy classification

  Maps every CodexIdentity to a v1 storage key. Legacy keys classified
  into four buckets so the continuity matcher can decide whether to
  honour them under the new identity.
  ```

#### Task 6.2: belongs_to_target_continuity

- Files:
  - `rust/src/providers/codex/history/ownership.rs` (new)
- Acceptance:
  - Function decides whether a stored key belongs to the current target.
  - Legacy email hash counts only when the target's canonical email-hash matches (a "same human, upgraded identity" case).
  - `hasAdjacentMultiAccountVeto = true` forces strict-only: legacy and loose continuity matches are rejected.
- Commit:
  ```
  feat(codex): gate history continuity by canonical key match

  belongs_to_target_continuity accepts legacy email-hash keys only when
  the target has a matching canonical key. The adjacent multi-account
  veto forces strict-only matching to prevent cross-account leakage.
  ```

#### Task 6.3: Ownership context

- Files:
  - `rust/src/providers/codex/history/context.rs` (new)
- Acceptance:
  - Aggregates runtime identity (from the reconciliation snapshot), `hasAdjacentMultiAccountVeto`, and `currentWeeklyResetAt`.
  - Veto set when active managed account and live system account resolve to different identities.
- Commit:
  ```
  feat(codex): expose CodexOwnershipContext to gate history reads

  Aggregates runtime identity, veto flag, and weekly reset. Consumers
  (charts, history menu) read continuity decisions through this context
  rather than rolling their own.
  ```

### Group 7: UI

#### Task 7.1: Provider card

- Files:
  - `apps/desktop-tauri/src/providers/codex/CodexProviderCard.tsx` (new)
  - `apps/desktop-tauri/src/providers/codex/index.tsx` (new, registers the override)
- Acceptance:
  - Renders bars for session (5h), weekly, and tertiary windows when present.
  - Credits balance row.
  - Monthly spend row.
  - "Buy Credits" CTA, disabled until a `creditsPurchaseURL` resolves.
  - Code-review remaining row when `dashboardExtras.codeReviewRemainingPercent` is present.
  - Usage breakdown disclosure (collapsed by default).
  - Bars dim to 60% opacity with a "stale" badge when last refresh failed but a previous snapshot exists.
- Commit:
  ```
  feat(codex): render the Codex provider card

  Session, weekly, and tertiary bars; credits row; monthly spend row;
  Buy Credits CTA; conditional code-review row; usage breakdown disclosure.
  Dim-state behaviour matches spec 41 12.
  ```

#### Task 7.2: Credits-history chart

- Files:
  - `apps/desktop-tauri/src/providers/codex/CreditsHistoryChart.tsx` (new)
- Acceptance:
  - Line chart from `credit_events[]` (date, creditsUsed).
  - Recharts under the hood.
  - Tooltips localized for the user's locale.
- Commit:
  ```
  feat(codex): chart credits history from scraped events

  Recharts line chart fed by credit_events from the dashboard snapshot.
  Locale-aware tooltips. Falls back to an empty-state message when no
  events have been scraped yet.
  ```

#### Task 7.3: Settings pane

- Files:
  - `apps/desktop-tauri/src/providers/codex/CodexSettingsPane.tsx` (new)
- Acceptance:
  - Codex usage source picker (`auto`, `oauth`, `cli`).
  - OpenAI dashboard extras toggle (binds to `openAIWebAccessEnabled`).
  - Cookie source tri-state (`off`, `auto`, `manual`).
  - Manual cookie textarea, gated on `cookieSource = manual`.
  - Managed-account list editor: add, remove, promote.
  - Each control wires to `invoke("codex_*")` Tauri commands per spec 41 §15.4.
- Commit:
  ```
  feat(codex): build the Codex settings pane

  Usage source picker, dashboard extras toggle, cookie source tri-state,
  manual cookie textarea, and managed-account list editor with add,
  remove, and promote actions. Wires to every codex_* Tauri command.
  ```

#### Task 7.4: Multi-account UI (switcher bar or stacked cards)

- Files:
  - `apps/desktop-tauri/src/providers/codex/AccountSwitcher.tsx` (new)
  - `apps/desktop-tauri/src/providers/codex/StackedAccountCards.tsx` (new)
- Acceptance:
  - Toggle in Preferences > Advanced > Display selects switcher bar OR stacked cards.
  - Hard cap at 6 visible accounts.
  - The live system account always appears with a checkmark on the resolved row.
  - Clicking a row sets `codexActiveSource` (no file mutation).
  - Right-clicking a row opens a context menu with "Promote to System Account" (triggers the promotion flow).
- Commit:
  ```
  feat(codex): show multi-account switcher or stacked cards

  Capped at 6 accounts. Switcher mode renders a horizontal pill bar;
  stacked mode renders one card per account. Click sets the active
  source; right-click opens promotion. Selection is configurable in
  Preferences > Advanced > Display.
  ```

#### Task 7.5: System Account submenu

- Files:
  - `apps/desktop-tauri/src/providers/codex/SystemAccountSubmenu.tsx` (new)
- Acceptance:
  - Menu entry: **Codex > System Account > checkmark on the resolved live row, other managed rows listed beneath**.
  - Clicking a non-live row triggers the promotion flow per spec 41 §6.2.
  - Spinner appears on the clicked row; other rows are disabled while the flow runs.
  - Empty state: when no managed accounts exist, the submenu shows a single "Add account..." row that opens the login flow.
- Commit:
  ```
  feat(codex): wire the System Account submenu to promotion

  Submenu lists live plus managed accounts with a checkmark on the
  resolved live. Click on another row spawns the promotion. Spinner on
  the clicked row; other rows disabled until the result lands. Empty
  state offers "Add account...".
  ```

### Group 8: Tests

#### Task 8.1: JWT fixture suite

- Files:
  - `rust/src/providers/codex/auth/tests/jwt_fixtures/*.json` (six fixtures)
  - `rust/src/providers/codex/auth/tests/jwt_parsing.rs` (new)
- Acceptance:
  - One fixture per cell of the three-fallback ladder for email, plan, and account id.
  - Six tests assert the extracted identity matches the expected value.
  - Malformed-payload fixture asserts `CodexIdentity::Unresolved`.
- Commit:
  ```
  test(codex): cover JWT identity ladder with six fixtures

  One fixture per fallback cell (canonical, mid-fallback, deep-fallback)
  across email, plan, and account id. A seventh malformed-payload fixture
  asserts graceful Unresolved fallback.
  ```

#### Task 8.2: Planner decision-matrix unit tests

- Files:
  - `rust/src/providers/codex/promotion/tests/cases/*.yaml` (one case per matrix cell)
  - `rust/src/providers/codex/promotion/tests/planner_replay.rs` (new)
- Acceptance:
  - YAML cases mirror the macOS `CodexPromotion/Cases/*.yaml` fixtures.
  - Each case maps a `PreparedPromotionContext` shape to the expected `Plan` variant.
  - Test runner replays every case and asserts the planner output.
  - Every cell of the matrix from spec 41 §6.6 is exercised (live missing, live unreadable, live api-key-only, live readable without identity, live readable matching target, refreshExisting via providerId, refreshExisting via persistedEmail fallback, reject conflicting, repair persisted-provider-with-missing-home, repair persisted-provider-with-unreadable-home, repair legacy-email-match, importNew no-existing-destination).
- Commit:
  ```
  test(codex): replay every promotion planner cell from yaml fixtures

  Twelve YAML cases cover the full decision matrix from spec 41 6.6. Each
  case shapes a PreparedPromotionContext and asserts the resulting Plan
  variant plus reason. Mirrors the macOS test suite.
  ```

#### Task 8.3: Promotion executor safety test

- Files:
  - `rust/src/providers/codex/promotion/tests/executor_safety.rs` (new)
- Acceptance:
  - Simulates failure paths: catalog write fails, mkdir fails, atomic-write fails, sandbox validation fails on a crafted symlink.
  - In every case, asserts the live `auth.json` byte stream is byte-identical before and after.
  - Asserts the executor never invokes `live_swapper.swap`.
- Commit:
  ```
  test(codex): assert promotion executor never mutates live auth

  Five failure paths exercised: catalog write, mkdir, atomic write, symlink
  sandbox escape, and best-effort cleanup. Each path asserts the live
  auth.json is byte-identical and the live swapper is never called.
  ```

#### Task 8.4: Managed catalog migration test

- Files:
  - `rust/src/providers/codex/managed/tests/migration.rs` (new)
  - `rust/src/providers/codex/managed/tests/fixtures/v1.json` (new)
  - `rust/src/providers/codex/managed/tests/fixtures/v2_expected.json` (new)
- Acceptance:
  - Reads `v1.json`, runs migration, asserts equals `v2_expected.json`.
  - Runs migration twice on the v2 output, asserts byte-identical (idempotent).
  - Hydration covers: row with embedded JWT, row missing JWT (hydration skipped, no panic), row with malformed JWT (logged, skipped).
- Commit:
  ```
  test(codex): pin v1 to v2 managed-catalog migration as idempotent

  v1 fixture migrates to the v2 expected bytes; a second migration over
  v2 is a no-op. Coverage for missing-JWT and malformed-JWT rows asserts
  graceful skip without losing other rows.
  ```

#### Task 8.5: should_fallback truth table

- Files:
  - `rust/src/providers/codex/oauth/tests/fallback_truth_table.rs` (extended)
- Acceptance:
  - Exercise every variant of `CodexOAuthError` across every value of `SourceMode`.
  - Pin expected outputs in a table; loop with `for (err, mode, expected) in ...`.
- Commit:
  ```
  test(codex): exhaust the should_fallback truth table

  Every CodexOAuthError variant times every SourceMode value. Expected
  outputs pinned in a table-driven test so future edits surface as diffs.
  ```

#### Task 8.6: End-to-end live-data smoke

- Files:
  - `rust/tests/codex_e2e_live.rs` (new, gated by `CODEXBAR_E2E_LIVE=1`)
- Acceptance:
  - Skipped by default; requires a real `~/.codex/auth.json`.
  - Runs the OAuth strategy end to end, asserts a non-empty `UsageSnapshot`.
  - Documents the env variable in the test file's leading comment.
- Commit:
  ```
  test(codex): add a gated end-to-end smoke against real auth

  Skipped unless CODEXBAR_E2E_LIVE=1. Runs the OAuth strategy against the
  user's real ~/.codex/auth.json and asserts a snapshot comes back. Used
  by the developer to verify before each release; not run in CI.
  ```

---

## Phase acceptance tests

The phase is "done" when every one of these holds:

1. **OAuth path**: a fresh checkout, with a valid `~/.codex/auth.json`, produces a live Codex card on the tray popup within 2 s of launch.
2. **CLI path**: with `usageSource = cli`, the same card populates from JSON-RPC over stdio (verifiable by deleting `~/.codex/auth.json` after the first refresh and restarting).
3. **Web extras path**: with `openAIWebAccessEnabled = true` and Chrome cookies present (v10), the card gains the code-review row and the usage breakdown disclosure.
4. **Manual cookies**: pasting a `Cookie:` header from DevTools into the settings textarea, with `cookieSource = manual`, populates the same web-extras fields.
5. **Buy Credits**: clicking the CTA opens a 980x760 WebView2 window already logged into ChatGPT under the active account.
6. **Multi-account end to end**: create two managed accounts, promote one, observe the System Account submenu's checkmark move; restart; observe the new state persisted.
7. **Promotion safety**: a forced failure (e.g. fill the managed-homes volume to provoke a mkdir failure) leaves `~/.codex/auth.json` byte-identical.
8. **Promotion decision-matrix**: `cargo test -p codexbar-core planner_replay` exercises every YAML case green.
9. **Migration**: launching against a v1 catalog produces a v2 catalog on disk; relaunching does not modify it.
10. **JWT ladder**: every fixture in `tests/jwt_fixtures/` resolves to the expected identity.
11. **Error strings**: snapshot tests for `promotion::errors` and `web::diagnostics` pass with byte-identical strings.
12. **Sandbox**: `validate_managed_home_for_deletion` rejects `%USERPROFILE%`, `~/.codex`, and a crafted symlink target outside the sandbox.

A passing run of all twelve, plus the spec 41 §14 macOS-parity checklist (using fixtures), is the formal sign-off.

---

## CI gates

The following must pass on every PR touching Phase 5 code, and must be wired into `.github/workflows/ci.yml` as part of Task 1.1 if not already present:

- `cargo fmt -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo test -p codexbar-core --features e2e-mock` (replays HTTP fixtures via `httpmock`; does not hit the network)
- `pnpm -C apps/desktop-tauri lint`
- `pnpm -C apps/desktop-tauri typecheck`
- `pnpm -C apps/desktop-tauri test`
- A non-CI gate: the `CODEXBAR_E2E_LIVE=1` smoke must be run locally by the engineer landing Group 1 commits (record the run in the PR description).

---

## Risks

### Web scraper brittleness

OpenAI changes the dashboard DOM without notice. The scrape script is the single biggest source of "it stopped working on Tuesday" reports. Mitigation:

- Ship the JS as a single string constant so a hot-patch is a one-file edit plus a release.
- Cache the last successful scrape per account so the card does not go blank the moment the DOM shifts.
- Make the regex parsers tolerant to whitespace and unicode variants; never anchor on a single class name.
- Add a `--dump-scrape` debug command that emits the raw `ScrapeResult` to a file for users to attach to bug reports.

### v20 cookies for chatgpt.com

Chrome 127+ wraps cookies with App-Bound Encryption. Today most ChatGPT users on Chrome will hit this. We do not unwrap v20 on policy grounds (see spec 60 §1.3). Mitigation:

- The `manual cookie paste` path is first-class: the textarea is exactly two clicks from the menubar.
- Onboarding shows the v20 banner the first time we detect a v20-only browser cookie set, with a one-click "Open settings" link.
- Firefox cookies still work (no v20 there) so a user can always switch browsers if motivated.

### JWT field drift

`chatgpt_account_id` once lived under `payload["https://api.openai.com/auth"]` and is migrating into the top-level `payload`. The three-fallback ladder for each field is **the** safeguard. Mitigation:

- Fixture-driven tests for every ladder cell.
- A `unknown_payload_fields_seen` telemetry counter (opt-in) flags new field locations early.

### Live `auth.json` corruption

If we ever crash mid-write to `~/.codex/auth.json`, the Codex CLI itself breaks. Mitigation:

- Every write is staged plus renamed. The staged file's owner-only ACL is set before the rename.
- `live_auth_swap` writes to a uniquely named staged file (`auth.json.codexbar-staged-<UUID>`) so concurrent CodexBar instances cannot race.
- A pre-promotion "dry run" reads the live file and asserts it parses cleanly before the swap.

### Promotion data loss

The flow is intricate. Mitigation:

- Executor never swaps live auth (safety contract).
- Service swaps live auth only after Planning and Execution succeed.
- Every executor write is sandbox-validated.
- The displaced-live disposition is computed by a pure function with a YAML-driven test suite.

### CLI subprocess orphans

`codex app-server` is a long-lived child. If CodexBar crashes, the child must die. Mitigation:

- Win32 `JobObject` with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- Test: kill the parent process forcibly, assert the child dies within 2 s.

### Per-account jar drift

A user logs into Account A, then Account B, then Account A again. Cookies must not bleed. Mitigation:

- Per-account UUID derived deterministically from the lowercased email.
- Persisted jars never share a file.
- The active source picker scopes every HTTP call by jar at the call site, not at the jar level.

---

## Time estimate

Order of magnitude, assuming one engineer working full time and no other phases blocking:

| Group                                | Days   |
| ------------------------------------ | -----: |
| 1. OAuth path (6 tasks)              | 4 to 5 |
| 2. CLI integration (5 tasks)         | 3 to 4 |
| 3. Web dashboard extras (12 tasks)   | 7 to 9 |
| 4. Account promotion (9 tasks)       | 5 to 7 |
| 5. Managed accounts (4 tasks)        | 3 to 4 |
| 6. History ownership (3 tasks)       | 1 to 2 |
| 7. UI (5 tasks)                      | 4 to 5 |
| 8. Tests (6 tasks)                   | 3 to 4 |
| Polish, bug fixes, doc updates       | 3 to 5 |
| **Total**                            | **33 to 45 days** |

That gives us roughly seven to nine working weeks. Phase 6 (Tier-1 cohort) can start as soon as Group 1 plus Group 2 are merged, since the framework changes there are minor; the rest of Phase 5 can proceed in parallel.

---

## Open questions

Each must be resolved before its respective task lands. Owner column is the role responsible for closing the question.

| # | Question                                                                                                  | Owner       |
|---|------------------------------------------------------------------------------------------------------------|-------------|
| 1 | Do we ship a DPAPI-wrapped sidecar copy of `auth.json` from day one, or only after Phase 5 ships?           | Tech lead   |
| 2 | Where exactly does the Buy Credits log live: `%TEMP%\codexbar-buy-credits.log` or under `%LOCALAPPDATA%\CodexBar\logs\`? Spec 41 says `%TEMP%`; spec 60 hints at `%LOCALAPPDATA%`. Pick one. | Tech lead |
| 3 | Should the scrape script be hot-patchable at runtime (downloaded from a CDN, signature-checked) or always shipped baked into the binary? Hot-patch trades velocity for an extra trust-boundary review. | Security    |
| 4 | What is the v20 cookie banner's exact wording? Draft: `Chrome 127+ requires a manual cookie paste for this provider. Open settings > paste cookie header > done.` | UX writer   |
| 5 | The Mac flow lets the user toggle "Disable Keychain access". Windows has no equivalent. Do we hide the toggle entirely or show it disabled with an explainer? | UX writer   |
| 6 | Promotion failure during the final live-swap step (e.g. cross-volume rename failure): do we offer a one-click rollback, or just surface `liveAuthSwapFailed` and let the user re-promote? | Tech lead |
| 7 | Should `historicalTrackingEnabled = false` retroactively delete history, or just stop writing new rows?    | Product     |
| 8 | The Mac flow listens for `Update available!` and `Run bun install -g @openai/codex` to trigger "CLI update needed". The exact set of trigger substrings is brittle; should we add a fuzzy match (e.g. any `\d+\.\d+\.\d+ ->` pattern)? | Tech lead |
| 9 | Battery-saver semantics: spec 41 says "reduce background refresh frequency for the web path". What is the target cadence? Mac uses ~5 min vs ~30 s. Confirm. | Product     |
| 10 | The workspace picker dialog: native Win32 dialog or in-app WebView2 modal? In-app is the consistent choice but adds a second window class. | UX writer |

---

## Reading the phase

The phase reads top to bottom. A new engineer should be able to skim the Deliverables, scan the atomic task list, and pick a Group to start on without needing to read the spec front to back. The spec is the source of truth for content; this plan is the source of truth for sequencing.

When two specs disagree, pin the source-of-truth interpretation here and update the spec in a follow-up PR. Examples in this phase:

- Spec 41 says "on this Mac" for `liveAccountUnreadable`. We use "on this PC" on Windows. Spec 41 §6.10 notes this parenthetically; we follow it.
- Spec 41 says cookies cache lives at Keychain `com.steipete.codexbar.cache`. We map this to Credential Manager `CodexBar.cache`. Spec 41 §13 row "Cookie cache" agrees; we follow it.
- Spec 41 §8 says "Buy credits" log at `~/Library/Caches/.../codexbar-buy-credits.log`. We map to `%TEMP%\codexbar-buy-credits.log` per spec 41 §15.5 row "Debug buy-credits log".

---

## Phase 5 exit criteria checklist

For the engineer landing the last commit of the phase. Tick every box before the PR.

- [ ] All 12 acceptance tests pass on a clean machine.
- [ ] `cargo test --workspace` green.
- [ ] `pnpm -C apps/desktop-tauri test` green.
- [ ] `pnpm -C apps/desktop-tauri typecheck` green.
- [ ] No `unwrap()` on user-facing paths in the Codex code (verified by `grep -R "unwrap()" rust/src/providers/codex` returning only test files).
- [ ] No em dashes in source or docs (verified by `grep -R "—" rust/src/providers/codex docs/windows/plan/phase-5-codex.md` empty).
- [ ] The README under `docs/windows/plan/` lists Phase 5 as complete.
- [ ] A release note draft exists under `docs/windows/release-notes/<version>.md`.
- [ ] The maintainer of `docs/windows/spec/41-provider-codex.md` is pinged for a final parity review (Mac-to-Windows behavioural diff).
- [ ] A short demo video is attached to the final PR showing: live OAuth card, CLI fallback after deleting `auth.json`, web extras after enabling, promotion of a managed account, and Buy Credits opening pre-authenticated.

---

## Appendix A: file-tree delta after Phase 5

```
rust/src/
  host/
    cookies/
      chromium_v10.rs           (new)
      local_state.rs            (new)
      v20_guard.rs              (new)
      firefox.rs                (new)
    process_group.rs            (new)
  providers/codex/
    mod.rs                      (new)
    auth/
      mod.rs                    (new)
      credentials.rs            (new)
      jwt.rs                    (new)
      identity.rs               (new)
      refresh.rs                (new)
      errors.rs                 (new)
      dpapi_mirror.rs           (new)
      tests/
        jwt_parsing.rs          (new)
        jwt_fixtures/           (new, 7 files)
    oauth/
      usage.rs                  (new)
      wham_response.rs          (new)
      fallback.rs               (new)
      tests/
        fallback_truth_table.rs (new)
    cli/
      binary_locator.rs         (new)
      rpc_framer.rs             (new)
      subprocess.rs             (new)
      rpc_strategy.rs           (new)
      pty_diagnostic.rs         (new)
      status_probe.rs           (new)
    web/
      cookie_source.rs          (new)
      account_jars.rs           (new)
      cookie_cache.rs           (new)
      scrape_script.rs          (new)
      scrape_result.rs          (new)
      text_parsing.rs           (new)
      api_preflight.rs          (new)
      webview2_fallback.rs      (new)
      usage_breakdown.rs        (new)
      diagnostics.rs            (new)
      buy_credits_window.rs     (new)
    promotion/
      mod.rs                    (new)
      types.rs                  (new)
      preparation.rs            (new)
      converged.rs              (new)
      planning.rs               (new)
      execution.rs              (new)
      home_factory.rs           (new)
      service.rs                (new)
      live_swap.rs              (new)
      coordinator.rs            (new)
      errors.rs                 (new)
      STATE_MACHINE.md          (new)
      tests/
        planner_matrix.rs       (new)
        planner_replay.rs       (new)
        executor_safety.rs      (new)
        error_strings.rs        (new)
        cases/                  (new, 12+ yaml files)
    managed/
      catalog.rs                (new)
      migration.rs              (new)
      home_factory.rs           (new)
      service.rs                (new)
      login_runner.rs           (new)
      workspace_resolver.rs     (new)
      tests/
        migration.rs            (new)
        fixtures/v1.json        (new)
        fixtures/v2_expected.json (new)
    history/
      keys.rs                   (new)
      ownership.rs              (new)
      context.rs                (new)

apps/desktop-tauri/src/providers/codex/
  index.tsx                     (new)
  CodexProviderCard.tsx         (new)
  CreditsHistoryChart.tsx       (new)
  CodexSettingsPane.tsx         (new)
  AccountSwitcher.tsx           (new)
  StackedAccountCards.tsx       (new)
  SystemAccountSubmenu.tsx      (new)
  WorkspacePickerDialog.tsx     (new)
  BuyCreditsButton.tsx          (new)

docs/windows/plan/
  phase-5-codex.md              (this file)
```

---

## Appendix B: Tauri command surface added in this phase

| Command                                                              | Effect                                                                                                                                              |
| -------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| `codex_promote_managed_account({ id })`                              | Runs the promotion flow. Streams progress via `event("codex/promotion/state", ...)`. Resolves with the final outcome.                               |
| `codex_authenticate_managed_account({ existing_id?, timeout_ms })`   | Spawns `codex login` in a fresh managed home. Streams progress. Optionally shows the workspace picker. Resolves with the final managed-account row. |
| `codex_remove_managed_account({ id })`                               | Sandbox-validated removal of a managed account.                                                                                                     |
| `codex_set_active_source({ source })`                                | Writes `codexActiveSource`. Kicks a scoped refresh. No file mutation unless the user explicitly promotes via the submenu.                           |
| `codex_set_cookie_source({ mode })`                                  | Switches off, auto, or manual. On switch to off, clears the cached cookie header.                                                                   |
| `codex_set_cookie_header({ header })`                                | Stores the manual cookie header. Revalidates on the next refresh.                                                                                   |
| `codex_open_buy_credits_window({ url, account_email, auto_start })`  | Opens the WebView2 window per task 3.12.                                                                                                            |
| `codex_force_web_refresh({ account_email })`                         | Forces a fresh dashboard scrape (bypasses battery saver and cache).                                                                                 |
| `codex_run_status_diagnostic()`                                      | Spawns ConPTY `/status` (debug menu only).                                                                                                          |

Every command emits a `codex/state-change` event after it mutates settings or storage, so the React layer can subscribe once and re-render against the new snapshot rather than re-invoking ad hoc commands.

---

End of Phase 5 plan.
