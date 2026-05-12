---
summary: "Auth, cookies, Keychain, and secret-storage subsystem blueprint for the Windows (Tauri 2 + React + shared Rust crate) refactor."
read_when:
  - Implementing secret storage, OAuth, or cookie import on Windows
  - Designing the prompt UX for first-time secret access
  - Auditing security boundaries when porting a provider
audience: Rust + TypeScript engineer with no Swift background
sibling_specs:
  - docs/windows/01-mac-platform-dependencies.md
  - docs/windows/04-recommended-architecture.md
status: draft-blueprint
---

# 60 — Auth, cookies, Keychain, and secrets

This document is the contract between the macOS sources and the Windows refactor for everything that stores or reads a secret: OAuth refresh tokens, browser-derived cookie headers, manual API keys, per-provider session cookies, and the rate-limit/cooldown state that protects users from prompt storms.

The Windows app must be **security-correct first** and **Phantom-wallet / Duolingo-polished second**. If those two ever conflict, security wins — but on this surface they should almost never conflict. Most of the friction in the macOS flow comes from Keychain ACL behavior that does **not exist** on Windows. We are inheriting a contract, not a chore.

Reading order:

1. Threat model (§1).
2. macOS Keychain catalog (§2) — what items exist *today*; the names matter for migration tooling we may ship.
3. Prompt policy state machine (§3) — the user-facing contract.
4. Browser cookie import (§4–§6) — the heaviest section, with Chromium decryption details.
5. OAuth flows (§7) — per provider, with PKCE / device-flow / file-fallback table.
6. Token-account model (§8) — multi-account manual tokens.
7. Per-provider store catalog (§9).
8. Config file (§10) and migrations (§11).
9. Paths (§12), Mac→Windows mapping (§13), logging discipline (§14), acceptance checklist (§15).

---

## 1. Threat model

### 1.1 What the app must protect

| Asset                                            | Sensitivity                                | Currently protected by                                                       |
| ------------------------------------------------ | ------------------------------------------ | ---------------------------------------------------------------------------- |
| Claude / Codex / Antigravity OAuth refresh tokens | High — long-lived, can mint access tokens. Refresh tokens cannot be revoked easily by the user. | macOS Keychain (`ThisDeviceOnly` AfterFirstUnlock), `~/.claude/.credentials.json` (file perms), `~/.codex/auth.json` (file perms). |
| OAuth access tokens                              | Medium — short-lived (Claude expiresAt in ms; Codex 8-day refresh window). | Same as refresh tokens. Often cached in memory for ≤30 min.                  |
| Browser-derived session cookie headers           | High — equivalent to logged-in browser. Often longer-lived than refresh tokens. | Keychain cache (`com.steipete.codexbar.cache`, account `cookie.<provider>`), keyed per managed account UUID. |
| Manual API keys (Zai, Synthetic, Copilot PAT, Kimi K2 …) | High — bearer credentials.        | `~/.codexbar/config.json` (0600). Previously per-provider Keychain items.    |
| Manual `Cookie:` headers pasted by user          | High — same as browser cookies.            | Same as manual API keys: config file.                                        |
| Claude `sessionKey` from manual paste            | High.                                      | Config file as a `tokenAccounts[].token`. Routed through `ClaudeCredentialRouting`. |
| In-memory caches (LRU/TTL)                       | High while resident.                       | Process address space only. Cleared on app exit, on settings change, on “Clear caches”. |
| Anti-prompt-storm cooldown state                 | Low (metadata).                            | `UserDefaults` (`claudeOAuthKeychainDeniedUntil`, `browserCookieAccessDeniedUntil`). |

### 1.2 What the app does **not** protect against

- A user with read access to their own DPAPI blob. DPAPI ties to the **current user’s Windows logon**, not to CodexBar; any process running as the same user can read what we encrypt. This is identical to the Keychain situation on macOS, and we accept it.
- Local administrator. Admin on the box can read the user’s registry hive, read the user’s DPAPI master key (with the LSA secret), or attach to our process. This is out of scope.
- Malware running as the user. Out of scope; same as above.
- Backups of `%APPDATA%` that the user themselves take. We do not encrypt the config file beyond ACLs (see §10) — the user owns the secrets and is allowed to back them up.
- Network adversaries (TLS is the OS / `reqwest` stack’s job). We assume HTTPS to provider endpoints.
- A *different Windows user* on the same machine — DPAPI under user scope is sufficient. We deliberately do **not** use `LocalMachine` scope for DPAPI; that would weaken the boundary.

### 1.3 Non-goals

- We do **not** attempt to defeat Chromium’s App-Bound Encryption (v20). When we hit a v20-only cookie, we fall back to manual paste with clear UX (see §4.6).
- We do **not** try to share secrets with another browser, another machine, or iCloud Keychain analogs. Each install is independent.
- We do **not** offer an in-app way to view raw secret values after they are saved. The user can edit `config.json` directly if they need to inspect.

### 1.4 Trust boundaries

```
+----------------------------+    DPAPI / Credential Manager     +-----------------+
|  CodexBar process (user)   | <-------------------------------> | LSA / DPAPI key |
+----------------------------+                                   +-----------------+
        |                                                                ^
        | reqwest TLS                                                    | derived per-user
        v                                                                |
+----------------------------+                                   +-----------------+
| Provider API (anthropic,   |                                   | Windows logon   |
| openai, github, ...)       |                                   | session         |
+----------------------------+                                   +-----------------+
        ^
        | local SQLite copy + AES-GCM
+----------------------------+
| Chromium cookies on disk   |
+----------------------------+
```

Any new code path that crosses a boundary (e.g. introducing IPC, a helper service, or a “share with `cli`” feature) **must** be reviewed against this picture before merge.

---

## 2. macOS Keychain usage today (catalog)

Every Keychain item written or read by macOS CodexBar, with attributes. This is the source of truth for the migration importer we may ship on Windows (read on first launch from old `%APPDATA%` and reflow into the new storage). It is **not** a target schema — Windows does not use Keychain.

### 2.1 Items written by the app

| Service                          | Account                                | Class                   | Accessibility                                  | Notes                                                                                                             |
| -------------------------------- | -------------------------------------- | ----------------------- | ---------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| `com.steipete.CodexBar`          | `codex-cookie`                         | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Legacy `KeychainCookieHeaderStore`. Now migrated into `config.json.cookieHeader`. Item is deleted post-migration. |
| `com.steipete.CodexBar`          | `claude-cookie`                        | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `cursor-cookie`                        | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `opencode-cookie`                      | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `factory-cookie`                       | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `minimax-cookie`                       | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `minimax-api-token`                    | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `augment-cookie`                       | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `amp-cookie`                           | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `copilot-api-token`                    | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same. GitHub device-flow token.                                                                                   |
| `com.steipete.CodexBar`          | `zai-api-token`                        | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `synthetic-api-key`                    | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `kimi-auth-token`                      | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.CodexBar`          | `kimi-k2-api-token`                    | GenericPassword         | `AfterFirstUnlockThisDeviceOnly`               | Same.                                                                                                             |
| `com.steipete.codexbar.cache`    | `cookie.<provider>` or `cookie.<provider>.<scope>` | GenericPassword | `AfterFirstUnlockThisDeviceOnly` + `SecAccess` (trusted apps: app bundle, CLI helper) | Live runtime cache. Holds the normalized `Cookie:` header, source label, and timestamp. JSON-encoded `Entry`. Still actively used. |
| `com.steipete.codexbar.cache`    | `oauth.claude`                         | GenericPassword         | Same                                          | Claude OAuth credential snapshot (bytes from `~/.claude/.credentials.json`) + storedAt + owner.                   |

The cache service `com.steipete.codexbar.cache` adds a `SecAccess` ACL listing the app bundle and the bundled `CodexBarCLI` helper as trusted, so reads from both binaries do not prompt.

### 2.2 Items read by the app (no UI prompt path)

The app uses `KeychainNoUIQuery` to **probe** items without prompting. This is the most important macOS-specific detail and the one that *vanishes on Windows* — DPAPI doesn’t prompt, period.

`KeychainNoUIQuery.apply` adds these attributes to every query that must not surface UI:

| Attribute                          | Value                                         | Purpose                                                                                                                                                            |
| ---------------------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `kSecUseAuthenticationContext`     | `LAContext` with `interactionNotAllowed=true` | Suppresses biometric prompt and ACL grant UI.                                                                                                                      |
| `kSecUseAuthenticationUI`          | `kSecUseAuthenticationUIFail` (resolved via `dlsym` to preserve the constant despite deprecation) | If the secret would require interaction, fail with `errSecInteractionNotAllowed` instead of showing UI.                                                            |

This pair is applied to every preflight (`KeychainAccessPreflight.checkGenericPassword`), every cache read, and every Claude OAuth background read. The Windows analogue is **trivially “always succeeds without UI”**, so the code path collapses to a direct read.

### 2.3 The `security(1)` CLI fallback

`ClaudeOAuthCredentials+SecurityCLIReader.swift` shells out to `/usr/bin/security find-generic-password -s "Claude Code-credentials" -a <account> -w` with a 1.5 s timeout. This is gated behind an experimental setting (`ClaudeOAuthKeychainReadStrategy.securityCLIExperimental`) because on some macOS versions the framework path prompts while the CLI path does not. The Windows port does not need this — there is no equivalent UI prompt to dodge.

### 2.4 Items read but **never** written by CodexBar

| Service                  | Account            | Owner       | What it stores                                                                 |
| ------------------------ | ------------------ | ----------- | ------------------------------------------------------------------------------ |
| `Claude Code-credentials` | per-user           | Claude CLI  | OAuth blob (`claudeAiOauth.accessToken`, `refreshToken`, `expiresAt` in ms, scopes, `subscriptionType`, `rateLimitTier`). |
| `Chrome Safe Storage`    | `Chrome`           | Google Chrome | Random per-install AES key used to decrypt `encrypted_value` in `Cookies` SQLite. Same shape for every Chromium fork — see §4.4. |
| `Brave Safe Storage`     | `Brave`            | Brave       | Same.                                                                          |
| `Microsoft Edge Safe Storage` | `Microsoft Edge` | Edge      | Same.                                                                          |
| `Arc Safe Storage`       | `Arc`              | Arc         | Same.                                                                          |
| `Vivaldi Safe Storage`   | `Vivaldi`          | Vivaldi     | Same.                                                                          |
| `Yandex Safe Storage`    | `Yandex`           | Yandex      | Same.                                                                          |
| `Helium Safe Storage`    | `Helium`           | Helium      | Same.                                                                          |
| (others)                 | …                  | various     | All Chromium derivatives we list in `BrowserCookieImportOrder`.                |

### 2.5 Read paths summary (high-level)

| Caller                                              | Service                          | Account              | Prompt allowed?                            | Cooldown gate                          |
| --------------------------------------------------- | -------------------------------- | -------------------- | ------------------------------------------ | -------------------------------------- |
| `KeychainCacheStore.load`                           | `com.steipete.codexbar.cache`    | `cookie.*`, `oauth.*` | No (always no-UI)                          | None — `temporarilyUnavailable` returned to caller. |
| `KeychainCookieHeaderStore.loadCookieHeader`        | `com.steipete.CodexBar`          | `<provider>-cookie`  | Preflight; if `interactionRequired`, fire user prompt and then attempt real read | None.                                  |
| `*TokenStore.loadToken` (per-provider catalog §9)   | `com.steipete.CodexBar`          | `<provider>-…-token` | Same pattern.                              | None.                                  |
| `ClaudeOAuthCredentialsStore.loadFromClaudeKeychainWithPromptIfAllowed` | `Claude Code-credentials` | per-user | Gated by `ClaudeOAuthKeychainAccessGate.shouldAllowPrompt` + `ClaudeOAuthKeychainPromptPreference` + `ProviderInteractionContext` | 6-hour cooldown on denial. Cleared by user action. |
| `BrowserCookieAccessGate.shouldAttempt(browser)`    | `<browser> Safe Storage`         | `<browser>`          | Preflight only; if `interactionRequired`, abort and cool down. | 6-hour cooldown per browser.           |
| `AlibabaChromiumCookieFallbackImporter.derivedKeys` | `<browser> Safe Storage`         | `<browser>`          | Plain `SecItemCopyMatching` (may prompt — gated by preflight above) | Aborts if preflight says interaction required. |

---

## 3. Keychain prompt policy state machine

The Claude OAuth prompt is the **only** secret access that supports a three-way user preference. Manual tokens and cookies go through a simpler path (`KeychainPromptHandler.notify` → modal `NSAlert`).

### 3.1 Modes

| Mode                  | Background flows                          | User-initiated flows (`ProviderInteractionContext.userInitiated`) |
| --------------------- | ----------------------------------------- | ---------------------------------------------------------------- |
| `never`               | Never prompt. Use cache / file / env only. Errors surface as “Run `claude` to re-authenticate.” | Never prompt.                                                    |
| `onlyOnUserAction` (default) | Never prompt. Background uses `KeychainNoUIQuery` and treats `interactionRequired` as “unavailable, cool down”. | Prompt allowed (once per user gesture).                          |
| `always`              | Prompt allowed (with cooldown).           | Prompt allowed.                                                  |

### 3.2 Cooldown

`ClaudeOAuthKeychainAccessGate.deniedUntil`:

- On any denial that returns `errSecUserCanceled` / `errSecInteractionNotAllowed` / `errSecAuthFailed` / `errSecNoAccessForItem`, cool down for **6 hours**.
- Persisted to `UserDefaults` (`claudeOAuthKeychainDeniedUntil`).
- Cleared on any **user action** (menu open with focus, settings change, manual “Refresh now”).
- The cooldown overrides the mode setting: even `always` will not prompt while cool.

### 3.3 Interaction context

`ProviderInteractionContext.current` is a `TaskLocal` set to `.userInitiated` by the menu, refresh button, settings UI, and CLI. Background refresh tasks set it to `.background`. The router uses this to decide whether to risk a prompt.

### 3.4 Interaction with “Disable Keychain access”

Advanced → Disable Keychain access flips `KeychainAccessGate.isDisabled`. When enabled:

- All Keychain reads / writes short-circuit to “unavailable” / no-op.
- The mode dropdown remains visible but is inert (a banner explains why).
- Browser-cookie import becomes restricted to Safari + Firefox + Zen + any other browser whose `usesKeychainForCookieDecryption == false`.
- Manual cookie / API key fields in Settings still work (they go to `config.json`).

### 3.5 Windows analogue

Windows DPAPI does not prompt under user scope. The mode dropdown should be **dropped on Windows**. The single Windows-side debug toggle is:

| Advanced toggle (Windows)        | Effect                                                                                         |
| -------------------------------- | ---------------------------------------------------------------------------------------------- |
| “Disable secret storage” (debug) | Disables DPAPI encryption — secrets are stored as plain JSON inside `config.json`. **Refuses to start unless `DEBUG` build.** Logs a banner in the status menu. |

Don’t expose “never / only on user action / always” — it’s confusing on Windows where there is no prompt to gate.

The Windows equivalent of the *cooldown* concept is **still useful** for cookie imports that hit App-Bound Encryption (v20) — we cool down per-browser for 6 hours after a v20-only failure to avoid hammering the user with the manual-paste banner.

---

## 4. Browser cookie import flow (Mac, then Windows)

This is the most complex subsystem and the one with the biggest macOS / Windows behavioural delta.

### 4.1 Order of attempts (per provider)

`BrowserCookieImportOrder` is per-provider via `ProviderDefaults.metadata[<provider>].browserCookieOrder`, with `Browser.defaultImportOrder` as the fallback. The OpenAI-dashboard importer’s order is representative:

1. **Cached cookie header** (`CookieHeaderCache.load(provider:scope:)`) — bypasses browser scanning entirely.
2. **Safari** — first because it never touches the Chromium Safe Storage Keychain.
3. **Chrome / Chromium forks** (Chrome, Chrome Beta/Canary, Arc/Arc Beta/Canary, Brave/Brave Beta/Nightly, Edge/Edge Beta/Canary, Chromium, Vivaldi, Yandex, Helium, Dia, ChatGPT Atlas, Comet, ...).
4. **Firefox / Zen / other Gecko**.

The candidate set is filtered before any read by `BrowserCookieImportOrder.cookieImportCandidates(using: BrowserDetection)`:

- Browser must have **usable cookie store** on disk (Chromium: `Default/Cookies` or `Default/Network/Cookies`; Firefox: at least one `*.default*/cookies.sqlite`).
- If `KeychainAccessGate.isDisabled`, Chromium browsers are excluded.
- If `BrowserCookieAccessGate.shouldAttempt(browser)` is false (preflight says interaction required, or per-browser cooldown is hot), excluded.

### 4.2 Safari (macOS only — drop on Windows)

- Reads `~/Library/Cookies/Cookies.binarycookies`.
- The binary format is parsed by **SweetCookieKit** (vendored). It’s a packed table of `<page header><cookie records>` with little-endian ints, NaN-coded dates, and trailing checksum. Documented in Apple’s WWDC materials and reverse-engineered widely.
- No decryption, no Keychain. Just file read.
- **On Windows: DROP.** Safari is unavailable. Document in onboarding: “If you only use Safari, run CodexBar on macOS.”

### 4.3 Firefox / Gecko (cross-platform, easy)

- Profile root: macOS `~/Library/Application Support/Firefox/Profiles/`, Windows `%APPDATA%\Mozilla\Firefox\Profiles\`.
- Pick directories matching `*.default*` (case-insensitive).
- Cookie store is **unencrypted SQLite** at `<profile>/cookies.sqlite`.
- Schema (Firefox 100+):
  - Table `moz_cookies` with columns `host, name, path, value, expiry, isSecure, isHttpOnly, sameSite, originAttributes, ...`.
  - `expiry` is seconds since epoch (not Chromium’s WebKit-epoch).
- Copy the file (with `-wal`/`-shm`) to a temp dir before opening read-only — Firefox holds an exclusive lock when running.
- Same code on Windows. Only the path changes.

### 4.4 Chromium on macOS (today)

`AlibabaChromiumCookieFallbackImporter` is the canonical readable example.

1. Open `Local State` is **not** needed on macOS; Chrome derives keys from the Keychain on every launch.
2. Look up the per-browser Safe Storage password from Keychain (`<browser> Safe Storage` / account `<browser>`). One call per browser, can prompt.
3. **Derive AES-128 key**:
   - PBKDF2-HMAC-SHA1, salt `"saltysalt"` (literal 9 bytes), iterations **1003**, output 16 bytes.
4. **Decrypt** each cookie blob:
   - Blob layout: `b"v10"` (3 bytes) || ciphertext.
   - **AES-128-CBC**, IV is `0x20` × 16 (literal ASCII space bytes), PKCS7 padding.
   - On Chrome ≥ 80 some installs prefix a 32-byte SHA-256 of the host as an authentication tag; the importer drops the first 32 bytes if direct UTF-8 decode fails.
5. Cookie SQLite location: `<profile>/Cookies` (legacy) or `<profile>/Network/Cookies` (Chrome ≥ 96). Copy to temp before opening.
6. Cookies table columns of interest: `host_key`, `name`, `path`, `expires_utc`, `is_secure`, `value`, `encrypted_value`. `expires_utc` is **microseconds since 1601-01-01 UTC** (WebKit epoch) — convert with `(value / 1e6) - 11_644_473_600`.

### 4.5 Chromium on Windows (target behaviour) — **the critical section**

On Windows, the Chromium cookie blob format and key derivation are **different from macOS**. This is the part we are re-implementing, not porting.

#### 4.5.1 V10 (DPAPI-wrapped AES-256-GCM key) — the common case

| Step | Where the data lives                                                                                                                                                                                                                                  | What to do                                                                                                                                                                                                                                                                                                                          |
| ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | `%LOCALAPPDATA%\<vendor>\<browser>\User Data\Local State` (JSON)                                                                                                                                                                                      | Parse `os_crypt.encrypted_key` (base64 string).                                                                                                                                                                                                                                                                                     |
| 2    | First 5 bytes of decoded `encrypted_key` are the ASCII string `"DPAPI"`. Strip them.                                                                                                                                                                  | Call `CryptUnprotectData` (Win32) on the remainder. **No entropy**, **user scope** (`CRYPTPROTECT_UI_FORBIDDEN`). Result is a 32-byte AES-256 key.                                                                                                                                                                                  |
| 3    | `<profile>/Network/Cookies` (Chrome ≥ 96) or `<profile>/Cookies` (older). Copy to temp dir (Chromium WAL).                                                                                                                                              | Open read-only. Query `SELECT host_key, name, path, expires_utc, is_secure, encrypted_value FROM cookies`.                                                                                                                                                                                                                          |
| 4    | Each `encrypted_value` blob starts with `b"v10"` (3 bytes).                                                                                                                                                                                            | Next 12 bytes are the AES-GCM **nonce**. Remainder is `ciphertext || 16-byte GCM tag`. Decrypt with the 32-byte key from step 2. Plaintext is UTF-8 cookie value. **No salt, no PBKDF2 on Windows.**                                                                                                                                  |

If `encrypted_value` lacks the `v10` prefix and is non-empty, it’s a legacy plain-DPAPI blob from very old Chromium — `CryptUnprotectData` on the raw bytes (rare path).

If `encrypted_value` is empty, fall back to the plaintext `value` column (very old, mostly migrated away).

#### 4.5.2 V20 (App-Bound Encryption) — **drop and fall back to manual**

Chrome ≥ 127 introduced “App-Bound Encryption”. Affected cookie blobs are prefixed with `b"v20"` instead of `v10`.

The v20 key path is intentionally hostile to non-Chrome processes:

- The wrapped key in `Local State` is `os_crypt.app_bound_encrypted_key` (different field).
- Unwrapping requires a COM call to a Chrome-elevation-service COM object that the OS allows **only when the caller binary path matches Chrome’s install path** (this is enforced via path-based ACLs).
- There are public reverse-engineered scripts. They:
  - Either require the user to grant admin to inject into `chrome.exe` (unacceptable for us).
  - Or rely on Chrome bugs that close on each minor release.

**Our policy**: do **not** attempt v20 decryption. When we see `b"v20"` in `encrypted_value`:

1. Tally v20-only count per cookie source.
2. If **every** target cookie for the domain we need is v20 (and no v10 fallback is available), abort that browser with `BrowserCookieError.accessDeniedHint("App-Bound Encryption (v20) — requires manual cookie paste")`.
3. Open the per-browser cooldown for that browser (6 h) — same code path as the macOS prompt cooldown.
4. Surface a one-time toast in the UI: “Chrome 127+ requires a manual cookie paste for this provider. Open settings → paste cookie header → done.”

Falling back cleanly here is the polish move. Make the *manual paste* UI Phantom-good (see §4.7).

#### 4.5.3 Chrome key-rotation edge

When Chrome itself rotates the `os_crypt.encrypted_key` (rare; happens after a profile reset or master key recovery), the SQLite cookies become undecryptable with our cached key. Detection: AES-GCM tag mismatch on every blob in a row. Action: clear our cookie header cache for that browser/provider, retry the import (forces re-read of `Local State`).

### 4.6 Firefox on Windows

Identical to §4.3 modulo path. Copy `cookies.sqlite` (+ `-wal`/`-shm`) to temp, open read-only, query `moz_cookies`.

Path: `%APPDATA%\Mozilla\Firefox\Profiles\*.default*\cookies.sqlite`.

Also support Zen browser: `%APPDATA%\zen\Profiles\` with the same Gecko store schema.

### 4.7 Cached cookie-header path (`CookieHeaderCache`) — the “don’t re-import” fast path

Even when a browser source is available, we **prefer the cache**.

- Cache key (`KeychainCacheStore.Key`):
  - category: `"cookie"`
  - identifier: `<provider.rawValue>` (global) or `<provider.rawValue>.<scope>` (per-managed-account on Codex).
  - On Windows, the *storage* changes (DPAPI blob in `%LOCALAPPDATA%\CodexBar\cookie-cache\<category>\<identifier>.bin`), but the **key schema is preserved** for migration parity.
- Entry payload (`CookieHeaderCache.Entry`):
  ```json
  {
    "cookieHeader": "name=value; other=value; ...",
    "storedAt": "2026-05-12T00:11:22Z",
    "sourceLabel": "Chrome / Default"
  }
  ```
- On read: if found, used immediately. If the importer subsequently determines the cached header is invalid (auth probe fails with `dashboardStillRequiresLogin` / `noMatchingAccount` / `manualCookieHeaderInvalid`), the cache entry is **cleared** and the browser-scan path runs.
- On store: header is normalized (`CookieHeaderNormalizer.normalize`) before write. Empty / unparseable headers clear the entry instead of writing.
- TTL: **no TTL.** The cache is invalidated only by:
  - Auth-probe failure.
  - User clears caches (Preferences → Debug → Caches).
  - `codexbar cache clear --cookies [--provider X]`.

### 4.8 `BrowserCookieAccessGate` — the prompt-storm shield

| State                                   | Triggered by                                                                                              | Effect                                                              | Cleared by                                  |
| --------------------------------------- | --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ------------------------------------------- |
| `KeychainAccessGate.isDisabled == true` | Advanced → Disable Keychain access (mac), or Windows analogue.                                            | All Chromium browsers excluded entirely from candidate list.        | User unchecks the toggle.                   |
| `deniedUntilByBrowser[<browser>]`       | (a) Preflight returns `interactionRequired` (mac only — n/a on Windows). (b) Real decryption fails with access-denied. (c) Windows: v20-only failure. | Browser is excluded from candidate list for 6 h.                    | Time elapses, or user clears via debug menu. |
| `BrowserDetection.isCookieSourceAvailable` returns false | Profile or cookie file missing on disk.                                                                   | Browser excluded.                                                   | Browser is installed and used.              |

### 4.9 `CookieHeaderNormalizer`

The user pastes random things. The normalizer accepts:

| Input form                                   | Example                                                              |
| -------------------------------------------- | -------------------------------------------------------------------- |
| Bare header                                  | `key=value; other=value`                                             |
| With `Cookie:` prefix                        | `Cookie: key=value; ...`                                             |
| `-H 'Cookie: ...'` (curl)                    | `-H 'Cookie: key=value; ...'`                                        |
| `--cookie 'k=v;'` / `-b 'k=v;'` (curl)       | `--cookie 'key=value; other=value'`                                  |
| Wrapped in single or double quotes           | `"key=value; other=value"`                                           |

Output: a `name=value; name=value` string, trimmed, with the `Cookie:` prefix stripped, wrapping quotes removed.

`pairs(from:)` returns the parsed `[(name, value)]` for downstream cookie reconstruction. `filteredHeader(from:allowedNames:)` is used by Claude session-key extraction.

---

## 5. `CookieHeaderCache` + Normalizer — disk format and validity

(Already partially covered in §4.7. This section is the reference.)

### 5.1 Format on disk

#### macOS (today)

- Keychain item, service `com.steipete.codexbar.cache`, account `cookie.<identifier>`.
- Value is JSON `Entry` (see §4.7), ISO-8601 dates.
- Trusted apps ACL: app bundle + `CodexBarCLI`.
- Accessibility: `AfterFirstUnlockThisDeviceOnly`.
- A *legacy file fallback* exists at `~/Library/Application Support/CodexBar/<provider>-cookie.json` for older installs; on read, the entry is migrated into Keychain and the file is deleted.

#### Windows (target)

- File path: `%LOCALAPPDATA%\CodexBar\cookie-cache\<category>\<identifier>.bin` (one file per cache key).
- File contents: `CryptProtectData(json_payload, entropy=None, scope=user)`. The result is a raw DPAPI blob — opaque to anything but our user’s LSA.
- ACL: parent directory created with `SetNamedSecurityInfo` granting `OWNER_RIGHTS` (current user) full control, no inherited rights. Belt and braces over DPAPI.
- No legacy file fallback (clean install).

### 5.2 Validity check

- On `load`, decrypt → JSON-decode → check `cookieHeader` non-empty and `storedAt` parses.
- A read failure (corrupt blob, decryption fails) returns `.invalid` and the file is deleted.
- A successful read returns `.found(entry)`. The caller decides whether to *verify* via an auth probe before using the value.

### 5.3 Refresh policy

| Trigger                                              | Action                                                                                          |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Auth probe succeeds with cached header               | Reuse, no write.                                                                                |
| Auth probe fails (login required / mismatch / etc.)  | Clear entry, fall through to browser import.                                                    |
| Browser import succeeds                              | Write new entry. Source label = display name of source (`"Chrome / Default"`, `"Safari"`, `"Manual"`). |
| User sets manual cookie in Settings                  | Write entry with source label `"Manual"`. Provider config `cookieSource = .manual` is set.       |
| User runs `codexbar cache clear --cookies`           | Clear all cookie cache entries.                                                                 |
| User runs `codexbar cache clear --cookies --provider X` | Clear all scopes for provider X (including managed-account scopes for Codex).                |
| Provider account switch                              | Cache **per managed account UUID** — switching is free.                                          |

---

## 6. `BrowserCookieAccessGate` — when import is allowed

Restated as a flowchart for clarity.

```
shouldAttempt(browser):
  if browser.usesKeychainForCookieDecryption == false:
      return true   // Safari, Firefox, Zen — never gated.
  if KeychainAccessGate.isDisabled:
      return false  // Chromium imports off entirely.
  if deniedUntilByBrowser[browser] > now:
      return false  // cooldown.
  preflight = checkGenericPassword("<browser> Safe Storage", "<browser>")
  if preflight == .interactionRequired:
      deniedUntilByBrowser[browser] = now + 6h
      return false
  return true       // .allowed or .notFound or .failure → proceed.
```

`recordIfNeeded(error)` is called by the import path; if it’s a `BrowserCookieError.accessDenied`, the gate is closed for 6 h.

**On Windows:** `usesKeychainForCookieDecryption` is irrelevant (DPAPI doesn’t prompt). The Windows gate uses **v20 detection** instead:

```
shouldAttempt(browser):
  if DisableSecretStorageDebug:                # debug-only toggle
      return false
  if v20DeniedUntilByBrowser[browser] > now:
      return false
  return true   // try the import; v20-only failure will set cooldown.
```

---

## 7. OAuth flow patterns

### 7.1 Per-provider matrix

| Provider     | Flow                                       | Where access/refresh tokens live (today, mac)                                                       | Refresh strategy                                                                                                              |
| ------------ | ------------------------------------------ | --------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Claude       | PKCE (Authorization Code + S256) — login is handled **by Claude CLI**; CodexBar only reads. | `Claude Code-credentials` Keychain item (Claude CLI’s), plus `~/.claude/.credentials.json`, plus our `oauth.claude` cache.    | Direct POST to `https://platform.claude.com/v1/oauth/token` with `grant_type=refresh_token`, `client_id=9d1c250a-e61b-44d9-88ed-5944d1962f5e`. We refresh in-process and persist the new tuple back to all three locations. Refresh can be delegated to Claude CLI on demand. |
| Codex        | OAuth — login handled by Codex CLI.        | `$CODEX_HOME/auth.json` (default `~/.codex/auth.json`). Refresh tokens persist 8-day windows.        | We invoke Codex CLI to refresh when `needsRefresh == true`; we do not own the refresh endpoint.                               |
| GitHub Copilot | OAuth Device Flow                        | `~/.codexbar/config.json` (`providers[copilot].apiKey`). One PAT per account, multiple accounts supported via `tokenAccounts`. | The device flow is in-app: `POST github.com/login/device/code` → poll `POST github.com/login/oauth/access_token` until user finishes. Client ID `Iv1.b507a08c87ecfe98` (VS Code’s; public, not a secret). |
| Antigravity  | Google OAuth (Authorization Code + PKCE)   | `~/.codexbar/antigravity/oauth_creds.json` (0600).                                                  | Standard Google `https://oauth2.googleapis.com/token` refresh. Client ID/secret discovered from installed Antigravity.app `main.js` (or env vars). |
| Vertex AI    | Google OAuth — login handled by `gcloud`.  | `~/.config/gcloud/application_default_credentials.json` (read-only by us).                          | We read; `gcloud` refreshes.                                                                                                  |
| All others (cookie / API-key providers) | No OAuth — manual paste of session cookie or API key. | `~/.codexbar/config.json` (cookieHeader / apiKey).                                                  | N/A.                                                                                                                          |

### 7.2 Shared helpers

There is *not* a fully shared OAuth abstraction today. Each provider re-implements:

- HTTP client (URLSession; on Windows: shared `reqwest` client in the Rust crate).
- JSON shape (`access_token` vs `accessToken`, `expires_in` vs `expiresAt`).
- Storage (Keychain vs file).

For the Windows refactor we should consolidate into a single Rust trait:

```rust
trait OAuthCredentialStore {
    fn load(&self) -> Result<OAuthCredentials, OAuthError>;
    fn save(&self, creds: &OAuthCredentials) -> Result<(), OAuthError>;
    fn refresh(&self, refresh_token: &str) -> Result<OAuthCredentials, OAuthError>;
}
```

with per-provider impls. Persistence: DPAPI-encrypted blob via `keyring` crate (which on Windows wraps Credential Manager) **for refresh tokens**, plain JSON file (DPAPI-protected) for the rest.

### 7.3 PKCE details (Claude)

- Code verifier: 32 bytes random, base64url no-pad.
- Code challenge: `BASE64URL(SHA256(verifier))`, `code_challenge_method=S256`.
- Redirect URI: `http://localhost:<random-port>/callback` (Claude CLI owns this on macOS; we don’t initiate the user flow ourselves).
- Refresh token rotation: yes — every refresh returns a new `refresh_token`. Persist immediately or you lose access.

### 7.4 Device flow details (Copilot)

`CopilotDeviceFlow`:

1. `POST https://github.com/login/device/code` with `client_id` + `scope=read:user` → `{ device_code, user_code, verification_uri, expires_in, interval }`.
2. Show `user_code` + open `verification_uri_complete` in browser.
3. Poll `POST https://github.com/login/oauth/access_token` with `grant_type=urn:ietf:params:oauth:grant-type:device_code` every `interval` seconds until non-`authorization_pending`.
4. On success, store the `access_token` (this is a long-lived GitHub PAT-like token with `read:user`; used to mint short-lived Copilot session tokens via `https://api.github.com/copilot_internal/v2/token`).

Enterprise hosts: replace `github.com` with the user-provided host. Validate via `URLComponents`.

### 7.5 Refresh-failure handling (Claude — most defensive)

`ClaudeOAuthRefreshFailureGate` adds **a global cooldown** on consecutive refresh failures so an expired refresh token does not hammer Anthropic. Windows should preserve this — if the user has revoked their session, retrying every refresh loop is rude.

---

## 8. Token-account model (multi-account manual tokens)

### 8.1 Storage

In `~/.codexbar/config.json`, each provider entry may have `tokenAccounts`:

```json
"tokenAccounts": {
  "version": 1,
  "activeIndex": 0,
  "accounts": [
    {
      "id": "9b3f9a91-...-uuid",
      "label": "Personal",
      "token": "sk-ant-...",
      "addedAt": 1735123456,
      "lastUsed": 1735220000,
      "externalIdentifier": "github-login-or-similar",
      "organizationId": "for-claude-org-disambiguation"
    }
  ]
}
```

### 8.2 Resolution

`TokenAccountSupportCatalog.support(for: provider)` returns a descriptor:

| Field                       | Meaning                                                                                                             |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `title`, `subtitle`, `placeholder` | UI strings.                                                                                                  |
| `injection`                 | Either `cookieHeader` (treat the token as a cookie) or `environment(key)` (set an env var when running CLI tools).   |
| `requiresManualCookieSource`| If true, switching active account also forces `cookieSource = manual` so the in-cookie tooling can find it.          |
| `cookieName`                | When `injection == cookieHeader` and the user pastes just the value, we wrap it as `<cookieName>=<value>`.            |

### 8.3 Claude’s special routing

`ClaudeCredentialRouting.resolve(tokenAccountToken:, manualCookieHeader:)` classifies a token:

- Starts with `sk-ant-oat` → `.oauth(accessToken)`. Route via OAuth path (uses `CODEXBAR_CLAUDE_OAUTH_TOKEN` env override).
- Looks like a `sessionKey` cookie value → `.cookie(sessionKey)`. Route via Web API path.
- Anything else → reject.

This means a user can have **mixed** sessionKey + OAuth-token accounts under Claude, and switching between them transparently flips the data path.

### 8.4 Active-index semantics

- `activeIndex` is clamped to `[0, accounts.count)`.
- Removing the active account: next active is the same index in the new array (or the last one).
- On `addTokenAccount`, new account becomes active.
- For Copilot, adding a token-account clears `apiKey` to avoid two sources of truth.

### 8.5 Windows storage

Token-account tokens are sensitive secrets stored in `config.json`. On Windows, **encrypt the `token` field** at rest:

- Store `{ "token": "dpapi:" + base64(CryptProtectData(plaintext)) }` to differentiate from plain values.
- On read, recognize the `"dpapi:"` prefix and decrypt.
- If `Disable secret storage (debug)` is on, write plaintext (matches macOS behavior of writing plain in config today).

Migration of an existing plaintext config to encrypted form runs once on first launch after upgrade (see §11).

---

## 9. Per-provider token-store catalog

Each `*TokenStore` is a thin Keychain wrapper around a single `<service> + <account>` item. Caches range from “none” to “30-minute TTL in memory.” The functional contract is identical: `load() -> String?`, `store(_:)`, `delete()`. All writes use `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`.

### 9.1 Keychain-backed legacy stores (all migrated to `config.json` today; deprecated)

| Store                         | Service                  | Account                | What it holds                                                | TTL of in-memory cache | Prompt context                |
| ----------------------------- | ------------------------ | ---------------------- | ------------------------------------------------------------ | ---------------------- | ----------------------------- |
| `KeychainCookieHeaderStore`   | `com.steipete.CodexBar`  | `<provider>-cookie`    | Normalized `Cookie:` header for Codex / Claude / Cursor / OpenCode / Factory / Augment / Amp.  | 30 min                 | `.codexCookie` / `.claudeCookie` / `.cursorCookie` / `.opencodeCookie` / `.factoryCookie` / `.augmentCookie` / `.ampCookie` |
| `KeychainZaiTokenStore`       | `com.steipete.CodexBar`  | `zai-api-token`        | Zai API token (`Bearer <token>` for `bigmodel.cn`).          | 30 min                 | `.zaiToken`                   |
| `KeychainSyntheticTokenStore` | `com.steipete.CodexBar`  | `synthetic-api-key`    | Synthetic.new API key.                                       | none                   | `.syntheticToken`             |
| `KeychainCopilotTokenStore`   | `com.steipete.CodexBar`  | `copilot-api-token`    | GitHub PAT (from device flow). Used to mint Copilot tokens.  | none                   | `.copilotToken`               |
| `KeychainKimiTokenStore`      | `com.steipete.CodexBar`  | `kimi-auth-token`      | Kimi (Moonshot) web auth token from cookies.                 | none                   | `.kimiToken`                  |
| `KeychainKimiK2TokenStore`    | `com.steipete.CodexBar`  | `kimi-k2-api-token`    | Kimi K2 API key (separate product surface from Kimi web).    | none                   | `.kimiK2Token`                |
| `KeychainMiniMaxCookieStore`  | `com.steipete.CodexBar`  | `minimax-cookie`       | MiniMax web cookie header (normalized via `MiniMaxCookieHeader`). | none                   | `.minimaxCookie`              |
| `KeychainMiniMaxAPITokenStore`| `com.steipete.CodexBar`  | `minimax-api-token`    | MiniMax developer API token.                                 | none                   | `.minimaxToken`               |

### 9.2 Active cache stores (kept; still important on Windows)

| Store                              | Service                          | Account                                  | What it holds                                                                                                | Lifetime semantics                                                                |
| ---------------------------------- | -------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| `KeychainCacheStore` (`cookie` category) | `com.steipete.codexbar.cache`    | `cookie.<provider>` or `cookie.<provider>.managed.<UUID>` or `cookie.<provider>.managed-store-unreadable` | `CookieHeaderCache.Entry` JSON (header + storedAt + sourceLabel).                                            | Invalidated by auth probe failure or “clear caches.”                              |
| `KeychainCacheStore` (`oauth` category) | `com.steipete.codexbar.cache`    | `oauth.claude`                           | `ClaudeOAuthCredentialsStore.CacheEntry` (raw bytes of `~/.claude/.credentials.json` + `storedAt` + owner).  | Refreshed whenever Claude credentials file changes (`syncWithClaudeKeychainIfChanged`). |

### 9.3 OAuth credential files (no Keychain involvement)

| File                                                  | Owner / Writer | Format       | Permissions | Notes                                                                  |
| ----------------------------------------------------- | -------------- | ------------ | ----------- | ---------------------------------------------------------------------- |
| `~/.claude/.credentials.json`                         | Claude CLI     | JSON         | 0600        | Mirrors Keychain `Claude Code-credentials`.                            |
| `~/.codex/auth.json`                                  | Codex CLI      | JSON         | 0600        | Tokens + `last_refresh`. May contain `OPENAI_API_KEY` for legacy installs. |
| `~/.codexbar/antigravity/oauth_creds.json`            | CodexBar       | JSON         | 0600        | Google OAuth tokens for Antigravity provider.                          |
| `~/.config/gcloud/application_default_credentials.json` | gcloud CLI   | JSON         | varies      | Read-only by us.                                                       |

### 9.4 Windows mapping

| macOS surface                                       | Windows replacement                                                                                                       |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| All `com.steipete.CodexBar/<account>` items          | **Deprecated.** Already migrated to `config.json`. On Windows we never write these.                                       |
| `com.steipete.codexbar.cache/cookie.*`              | `%LOCALAPPDATA%\CodexBar\cookie-cache\cookie\<identifier>.bin` (DPAPI-encrypted JSON).                                    |
| `com.steipete.codexbar.cache/oauth.claude`          | `%LOCALAPPDATA%\CodexBar\oauth-cache\claude.bin` (DPAPI-encrypted JSON wrapping the source `.credentials.json` bytes).    |
| OAuth refresh tokens (Antigravity, future providers we own end-to-end) | `%LOCALAPPDATA%\CodexBar\oauth\<provider>.bin` (DPAPI-encrypted) **and** mirrored into Windows Credential Manager under target `CodexBar/<provider>/refresh_token` via `keyring` so external tooling and password-managers can discover them. The DPAPI blob is canonical; Credential Manager is a convenience copy. |
| `~/.codexbar/config.json` `tokenAccounts[].token`   | DPAPI-prefixed string inside the same JSON (`"dpapi:base64..."`). Plaintext under debug toggle.                            |
| `~/.codexbar/config.json` `apiKey`, `cookieHeader`  | Same: DPAPI-wrapped strings inside the JSON.                                                                              |

---

## 10. `~/.codexbar/config.json` — schema, sensitivity, permissions

### 10.1 Location

- macOS / Linux: `~/.codexbar/config.json`.
- Windows: `%APPDATA%\CodexBar\config.json`.
- Created on first launch if missing.

### 10.2 Schema (current, version 1)

```json
{
  "version": 1,
  "providers": [
    {
      "id": "codex",
      "enabled": true,
      "source": "auto",
      "cookieSource": "auto",
      "cookieHeader": null,
      "apiKey": null,
      "region": null,
      "workspaceID": null,
      "tokenAccounts": null
    }
  ]
}
```

### 10.3 Sensitive fields

| Field                               | Sensitivity | Encryption at rest (Windows target)        |
| ----------------------------------- | ----------- | ------------------------------------------ |
| `providers[].cookieHeader`          | High        | DPAPI-wrapped string (`"dpapi:..."`).      |
| `providers[].apiKey`                | High        | DPAPI-wrapped string.                      |
| `providers[].tokenAccounts.accounts[].token` | High | DPAPI-wrapped string.                     |
| `providers[].tokenAccounts.accounts[].label` | Low (may contain email) | Plain. Optionally redacted in logs via `PersonalInfoRedactor` analogue. |
| `providers[].tokenAccounts.accounts[].externalIdentifier` | Low (github login, etc.) | Plain.                       |
| `providers[].region`, `workspaceID` | Low         | Plain.                                     |
| `providers[].source`, `cookieSource`, `enabled` | None | Plain.                                     |

### 10.4 File permissions

- macOS / Linux: `0600` (`NSFileManager.setAttributes`) on every write.
- Windows: NTFS ACL via `SetNamedSecurityInfo` — only the current user has full control; `Administrators` and `SYSTEM` inherit by default (we don’t strip them; that would be hostile to system imaging / corp policy). DPAPI on the sensitive fields is the actual content protection. Other users on the same box cannot read the file.

### 10.5 Atomic writes

- Always write to `<config.json>.tmp` then rename. Never partial-write.
- File watcher (FSEvents on mac; `ReadDirectoryChangesW` or `notify` crate on Windows) reloads on external edit, validates JSON, and rolls back to last-good in memory if parsing fails.

---

## 11. Migrations (`CodexBarConfigMigrator` + `KeychainMigration`)

### 11.1 `KeychainMigration` (V1) — accessibility migration

- Runs once. Gate: `UserDefaults.bool(forKey: "KeychainMigrationV1Completed")`.
- Scope: legacy `com.steipete.CodexBar/<account>` items.
- Action: read existing item, delete it, re-add with `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`.
- Outcome (mac): zero rebuild prompts. (Windows equivalent: not applicable — DPAPI has no accessibility concept.)

### 11.2 `CodexBarConfigMigrator` — Keychain → config.json migration

- Gate: `UserDefaults.bool(forKey: "codexbar.legacySecretsMigrationCompleted")`.
- Phases:
  1. **Cookie sources from `UserDefaults`**: legacy keys like `claudeCookieSource`, `codexCookieSource`, `openAIWebAccessEnabled` are folded into `providers[].cookieSource`. Always runs (cheap).
  2. **Legacy provider order/toggles**: read from `UserDefaults` keys `providerOrder`, `providerToggles`. Only applied when there is **no** existing config (first launch after upgrade).
  3. **Secrets**: read every legacy Keychain item; if non-empty, copy into `config.json` (`apiKey` / `cookieHeader`). `setIfEmpty` semantics: never overwrite a config value already present.
  4. **Token accounts**: read `token-accounts.json` (legacy) and fold into `providers[].tokenAccounts`.
- After phase 3 succeeds, **delete legacy Keychain items** and **set the completion flag**.
- Partial-state safety: the completion flag is set *only* after `clearLegacyStores` succeeds. If migration writes config but crashes before clearing Keychain, the next launch sees `existing != nil` and skips phase 2 but still runs phase 3 — which is idempotent (already-empty legacy items contribute nothing).

### 11.3 Windows-specific migration (new)

On first launch we will additionally run, gated by `codexbar.windowsSecretEncryptionV1Completed`:

1. Read `config.json`.
2. For each sensitive field (table in §10.3), if the value is non-empty and **not** already DPAPI-prefixed, wrap it with `CryptProtectData` and write back.
3. Set the gate flag.

This is reversible from the debug toggle: turning on “Disable secret storage” simply stops wrapping on next write; existing wrapped values are still decrypted on read until cleared.

---

## 12. PathEnvironment — full list of paths

(Cross-reference: `docs/windows/01-mac-platform-dependencies.md` §11.)

| macOS path                                                                 | Purpose                                  | Windows target                                                                                  |
| -------------------------------------------------------------------------- | ---------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `~/.codexbar/config.json`                                                  | Main config + manual tokens              | `%APPDATA%\CodexBar\config.json`                                                                |
| `~/.codexbar/antigravity/oauth_creds.json`                                 | Antigravity OAuth                        | `%APPDATA%\CodexBar\antigravity\oauth_creds.json`                                               |
| `~/Library/Application Support/CodexBar/token-accounts.json` (legacy)      | Pre-config-migration token accounts      | `%APPDATA%\CodexBar\token-accounts.json` (legacy only; migration target)                        |
| `~/Library/Application Support/CodexBar/<provider>-cookie.json` (legacy)   | Pre-keychain cookie cache                | n/a (clean install)                                                                             |
| `~/Library/Caches/CodexBar/cost-usage/*.json`                              | Cost usage scan cache                    | `%LOCALAPPDATA%\CodexBar\cache\cost-usage\*.json`                                               |
| `~/.claude/.credentials.json`                                              | Claude OAuth fallback                    | `%USERPROFILE%\.claude\.credentials.json` (Claude CLI on Windows uses the same path)            |
| `~/.codex/auth.json` (overridable via `$CODEX_HOME`)                       | Codex OAuth                              | `%USERPROFILE%\.codex\auth.json`                                                                |
| `$CLAUDE_CONFIG_DIR/projects/**/*.jsonl`                                   | Claude session logs (cost)               | Same env var on Windows                                                                          |
| `~/.config/claude/projects/**/*.jsonl`                                     | Claude legacy logs                       | `%USERPROFILE%\.config\claude\projects\**\*.jsonl`                                              |
| `~/.claude/projects/**/*.jsonl`                                            | Claude native logs                       | `%USERPROFILE%\.claude\projects\**\*.jsonl`                                                     |
| `~/.pi/agent/sessions/**/*.jsonl`                                          | pi sessions                              | `%USERPROFILE%\.pi\agent\sessions\**\*.jsonl`                                                   |
| `~/Library/Cookies/Cookies.binarycookies`                                  | Safari cookies                           | n/a — drop                                                                                       |
| `~/Library/Application Support/Google/Chrome/<profile>/Cookies` or `Network/Cookies` | Chrome cookies                  | `%LOCALAPPDATA%\Google\Chrome\User Data\<profile>\Network\Cookies`                              |
| `~/Library/Application Support/Google/Chrome/Local State`                  | Chrome key blob (mac: not used — Keychain is canonical) | `%LOCALAPPDATA%\Google\Chrome\User Data\Local State` (required on Windows)               |
| `~/Library/Application Support/BraveSoftware/Brave-Browser/...`            | Brave cookies                            | `%LOCALAPPDATA%\BraveSoftware\Brave-Browser\User Data\<profile>\Network\Cookies` + `Local State` |
| `~/Library/Application Support/Microsoft Edge/...`                         | Edge cookies                             | `%LOCALAPPDATA%\Microsoft\Edge\User Data\<profile>\Network\Cookies` + `Local State`             |
| `~/Library/Application Support/Vivaldi/...`                                | Vivaldi cookies                          | `%LOCALAPPDATA%\Vivaldi\User Data\<profile>\Network\Cookies` + `Local State`                    |
| `~/Library/Application Support/Firefox/Profiles/*/cookies.sqlite`          | Firefox cookies                          | `%APPDATA%\Mozilla\Firefox\Profiles\*\cookies.sqlite`                                           |
| `~/Library/Application Support/zen/Profiles/.../cookies.sqlite`            | Zen browser                              | `%APPDATA%\zen\Profiles\*\cookies.sqlite`                                                       |
| `/Applications/<App>.app`, `~/Applications/<App>.app`                      | Browser app detection                    | Read installed-browsers from `HKLM\Software\Clients\StartMenuInternet` + check binary paths.    |

Path resolution is centralized in a `Paths` Rust module:

```rust
pub struct Paths {
    pub config_dir: PathBuf,           // %APPDATA%\CodexBar
    pub cache_dir: PathBuf,            // %LOCALAPPDATA%\CodexBar\cache
    pub cookie_cache_dir: PathBuf,     // %LOCALAPPDATA%\CodexBar\cookie-cache
    pub oauth_cache_dir: PathBuf,      // %LOCALAPPDATA%\CodexBar\oauth-cache
    pub claude_home: PathBuf,          // %USERPROFILE%\.claude
    pub codex_home: PathBuf,           // $CODEX_HOME or %USERPROFILE%\.codex
}
```

All path constants live in one place. Never sprinkle `dirs::config_dir()` calls across modules.

---

## 13. Mac → Windows mapping (the meat)

This is the single most important table in this document.

### 13.1 Secret storage

| macOS surface                                                                                 | Windows replacement                                                                                                                                                                                                                                                                                       |
| --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SecItemAdd`/`SecItemCopyMatching` GenericPassword for cached blobs (cookie + OAuth)          | **DPAPI** via `windows` crate (`CryptProtectData`/`CryptUnprotectData`) with user scope, no entropy, `CRYPTPROTECT_UI_FORBIDDEN`. Files in `%LOCALAPPDATA%\CodexBar\cookie-cache\` and `%LOCALAPPDATA%\CodexBar\oauth-cache\`.                                                                              |
| `Claude Code-credentials` Keychain (read by us, written by Claude CLI)                        | Same — Claude CLI on Windows writes `%USERPROFILE%\.claude\.credentials.json`. We **only read the file**; there is no Windows-specific Claude Keychain. The “Claude Keychain” code path is removed entirely.                                                                                                |
| Chrome `Safe Storage` Keychain key                                                            | `Local State` `os_crypt.encrypted_key` + DPAPI unwrap (see §4.5.1).                                                                                                                                                                                                                                       |
| Anti-prompt UI (`KeychainNoUIQuery`, `KeychainAccessPreflight`, `KeychainPromptHandler`)      | **Removed.** Drop all this. DPAPI doesn’t prompt; cookie decryption uses keys we already have, doesn’t prompt; v20 has no prompt either — it just fails.                                                                                                                                                  |
| `~/.codexbar/config.json` with `0600`                                                         | `%APPDATA%\CodexBar\config.json` with NTFS user ACL **and** DPAPI-wrapped sensitive fields. (Belt and braces because Windows lets users back up `%APPDATA%`.)                                                                                                                                                |
| `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`                                            | DPAPI user-scope semantics naturally satisfy “this device, this user only.” No equivalent attribute needed.                                                                                                                                                                                                |
| `KeychainAccessGate.isDisabled` (`debugDisableKeychainAccess`)                                | `DisableSecretStorageDebug` setting (DEBUG-only). When on: write plaintext config, skip DPAPI on read.                                                                                                                                                                                                     |

### 13.2 Long-lived OAuth refresh tokens — store both ways

For Antigravity and any future provider we own end-to-end:

| Location                                                              | Role                                                                                                  |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Windows Credential Manager target `CodexBar/<provider>/refresh_token` | **Discoverability + interop.** Lets password managers and IT tooling enumerate, revoke, or migrate.   |
| `%LOCALAPPDATA%\CodexBar\oauth\<provider>.bin` (DPAPI blob)           | **Canonical.** Always-consistent local copy; survives if Credential Manager is misconfigured.         |

Use the `keyring` crate (3.x) to write the Credential Manager entry. On read, prefer the file (canonical), fall back to Credential Manager only if the file is missing.

### 13.3 What we **drop entirely** on Windows

- Safari binarycookies path.
- Chrome / Chromium Safe Storage Keychain reads.
- `KeychainNoUIQuery`, `KeychainAccessPreflight`, `KeychainPromptCoordinator`, `KeychainMigration` (the V1 accessibility one).
- The three-mode `ClaudeOAuthKeychainPromptMode` UI control (replace with a single info row in settings: “Claude credentials are read from `%USERPROFILE%\.claude\.credentials.json`. No prompt is required.”).
- The `/usr/bin/security` CLI fallback for Claude.
- `BrowserCookieKeychainPromptHandler` (no per-domain prompt to coordinate).

### 13.4 What we **must add** on Windows

- Chromium V10 decryptor (AES-256-GCM with 12-byte nonce, DPAPI-unwrapped key).
- V20 detection + per-browser cooldown + manual-paste fallback UX.
- DPAPI utility crate (wrapping `CryptProtectData` / `CryptUnprotectData`, error mapping, automatic re-throw if `Disable secret storage` is on).
- File-level migration runner for `windowsSecretEncryptionV1`.

---

## 14. Logging discipline

### 14.1 Universal rules

Never log:

- Raw tokens, refresh tokens, access tokens, API keys.
- Raw cookie values (anything between `=` and `;`).
- Full request bodies for auth endpoints.
- Email addresses (use `PersonalInfoRedactor` analogue).
- Account labels that look like emails (regex: `[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}` → `Hidden`).

Allowed to log:

- Token *presence* booleans (`hasRefreshToken=true`).
- Token *expiry seconds* (`expiresInSec=1234`).
- Scopes (the list of scope names, e.g. `["user:profile", "user:inference"]`).
- Provider IDs, source labels, sizes (`payload_bytes=512`).
- Error type names and OSStatus / Win32 error codes.

### 14.2 macOS implementation today

- `CodexBarLog.logger(LogCategories.<category>)` wraps `os.Logger`.
- Categories used in this subsystem: `keychain-cache`, `keychain-migration`, `keychain-preflight`, `keychain-prompt`, `cookie-cache`, `cookie-header-store`, `browser-cookie-gate`, `claude-usage`, `zai-token-store`, etc.
- `PersonalInfoRedactor.redactEmails(in: text, isEnabled: settings.hideEmails)` is applied to user-visible UI strings derived from probes.

### 14.3 Windows equivalent

| Need                              | Windows implementation                                                                                                                                                                                                            |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Structured logger                 | `tracing` + `tracing-subscriber` with JSON formatter for file output, pretty formatter for stderr in DEBUG.                                                                                                                       |
| Log file location                 | `%LOCALAPPDATA%\CodexBar\logs\codexbar.log` rotated daily (use `tracing-appender`).                                                                                                                                                |
| ETW (Event Tracing for Windows)   | Optional later. Not required for v1.                                                                                                                                                                                              |
| Personal-info redaction           | `Redactor::email(text)` Rust helper, mirrors `PersonalInfoRedactor.redactEmails`. Applied at the log-event boundary via a `tracing::Layer`.                                                                                       |
| Token redaction                   | A `SensitiveString` newtype that implements `Debug`/`Display` as `"<redacted, N bytes>"`. Tokens, cookies, and headers MUST flow through it. Compile-time errors prevent accidental logging.                                       |
| Settings toggle for redaction     | `settings.hidePersonalInfo` → drives the redaction layer.                                                                                                                                                                          |
| “No raw secrets in crash reports” | Configure the crash reporter (Sentry / Crashpad) to filter the `SensitiveString` type and redact stack-frame argument names matching `*token*`, `*cookie*`, `*key*`, `*secret*`.                                                  |

Add a CI test that runs the binary with debug-level logging against fake providers and asserts the captured log buffer contains **no string longer than 16 characters** matching `[A-Za-z0-9_-]{16,}` (cheap heuristic for token-shaped substrings). Tune the threshold over time.

---

## 15. Acceptance checklist

A reviewer should be able to tick every box before this subsystem is considered “done.”

### 15.1 Functional parity

- [ ] Claude OAuth path works on a Windows box where `claude` CLI has already logged in (reads `%USERPROFILE%\.claude\.credentials.json`).
- [ ] Claude OAuth refresh on `expires_at` in the past: hits `https://platform.claude.com/v1/oauth/token`, persists new tuple to file + DPAPI cache + (optional) Credential Manager.
- [ ] Codex auth flow reads `%USERPROFILE%\.codex\auth.json` (with `$CODEX_HOME` override).
- [ ] GitHub Copilot device flow completes end-to-end; access token persists in `config.json` `apiKey` (DPAPI-wrapped).
- [ ] Antigravity Google OAuth: in-app authorize flow, token persists to `%APPDATA%\CodexBar\antigravity\oauth_creds.json`, refresh on expiry, deleteIfPresent on sign-out.
- [ ] Multi-account: add 2 Claude token accounts (one sessionKey, one `sk-ant-oat`), switch active, both route to correct path.
- [ ] Manual cookie header for Cursor / OpenCode / Factory / Augment / Amp / MiniMax saves in `config.json`, encrypted, and is read on next launch.

### 15.2 Cookie import

- [ ] Chrome stable on Windows: v10 cookies decrypt; sample of 20 cookies from `chatgpt.com` round-trip plaintext.
- [ ] Edge: same.
- [ ] Brave: same.
- [ ] Firefox: cookies SQLite read; locked file handled by temp-copy of `cookies.sqlite` + `cookies.sqlite-wal` + `cookies.sqlite-shm`.
- [ ] Chrome 127+ with App-Bound Encryption: v20 detected, browser cooled down 6 h, user sees a manual-paste prompt; the prompt does not re-fire while cool.
- [ ] Cookie header cache: identical scope-keying as macOS (per provider + per managed-account UUID for Codex).
- [ ] Cookie header normalizer round-trips all forms in the test corpus (bare, with prefix, curl `-H`, `--cookie`, quoted).

### 15.3 Storage hygiene

- [ ] `%APPDATA%\CodexBar\config.json` is created with NTFS ACL restricted to current user.
- [ ] DPAPI-wrapped fields use the `dpapi:` prefix; round-trip read/write works.
- [ ] `Disable secret storage (debug)` toggle: read still works for already-wrapped values; new writes are plaintext; banner appears in UI.
- [ ] All DPAPI calls use user scope and `CRYPTPROTECT_UI_FORBIDDEN` (no UI ever appears from secret storage).
- [ ] Cookie cache file format: DPAPI(JSON) — file is unreadable as plain JSON.
- [ ] Credential Manager mirror for OAuth refresh tokens exists at `CodexBar/<provider>/refresh_token` (when the provider opts in).

### 15.4 Migration

- [ ] First launch on top of a config from macOS (synthetic test fixture): `windowsSecretEncryptionV1` runs, wraps sensitive fields, leaves non-sensitive fields alone.
- [ ] Second launch is idempotent (gate flag set, no-op).
- [ ] Reset gate flag → migration runs again, no double-wrap (recognize `dpapi:` prefix).

### 15.5 Logging

- [ ] `tracing` logs never contain a string matching the secret-shape heuristic (CI test).
- [ ] `SensitiveString` Debug output is `"<redacted, N bytes>"`.
- [ ] Email redaction respects the `hidePersonalInfo` setting; UI strings reflect the toggle live.
- [ ] No raw cookie value, access token, or refresh token appears in `codexbar.log` during a full provider scan with `--verbose`.

### 15.6 Security review

- [ ] No code path writes a secret to `%PUBLIC%`, `%TEMP%` permanently, or `%PROGRAMDATA%`.
- [ ] All temp copies of cookie SQLite are deleted in `Drop` regardless of error path.
- [ ] HTTPS-only for every provider endpoint; no `http://` fallback.
- [ ] No `LocalMachine` DPAPI use anywhere.
- [ ] No “share between users” feature plumbed.
- [ ] Crash reporter scrubs sensitive types.
- [ ] Provider list in `config.json` is reorderable, but the secret content of each entry is untouched by the reorder path (no decrypt-encrypt round-trip on cosmetic edits).

### 15.7 UX polish (Phantom / Duolingo bar)

- [ ] Settings → Providers → \<provider\> → Manual cookie field is a single-line input that accepts pasted curl, copies the *normalized* form back into the field on blur (gives the user feedback).
- [ ] When a v20-only failure happens, the toast deep-links to the manual paste field with the provider preselected.
- [ ] OAuth flows in-app surface the **next** action clearly: device flow shows the user code + a copy button + a “Continue in browser” primary CTA.
- [ ] First-time secret-storage initialization runs in <50 ms on a cold start; visible in startup trace.
- [ ] Empty state for token accounts has an illustration and a single CTA (“Add account”).
- [ ] Errors from DPAPI map to user copy that does not contain raw error codes (`"E_INVALIDARG"` → `"Something’s off with your Windows user profile’s secret storage. Try restarting CodexBar."`).

---

## Appendix A — quick reference for the Rust crate API surface

The shared Rust crate (target: `codexbar-secrets`) exposes:

```rust
pub trait SecretBlobStore: Send + Sync {
    fn read(&self, key: &SecretKey) -> Result<Option<Vec<u8>>>;
    fn write(&self, key: &SecretKey, plaintext: &[u8]) -> Result<()>;
    fn delete(&self, key: &SecretKey) -> Result<()>;
}
pub struct SecretKey { pub category: &'static str, pub identifier: String }

pub trait OAuthRefreshTokenStore: Send + Sync { /* keyring-backed */ }

pub fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>>; // user scope, UI forbidden
pub fn dpapi_unprotect(cipher: &[u8]) -> Result<Vec<u8>>;
```

```rust
pub struct CookieImporter {
    pub browsers: Vec<Browser>,
    pub paths: Paths,
    pub gate: BrowserCookieAccessGate,
    pub cache: CookieHeaderCache,
}
impl CookieImporter {
    pub fn import_for(&self, provider: ProviderId, target: ImportTarget) -> Result<ImportResult>;
}
```

```rust
pub struct CookieHeaderNormalizer;
impl CookieHeaderNormalizer {
    pub fn normalize(raw: &str) -> Option<String>;
    pub fn pairs(raw: &str) -> Vec<(String, String)>;
    pub fn filtered_header(raw: &str, allowed: &HashSet<String>) -> Option<String>;
}
```

```rust
pub struct ChromiumCookieReader<'a> {
    pub browser: Browser,
    pub local_state_path: &'a Path,
    pub cookies_db_path: &'a Path,
}
impl<'a> ChromiumCookieReader<'a> {
    pub fn read_for_domains(&self, domains: &[&str]) -> Result<Vec<HttpCookie>>;
}
// Errors: KeyUnavailable, V20OnlyForDomain, SqliteFailed, DpapiFailed.
```

---

## Appendix B — security-correctness reminders

- **DPAPI calls must pass `CRYPTPROTECT_UI_FORBIDDEN`.** Without it, DPAPI can theoretically surface a credential UI in obscure scenarios (smartcards). We never want UI.
- **Never log `CryptUnprotectData` failure data verbatim** — the failed ciphertext could be a *different* user’s old config or a corrupted secret. Log a hash, never the bytes.
- **`SecretKey::identifier` must be normalised** (lowercased, trimmed) before being used in a filename to avoid Windows-FS-case-folding surprises producing duplicate “different” keys.
- **`reqwest` clients must enable `https-only-mode`** (or the moral equivalent) for token endpoints.
- **The cookie cache file is opaque to JSON parsers** — do not be tempted to gzip or compress: keep one layer (DPAPI), one format (JSON inside), one extension (`.bin`). Future fields are added via JSON keys, not by a second wrapping format.
- **Refresh tokens rotate.** If you observe two consecutive successful refreshes returning the same `refresh_token`, log a warning — that is anomalous and may indicate a replay risk.
- **Migration code paths must never write a secret to a non-final location.** Wrap-in-place, atomic rename, then commit. Do not stage to `Documents`, `Temp`, or any directory not under `%LOCALAPPDATA%` or `%APPDATA%`.
- **The user can paste a cookie containing the literal string `dpapi:`.** Don’t mistake user input for an already-wrapped value. Always wrap user input *before* writing, and validate that read-side detection of the prefix is *whole-string match on the JSON-string value*, not a substring search.

---

End of spec.
