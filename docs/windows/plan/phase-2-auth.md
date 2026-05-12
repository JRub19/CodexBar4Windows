---
phase: 2
title: "Auth, secrets, and cookies subsystem"
status: ready
owner: platform
depends_on:
  - phase-0-bootstrap
  - phase-1-foundations
unblocks:
  - phase-3-tray-popup
  - phase-4-claude-provider
read_when:
  - Implementing DPAPI, Credential Manager, or cookie import on Windows
  - Reviewing the security boundary between the Tauri shell and Rust core
  - Sequencing atomic commits for the secrets crate
sibling_specs:
  - docs/windows/spec/60-auth-cookies-secrets.md
  - docs/windows/01-mac-platform-dependencies.md
  - docs/windows/07-risks-and-open-questions.md
---

# Phase 2: Auth, secrets, and cookies subsystem

## Why this phase exists

Every provider that lands in Phase 4 and later (Claude, Codex, Cursor, Copilot, Gemini, OpenRouter, Factory) needs the same four things: a place to stash an OAuth refresh token, a place to stash a manual API key, a way to obtain a `Cookie:` header for a vendor domain, and a way to redact those values out of every log line. On the macOS source tree those four needs are spread across `KeychainCacheStore`, `KeychainCookieHeaderStore`, the per provider `*TokenStore.swift` files, `BrowserCookieAccessGate`, `CookieHeaderCache`, `CookieHeaderNormalizer`, `PersonalInfoRedactor`, and `ClaudeOAuthCredentials+SecurityCLIReader`. On Windows we collapse that into a single Rust crate, `codexbar-secrets`, that every provider crate consumes through five traits.

Doing this up front, before any provider ships, has three concrete benefits:

1. Providers stay thin. A provider crate is HTTP parsing plus a call to `SecretBlobStore::read` or `CookieImporter::import_for`. No provider re writes DPAPI or SQLite cookie decryption.
2. Security review is bounded. The audit surface for "where do secrets live" is one crate, not seven. Phase 6 security review reads `codexbar-secrets` and the integration tests, then signs off.
3. The v20 fallback path is wired once. Chrome 127 plus App Bound Encryption is the single biggest risk on this surface (R1 in `docs/windows/07-risks-and-open-questions.md`). Building the manual paste flow as part of the platform layer, rather than retrofitting it later, means the providers that need cookies in Phase 5 are not blocked when a developer machine upgrades Chrome.

The phase delivers no user visible features in the popup. It delivers a library and a small set of Tauri IPC commands. The popup in Phase 3 will still run on mock data. Phase 4 is the first time a real secret moves through the new code paths.

## Dependencies (what must already be true)

From Phase 0 (Bootstrap):

- `pnpm` workspace at the repo root, with the Tauri 2 application at `src-tauri/`.
- Rust workspace declared in the root `Cargo.toml` with members under `crates/`.
- CI pipeline running `cargo test`, `cargo clippy`, `cargo fmt --check`, and `pnpm test` on Windows runners.
- Signing configuration scaffolded (the cert itself is not required for this phase; signed builds land in Phase 7).

From Phase 1 (Foundations):

- `crates/codexbar-paths` exports a `Paths` struct with the fields listed in `docs/windows/spec/60-auth-cookies-secrets.md` §12. This phase consumes `Paths`, it does not define it.
- `crates/codexbar-settings` exports a `SettingsStore` with read and write of `config.json` and a file watcher. This phase adds a small number of fields to the settings schema (see deliverable 9 below) but does not change the storage strategy.
- Tauri IPC plumbing is live: a command can be registered in Rust, invoked from React, and round trip a typed payload. The pattern is established in Phase 1 with a `system:get_version` command. Phase 2 follows the same pattern for `secrets:*`.
- `tracing` and `tracing-subscriber` are initialized at app start with a JSON file appender at `%LOCALAPPDATA%\CodexBar4Windows\logs\codexbar.log`. This phase adds a redaction layer to the existing subscriber, it does not stand up logging from scratch.

If any of the above is missing when this phase starts, the first task is to retrofit it into Phase 1 and rebase. Do not paper over a missing foundation by inlining it here.

## Deliverables

The phase produces, in this order:

1. A `codexbar-secrets` Rust crate at `crates/codexbar-secrets/` with the public surface described in `docs/windows/spec/60-auth-cookies-secrets.md` Appendix A.
2. A `codexbar-cookies` Rust crate at `crates/codexbar-cookies/` that depends on `codexbar-secrets` and implements the `BrowserCookieImporter` trait for Chromium browsers (Chrome, Edge, Brave) and Firefox.
3. A `codexbar-redact` Rust crate at `crates/codexbar-redact/` with the `SensitiveString` newtype and the `tracing` redaction layer.
4. Tauri commands registered in `src-tauri/src/commands/secrets.rs` covering: set manual cookie, list token accounts, add token account, edit token account, remove token account, set active token account, import cookies for provider, clear cookie cache.
5. A test crate at `crates/codexbar-secrets/tests/` with round trip and integration tests, plus a CI gate that runs them on Windows runners.
6. A docs update appending a short "secrets crate API" section to `docs/windows/spec/60-auth-cookies-secrets.md` Appendix A if any field drift occurs during implementation. (The spec is the source of truth; the plan does not duplicate it.)

The phase does **not** produce: a popup UI, provider specific code, the Claude OAuth refresh implementation, the Copilot device flow, or the Antigravity Google OAuth path. Those land in Phase 4 and later, consuming the surface this phase builds.

## File layout (target end state)

```
crates/
  codexbar-secrets/
    Cargo.toml
    src/
      lib.rs                # re exports, SecretBlobStore trait, SecretKey
      dpapi.rs              # dpapi_protect, dpapi_unprotect, error mapping
      secure_file.rs        # SecureFile<T: Serialize+Deserialize>
      blob_store.rs         # FileSecretBlobStore impl over SecureFile
      keyring_store.rs      # CredentialManagerOAuthStore via keyring 3.x
      token_account.rs      # TokenAccount, TokenAccountStore (over SettingsStore)
      gate.rs               # CookieAccessGate, with persisted cooldown map
      errors.rs             # SecretsError enum
      migration.rs          # windowsSecretEncryptionV1 runner
    tests/
      dpapi_roundtrip.rs
      secure_file_roundtrip.rs
      token_account_crud.rs
      migration_idempotent.rs
  codexbar-cookies/
    Cargo.toml
    src/
      lib.rs                # BrowserCookieImporter trait, ImportResult, ImportError
      chromium.rs           # ChromiumCookieReader, key derivation, v10/v20 detect
      firefox.rs            # FirefoxCookieReader
      normalizer.rs         # CookieHeaderNormalizer
      header_cache.rs       # CookieHeaderCache (DPAPI wrapped, file backed)
      manual.rs             # ManualCookieSource
      detect.rs             # BrowserDetection (installed browsers, profile paths)
    tests/
      chromium_v10_fixture.rs
      chromium_v20_detect.rs
      firefox_fixture.rs
      normalizer_corpus.rs
      header_cache_roundtrip.rs
  codexbar-redact/
    Cargo.toml
    src/
      lib.rs                # SensitiveString, Redactor, email regex
      tracing_layer.rs      # tracing::Layer that scrubs SensitiveString fields
    tests/
      sensitive_string_display.rs
      tracing_no_leak.rs

src-tauri/
  src/
    commands/
      secrets.rs            # all secrets:* IPC commands
      cookies.rs            # cookie import IPC commands
    main.rs                 # registers the new commands
  Cargo.toml                # adds codexbar-secrets, codexbar-cookies, codexbar-redact

src/
  hooks/
    useSecretsApi.ts        # typed wrappers for the secrets:* IPC commands
  types/
    secrets.ts              # TypeScript types mirroring TokenAccount, etc.
```

The Rust crates are pure library crates; only `src-tauri/` produces a binary. The `crates/codexbar-redact` crate is separate from `codexbar-secrets` because it is consumed by every other crate, including the cookies crate and the future provider crates. Putting it inside `codexbar-secrets` would force a circular intent. Splitting it now costs nothing.

## Public API surface (locked for this phase)

### `codexbar-secrets`

```rust
// blob_store.rs
pub trait SecretBlobStore: Send + Sync {
    fn read(&self, key: &SecretKey) -> Result<Option<Vec<u8>>, SecretsError>;
    fn write(&self, key: &SecretKey, plaintext: &[u8]) -> Result<(), SecretsError>;
    fn delete(&self, key: &SecretKey) -> Result<(), SecretsError>;
}

pub struct SecretKey {
    pub category: &'static str,  // "cookie", "oauth", "token-account"
    pub identifier: String,      // normalized: lowercased, trimmed, NFC
}

pub struct FileSecretBlobStore { /* root: PathBuf */ }
impl SecretBlobStore for FileSecretBlobStore { /* ... */ }

// dpapi.rs
pub fn dpapi_protect(plain: &[u8]) -> Result<Vec<u8>, SecretsError>;
pub fn dpapi_unprotect(cipher: &[u8]) -> Result<Vec<u8>, SecretsError>;
pub const DPAPI_BLOB_PREFIX: &str = "dpapi:v1:";
pub fn wrap_string(plain: &str) -> Result<String, SecretsError>;     // -> "dpapi:v1:<base64>"
pub fn unwrap_string(wrapped: &str) -> Result<String, SecretsError>;

// secure_file.rs
pub struct SecureFile<T: serde::Serialize + serde::de::DeserializeOwned> { /* path: PathBuf */ }
impl<T> SecureFile<T> {
    pub fn new(path: PathBuf) -> Self;
    pub fn load(&self) -> Result<Option<T>, SecretsError>;
    pub fn save(&self, value: &T) -> Result<(), SecretsError>;       // atomic: tmp + rename
    pub fn delete(&self) -> Result<(), SecretsError>;
}

// keyring_store.rs
pub trait OAuthRefreshTokenStore: Send + Sync {
    fn load(&self, provider_id: &str) -> Result<Option<SensitiveString>, SecretsError>;
    fn save(&self, provider_id: &str, token: &SensitiveString) -> Result<(), SecretsError>;
    fn delete(&self, provider_id: &str) -> Result<(), SecretsError>;
}
pub struct CredentialManagerOAuthStore { /* service_prefix: "CodexBar4Windows" */ }
impl OAuthRefreshTokenStore for CredentialManagerOAuthStore { /* keyring crate */ }

// token_account.rs
#[derive(Serialize, Deserialize, Clone)]
pub struct TokenAccount {
    pub id: String,                 // uuid v4
    pub provider_id: String,
    pub kind: TokenAccountKind,     // Cookie | OAuthToken | ApiKey
    pub label: String,
    pub external_identifier: Option<String>,
    #[serde(with = "wrapped_string")]
    pub value: SensitiveString,     // serialized as "dpapi:v1:..." in JSON
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}
pub enum TokenAccountKind { Cookie, OAuthToken, ApiKey }

pub struct TokenAccountStore { /* settings: Arc<SettingsStore> */ }
impl TokenAccountStore {
    pub fn list(&self, provider_id: &str) -> Result<Vec<TokenAccount>, SecretsError>;
    pub fn add(&self, account: TokenAccount) -> Result<TokenAccount, SecretsError>;
    pub fn edit(&self, account_id: &str, patch: TokenAccountPatch) -> Result<TokenAccount, SecretsError>;
    pub fn remove(&self, account_id: &str) -> Result<(), SecretsError>;
    pub fn set_active(&self, provider_id: &str, account_id: &str) -> Result<(), SecretsError>;
    pub fn get_active(&self, provider_id: &str) -> Result<Option<TokenAccount>, SecretsError>;
}

// gate.rs
pub struct CookieAccessGate { /* path: PathBuf, settings: Arc<SettingsStore> */ }
impl CookieAccessGate {
    pub fn is_browser_import_allowed(&self) -> bool;             // settings toggle
    pub fn should_attempt(&self, browser: BrowserId) -> bool;    // toggle && not cooled down
    pub fn record_v20_denial(&self, browser: BrowserId);         // 6 h cooldown
    pub fn clear_cooldown(&self, browser: BrowserId);
}

// errors.rs
#[derive(thiserror::Error, Debug)]
pub enum SecretsError {
    #[error("dpapi: {0}")] Dpapi(String),
    #[error("io: {0}")]    Io(#[from] std::io::Error),
    #[error("serde: {0}")] Serde(#[from] serde_json::Error),
    #[error("not found")]  NotFound,
    #[error("disabled by debug toggle")] Disabled,
    #[error("invalid prefix")]            InvalidPrefix,
    #[error("keyring: {0}")]              Keyring(String),
}
```

### `codexbar-cookies`

```rust
pub trait BrowserCookieImporter: Send + Sync {
    fn browser(&self) -> BrowserId;
    fn read_cookies_for(&self, domains: &[&str]) -> Result<Vec<HttpCookie>, ImportError>;
}

pub enum BrowserId { Chrome, ChromeBeta, ChromeCanary, Edge, EdgeBeta, EdgeCanary, Brave, BraveBeta, Vivaldi, Firefox, Zen }

pub struct HttpCookie {
    pub host: String,
    pub name: String,
    pub value: SensitiveString,
    pub path: String,
    pub expires_utc: Option<chrono::DateTime<chrono::Utc>>,
    pub is_secure: bool,
    pub is_http_only: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum ImportError {
    #[error("browser not installed")]        NotInstalled,
    #[error("cookie database missing")]      DbMissing,
    #[error("cookie database locked, advise user to close {0:?}")] DbLocked(BrowserId),
    #[error("key unwrap failed: {0}")]       KeyUnwrap(String),
    #[error("v20 only for domain {0}")]      V20OnlyForDomain(String),
    #[error("sqlite: {0}")]                  Sqlite(String),
    #[error("dpapi: {0}")]                   Dpapi(String),
    #[error("io: {0}")]                      Io(String),
}

pub struct ChromiumCookieReader { /* local_state, cookies_db, browser */ }
impl BrowserCookieImporter for ChromiumCookieReader { /* ... */ }

pub struct FirefoxCookieReader { /* profile_root */ }
impl BrowserCookieImporter for FirefoxCookieReader { /* ... */ }

pub struct ManualCookieSource { /* provider_id, value */ }

pub struct CookieHeaderNormalizer;
impl CookieHeaderNormalizer {
    pub fn normalize(raw: &str) -> Option<String>;
    pub fn pairs(raw: &str) -> Vec<(String, String)>;
    pub fn filtered_header(raw: &str, allowed: &std::collections::HashSet<String>) -> Option<String>;
}

pub struct CookieHeaderCache { /* path: %LOCALAPPDATA%/CodexBar4Windows/cache/cookie-headers.json */ }
#[derive(Serialize, Deserialize)]
pub struct CookieHeaderCacheEntry {
    pub provider_id: String,
    pub scope: Option<String>,             // managed account UUID for codex
    pub cookie_header: String,             // serialized as wrapped DPAPI inside the JSON file
    pub stored_at: chrono::DateTime<chrono::Utc>,
    pub source_label: String,              // "Chrome / Default", "Firefox", "Manual"
}
impl CookieHeaderCache {
    pub fn load(&self, provider_id: &str, scope: Option<&str>) -> Result<Option<CookieHeaderCacheEntry>, ImportError>;
    pub fn store(&self, entry: CookieHeaderCacheEntry) -> Result<(), ImportError>;
    pub fn clear(&self, provider_id: &str, scope: Option<&str>) -> Result<(), ImportError>;
    pub fn clear_all(&self) -> Result<(), ImportError>;
}

pub struct CookieImporter {
    pub importers: Vec<Box<dyn BrowserCookieImporter>>,
    pub cache: CookieHeaderCache,
    pub gate: CookieAccessGate,
}
impl CookieImporter {
    pub fn import_for(&self, provider_id: &str, target: ImportTarget) -> Result<ImportResult, ImportError>;
}

pub struct ImportTarget {
    pub domains: Vec<String>,
    pub required_cookie_names: Vec<String>,
    pub scope: Option<String>,
}

pub struct ImportResult {
    pub cookie_header: String,
    pub source_label: String,
    pub stored_at: chrono::DateTime<chrono::Utc>,
}
```

### `codexbar-redact`

```rust
#[derive(Clone)]
pub struct SensitiveString(String);
impl SensitiveString {
    pub fn new(s: impl Into<String>) -> Self;
    pub fn expose(&self) -> &str;       // explicit, audited call sites
    pub fn len(&self) -> usize;
}
impl std::fmt::Debug for SensitiveString { /* "<redacted, N bytes>" */ }
impl std::fmt::Display for SensitiveString { /* "<redacted>" */ }
// Serde: serialize the wrapped value through dpapi::wrap_string; deserialize through dpapi::unwrap_string. Module `wrapped_string` exposes the helper for serde with attribute.

pub struct Redactor;
impl Redactor {
    pub fn email(text: &str) -> std::borrow::Cow<'_, str>;
    pub fn token_shaped(text: &str) -> std::borrow::Cow<'_, str>;  // [A-Za-z0-9_-]{20,}
}

pub fn install_redaction_layer(subscriber_builder: tracing_subscriber::layer::Layered<...>) -> /* layered subscriber */ ;
```

### Tauri IPC surface

All commands live under `src-tauri/src/commands/secrets.rs` and `src-tauri/src/commands/cookies.rs`. Naming convention: `secrets:*` and `cookies:*`. Return types are TypeScript safe via `ts-rs`. The exact signatures:

```rust
#[tauri::command]
fn secrets_set_manual_cookie(provider_id: String, raw_cookie: String) -> Result<(), CommandError>;

#[tauri::command]
fn secrets_list_token_accounts(provider_id: String) -> Result<Vec<TokenAccountView>, CommandError>;

#[tauri::command]
fn secrets_add_token_account(input: AddTokenAccountInput) -> Result<TokenAccountView, CommandError>;

#[tauri::command]
fn secrets_edit_token_account(account_id: String, patch: TokenAccountPatch) -> Result<TokenAccountView, CommandError>;

#[tauri::command]
fn secrets_remove_token_account(account_id: String) -> Result<(), CommandError>;

#[tauri::command]
fn secrets_set_active_token_account(provider_id: String, account_id: String) -> Result<(), CommandError>;

#[tauri::command]
fn cookies_import_for_provider(provider_id: String, target: ImportTargetInput) -> Result<ImportResultView, CommandError>;

#[tauri::command]
fn cookies_clear_cache(provider_id: Option<String>) -> Result<(), CommandError>;
```

`TokenAccountView` strips the `value` field. The raw secret never leaves the Rust side. Edit operations take a `value: Option<String>` in the patch, which when present is wrapped and stored. The renderer never reads back a stored secret.

## Atomic commit tasks

Each task below is a single conventional commit that leaves the tree green: `cargo test` passes, `cargo clippy --workspace` passes, `cargo fmt --check` passes, `pnpm test` passes. Push after every commit. If CI on the pushed commit goes red, the next commit fixes it; do not amend.

The tasks are ordered by dependency. Tasks 2.1 through 2.5 build the DPAPI and file primitives. Tasks 2.6 through 2.10 build the cookie subsystem. Tasks 2.11 through 2.14 wire the Tauri IPC. Tasks 2.15 through 2.19 are tests, migration, and polish.

### 2.1 Scaffold `codexbar-redact` crate

Files: `crates/codexbar-redact/Cargo.toml`, `crates/codexbar-redact/src/lib.rs`, `crates/codexbar-redact/src/tracing_layer.rs`, `Cargo.toml` (workspace member add).

What lands: `SensitiveString` with `Debug` and `Display` returning `<redacted>`. A `Redactor::email` helper using the regex from `PersonalInfoRedactor`. The `tracing::Layer` is a stub that compiles; the real scrubbing logic lands in 2.18 with its test.

Acceptance: `cargo test -p codexbar-redact` runs and passes a trivial test asserting `format!("{}", SensitiveString::new("abc")) == "<redacted>"` and `format!("{:?}", SensitiveString::new("abc")) == "<redacted, 3 bytes>"`.

Draft commit:
```
feat(redact): add SensitiveString newtype and email redactor
```

### 2.2 Scaffold `codexbar-secrets` crate with error type and `Paths` consumption

Files: `crates/codexbar-secrets/Cargo.toml`, `crates/codexbar-secrets/src/lib.rs`, `crates/codexbar-secrets/src/errors.rs`, `Cargo.toml` (workspace member add).

What lands: `SecretsError` enum, public re exports, no DPAPI yet. Crate depends on `codexbar-paths` and `codexbar-redact`.

Acceptance: `cargo build -p codexbar-secrets` compiles. `cargo test -p codexbar-secrets` runs (zero tests is fine).

Draft commit:
```
chore(secrets): scaffold codexbar-secrets crate
```

### 2.3 Implement DPAPI wrapper

Files: `crates/codexbar-secrets/src/dpapi.rs`, `crates/codexbar-secrets/src/lib.rs` (re export).

What lands: `dpapi_protect`, `dpapi_unprotect` calling `CryptProtectData` and `CryptUnprotectData` from the `windows` crate 0.61, user scope, `CRYPTPROTECT_UI_FORBIDDEN` flag. `wrap_string` and `unwrap_string` with the `dpapi:v1:<base64>` versioned envelope. `base64` via the `base64` crate, no padding. Error mapping: any non zero status from the Win32 call lands in `SecretsError::Dpapi` with an integer code, never the raw bytes.

Acceptance: `cargo test -p codexbar-secrets dpapi_roundtrip` passes. Test inputs include the empty slice, a 1 KiB random blob, and a string containing the literal `dpapi:v1:` substring (must round trip cleanly).

Draft commit:
```
feat(secrets): wrap DPAPI via CryptProtectData with versioned envelope
```

### 2.4 Implement `SecureFile<T>`

Files: `crates/codexbar-secrets/src/secure_file.rs`, `crates/codexbar-secrets/src/lib.rs`.

What lands: A struct holding a `PathBuf`. `save` serializes `T` to JSON, then writes the JSON bytes to `<path>.tmp` (not DPAPI wrapped at the file level; the *fields* inside `T` decide what is wrapped via `SensitiveString` serde). Renames `<path>.tmp` to `<path>` atomically. `load` returns `Ok(None)` if missing, `Ok(Some(T))` if parseable, `Err` if corrupt. `delete` removes the file if present, returns `Ok(())` if not.

Acceptance: `cargo test -p codexbar-secrets secure_file_roundtrip` writes a struct with one plain field and one `SensitiveString` field, reloads, asserts equality. A second test deletes the file and reloads, expecting `None`.

Draft commit:
```
feat(secrets): add SecureFile with atomic write and JSON roundtrip
```

### 2.5 Implement `FileSecretBlobStore` and `CredentialManagerOAuthStore`

Files: `crates/codexbar-secrets/src/blob_store.rs`, `crates/codexbar-secrets/src/keyring_store.rs`.

What lands: `FileSecretBlobStore` writes one DPAPI wrapped blob per `SecretKey` to `<root>/<category>/<normalized_identifier>.bin`. Identifier normalization: NFC, lowercase, trim, then replace any character not in `[a-z0-9._-]` with `_`. The `CredentialManagerOAuthStore` uses `keyring` 3.x with service `CodexBar4Windows`, target user the OS user, and a per provider entry name `oauth/<provider_id>/refresh_token`. The Credential Manager copy is convenience only; the canonical store is the DPAPI file blob.

Acceptance: Tests in `tests/blob_store_roundtrip.rs` write and read three categories. Tests in `tests/keyring_roundtrip.rs` are `#[cfg(windows)]` and skipped on other platforms; they write, read, delete via `keyring` and assert success.

Draft commit:
```
feat(secrets): add file and Credential Manager blob stores
```

### 2.6 Scaffold `codexbar-cookies` crate, trait, and detect module

Files: `crates/codexbar-cookies/Cargo.toml`, `crates/codexbar-cookies/src/lib.rs`, `crates/codexbar-cookies/src/detect.rs`.

What lands: `BrowserCookieImporter` trait, `HttpCookie`, `BrowserId`, `ImportError`. `BrowserDetection` reads `HKLM\Software\Clients\StartMenuInternet` and probes the canonical install paths in `docs/windows/spec/60-auth-cookies-secrets.md` §12 for each browser. Returns a `BrowserPresence` struct with `local_state_path`, `profile_root`, and `cookie_db_path` per installed browser.

Acceptance: `cargo test -p codexbar-cookies detect_smoke` runs `BrowserDetection::probe_all()` and asserts the call does not error; results vary per CI runner.

Draft commit:
```
feat(cookies): add BrowserCookieImporter trait and detection
```

### 2.7 Implement `CookieHeaderNormalizer`

Files: `crates/codexbar-cookies/src/normalizer.rs`.

What lands: A port of the Swift `CookieHeaderNormalizer`. Accepts bare headers, `Cookie:` prefixed, curl `-H` and `--cookie` and `-b` forms, single or double quoted. Outputs `name=value; name=value`. Exposes `pairs(raw)` returning `Vec<(String, String)>` and `filtered_header(raw, allowed)`.

Acceptance: `tests/normalizer_corpus.rs` covers all forms in the spec table §4.9 plus a real curl line with leading whitespace and one with embedded `=` in the value.

Draft commit:
```
feat(cookies): port CookieHeaderNormalizer with curl forms
```

### 2.8 Implement `ChromiumCookieReader` for v10

Files: `crates/codexbar-cookies/src/chromium.rs`.

What lands: Parse `Local State` JSON, extract `os_crypt.encrypted_key`, base64 decode, strip the 5 byte `DPAPI` prefix, call `dpapi_unprotect` to get the 32 byte AES 256 key. Copy `Network/Cookies` (and `-wal`, `-shm`) to a `tempfile::TempDir`. Open SQLite read only via `rusqlite` 0.32 with `OpenFlags::SQLITE_OPEN_READ_ONLY`. For each row in `cookies` matching the requested domains, inspect `encrypted_value`: if it starts with `v10`, slice the next 12 bytes as the AES GCM nonce, decrypt the remainder (ciphertext plus 16 byte tag) using `aes-gcm` crate. If it starts with `v20`, do not decrypt; surface `ImportError::V20OnlyForDomain(host)` after counting whether any v10 fallback exists for the same domain. If `encrypted_value` is empty, fall back to the plaintext `value` column.

Browser DB lock case: if `rusqlite::Error::SqliteFailure` has code `SQLITE_BUSY` or `SQLITE_LOCKED`, return `ImportError::DbLocked(self.browser)`. The renderer surfaces a "close Chrome and try again" toast.

Acceptance: `tests/chromium_v10_fixture.rs` uses a checked in test fixture under `crates/codexbar-cookies/tests/fixtures/chromium_v10/` containing a synthetic `Local State` and a synthetic SQLite database produced by a `build.rs` script (because real Chromium DBs vary). The fixture encrypts a known cookie with a known DPAPI key wrapped key; the test asserts round trip. A second test, `chromium_v20_detect`, feeds a v20 prefixed blob and asserts the right `ImportError::V20OnlyForDomain` is returned.

Draft commit:
```
feat(cookies): implement Chromium v10 cookie reader with v20 detection
```

### 2.9 Implement `FirefoxCookieReader`

Files: `crates/codexbar-cookies/src/firefox.rs`.

What lands: Enumerate `%APPDATA%\Mozilla\Firefox\Profiles\*.default*\cookies.sqlite`. Copy to temp with `-wal` and `-shm`. Open read only. Query `SELECT host, name, value, path, expiry, isSecure, isHttpOnly FROM moz_cookies WHERE host LIKE ?` per requested domain. Convert `expiry` (seconds) to `DateTime<Utc>`. Profile path is URL encoded by `dirs::data_dir` on Windows; the implementation calls `Path::join` and never string concatenates.

Acceptance: `tests/firefox_fixture.rs` uses a checked in synthetic `cookies.sqlite` containing two rows for two hosts; the test asserts both are read and that the path with URL encoded characters resolves.

Draft commit:
```
feat(cookies): add Firefox cookies.sqlite reader
```

### 2.10 Implement `CookieHeaderCache`

Files: `crates/codexbar-cookies/src/header_cache.rs`.

What lands: A single JSON file at `%LOCALAPPDATA%\CodexBar4Windows\cache\cookie-headers.json`. The file is itself DPAPI wrapped via `SecureFile<CookieHeaderCacheData>`, where `CookieHeaderCacheData` is `{ "version": 1, "entries": Vec<CookieHeaderCacheEntry> }`. Each entry is provider tagged, optionally scope tagged, and carries a `stored_at`. The cache has no global TTL but every read returns the entry plus an `age` field that the caller may use to decide to refresh. On `load` for a missing entry, returns `Ok(None)`. On a corrupt blob, deletes the file and returns `Ok(None)` (and logs a warning, never the bytes).

Acceptance: `tests/header_cache_roundtrip.rs` writes two entries (one with scope, one without), reloads, asserts both round trip. A second test corrupts the file by writing one byte, then asserts that the next load returns `None` and that the file no longer exists.

Draft commit:
```
feat(cookies): persist cookie headers to DPAPI wrapped cache file
```

### 2.11 Implement `TokenAccountStore`

Files: `crates/codexbar-secrets/src/token_account.rs`.

What lands: `TokenAccount` struct with the fields in the API surface section. `TokenAccountStore` reads and writes `tokenAccounts` inside `config.json` via the `SettingsStore` from Phase 1. The `value` field uses a custom serde module `wrapped_string` that serializes through `dpapi::wrap_string` and deserializes through `dpapi::unwrap_string`. New IDs are `uuid::Uuid::new_v4()`. The store updates `last_used` only on `set_active`. Active index semantics match the macOS Swift code in `TokenAccountsRouter.swift`: clamp to `[0, len)`, on removal of the active account, the next active is the same index in the new array or the last one.

Acceptance: `tests/token_account_crud.rs` exercises add, edit, remove, set_active, list. A separate test asserts that a `TokenAccount` round tripped through JSON has its `value` stored as `dpapi:v1:<base64>` in the on disk representation.

Draft commit:
```
feat(secrets): add TokenAccountStore over config.json
```

### 2.12 Implement `CookieAccessGate` with persisted cooldown

Files: `crates/codexbar-secrets/src/gate.rs`.

What lands: `CookieAccessGate` reads `settings.allow_browser_cookie_import` (defaults to `true`, exposed in the advanced settings pane). Persisted cooldown map at `%LOCALAPPDATA%\CodexBar4Windows\cache\cookie-gate.json`, mapping `BrowserId` to `denied_until: DateTime<Utc>`. `record_v20_denial` writes `now + 6h`. `should_attempt` returns false when the toggle is off or when the cooldown is hot. `clear_cooldown` removes the entry. The file is not DPAPI wrapped because it contains no secret content, only timestamps.

Acceptance: `tests/gate_cooldown.rs` records a denial, asserts `should_attempt` returns false within the window, asserts true after manipulating the persisted timestamp to a past value.

Draft commit:
```
feat(secrets): add CookieAccessGate with persisted v20 cooldown
```

### 2.13 Implement `CookieImporter` orchestrator

Files: `crates/codexbar-cookies/src/lib.rs` (extend), `crates/codexbar-cookies/src/manual.rs`.

What lands: `CookieImporter::import_for(provider_id, target)` flow:

1. Read `CookieHeaderCache`. If a fresh entry exists and was not invalidated, return it.
2. Look up the active `TokenAccount` for the provider. If kind is `Cookie`, materialize it via `ManualCookieSource` and store back to the cache with source label `"Manual"`.
3. Otherwise, for each `BrowserCookieImporter` in order (Chrome, Edge, Brave, Firefox), check `gate.should_attempt`. Call `read_cookies_for(target.domains)`. On `V20OnlyForDomain`, call `gate.record_v20_denial(browser)` and continue to the next importer.
4. If all importers fail, return the structured error so the renderer can surface the manual paste fallback.

`ManualCookieSource::materialize` takes a raw string, runs it through `CookieHeaderNormalizer`, errors if the normalized output is empty, and wraps it in a `CookieHeaderCacheEntry`.

Acceptance: `tests/importer_orchestration.rs` mocks two `BrowserCookieImporter` impls (one returning `V20OnlyForDomain`, one returning a valid cookie set) and asserts the orchestrator records the cooldown and returns the second importer's result. A second test sets a manual cookie and asserts the importer returns it without invoking any browser.

Draft commit:
```
feat(cookies): orchestrate cache, manual, and browser sources
```

### 2.14 Register Tauri IPC commands

Files: `src-tauri/src/commands/secrets.rs`, `src-tauri/src/commands/cookies.rs`, `src-tauri/src/main.rs`, `src-tauri/Cargo.toml`, `src/hooks/useSecretsApi.ts`, `src/types/secrets.ts`.

What lands: All commands from the IPC surface section above. Each command wraps its body in `tracing::instrument(skip(...))` with sensitive arguments listed in `skip`. `CommandError` is a thin wrapper that maps `SecretsError` and `ImportError` to a serializable shape with a code, a human readable message, and a structured payload (`{ "kind": "v20_only", "browser": "chrome", "domain": "claude.ai" }` for v20 denials, so the renderer can deep link to the manual paste field). `ts-rs` emits the matching TypeScript types into `src/types/secrets.ts`. `useSecretsApi.ts` exports React hooks: `useTokenAccounts`, `useSetManualCookie`, `useImportCookies`. No UI ships yet; these hooks are consumed in Phase 3 and later.

Acceptance: `cargo test -p src-tauri ipc_smoke` invokes each command via `tauri::test::mock_invoke_handler` with valid payloads and asserts non error returns. The TypeScript types compile under `pnpm typecheck`.

Draft commit:
```
feat(ipc): register secrets and cookies Tauri commands
```

### 2.15 Wire `windowsSecretEncryptionV1` migration

Files: `crates/codexbar-secrets/src/migration.rs`, `src-tauri/src/main.rs` (call site at startup).

What lands: A migration runner gated by `settings.flags.windows_secret_encryption_v1_completed`. On first run, reads `config.json`, walks `providers[*].cookieHeader`, `providers[*].apiKey`, and `providers[*].tokenAccounts.accounts[*].value`. For each non empty string that does not start with `dpapi:v1:`, wraps it via `dpapi::wrap_string` and rewrites the field. Sets the flag on success. Idempotent: a second run is a no op. Reversible only via the debug toggle.

Acceptance: `tests/migration_idempotent.rs` seeds a synthetic `config.json` with three plain secret fields, runs the migration, asserts all three have the `dpapi:v1:` prefix on disk. Runs the migration again, asserts the file content byte equals the post first run content.

Draft commit:
```
feat(secrets): run windowsSecretEncryptionV1 migration on startup
```

### 2.16 Add `CookieAccessGate` advanced settings toggle

Files: `crates/codexbar-settings/src/schema.rs` (add field), `src/components/settings/AdvancedPane.tsx` (add toggle), `src/types/settings.ts` (regenerate).

What lands: The settings schema gains `allow_browser_cookie_import: bool` defaulting to `true`. The advanced settings pane in the React UI exposes a single checkbox: "Allow CodexBar4Windows to read cookies from installed browsers (Chrome, Edge, Brave, Firefox)." Off disables Chromium and Firefox importers via the gate. The label and help text live in the en US locale file; localization syncs in Phase 9.

Acceptance: The toggle persists across an app restart. Manual integration check on a dev machine: with the toggle off, `cookies:import_for_provider` for a fake provider returns the "browser import disabled by user" structured error.

Draft commit:
```
feat(settings): add allow-browser-cookie-import advanced toggle
```

### 2.17 Add `Disable secret storage (debug)` toggle

Files: `crates/codexbar-settings/src/schema.rs`, `crates/codexbar-secrets/src/dpapi.rs`, `src/components/settings/AdvancedPane.tsx`.

What lands: A debug only setting `disable_secret_storage` that, when enabled in a DEBUG build, causes `wrap_string` to write the plaintext value and `unwrap_string` to detect both wrapped and plain forms. Release builds refuse to honor the flag and log a startup warning if it is on. A banner appears in the popup chrome when the flag is honored. The Phase 3 popup will render this banner; Phase 2 only sets up the settings field and the conditional code path.

Acceptance: `tests/disable_secret_storage.rs` builds with `#[cfg(debug_assertions)]`, flips the flag, asserts a string round trips without the `dpapi:v1:` prefix on disk. With the flag on, a previously wrapped value still decrypts on read.

Draft commit:
```
feat(secrets): honor disable-secret-storage debug toggle
```

### 2.18 Implement and test the `tracing` redaction layer

Files: `crates/codexbar-redact/src/tracing_layer.rs`, `crates/codexbar-redact/tests/tracing_no_leak.rs`.

What lands: A `tracing::Layer` that intercepts field recording. For any field whose recorded `&dyn Debug` value is a `SensitiveString`, substitute `<redacted>`. Additionally, a post format pass scrubs the formatted event string with `Redactor::email` and `Redactor::token_shaped` when `settings.hide_personal_info` is true (read once at subscriber install time; live toggling lands in Phase 9).

Acceptance: `tests/tracing_no_leak.rs` installs a `tracing_subscriber::fmt` with the redaction layer writing to a buffer. Emits an `info!` event with a `SensitiveString` field and a literal email and a literal 32 char token. Asserts the buffer contains `<redacted>`, contains `Hidden` for the email, and contains no substring matching `[A-Za-z0-9_-]{20,}` other than known UUIDs.

Draft commit:
```
feat(redact): scrub SensitiveString and token shaped substrings in tracing
```

### 2.19 Add CI gate for secret leakage heuristic

Files: `.github/workflows/ci.yml`, `crates/codexbar-redact/tests/no_secret_leak_in_log.rs`.

What lands: A CI job that runs `cargo test -p codexbar-redact no_secret_leak_in_log -- --include-ignored`. The test boots the app's tracing subscriber, runs a scripted sequence of operations (write a token account, import a fake cookie, refresh a fake OAuth token), captures the log buffer, and fails if any line contains a substring matching the secret shape heuristic `[A-Za-z0-9_-]{20,}` that is not in a known allowlist (request IDs, UUIDs in field names). The workflow uploads the captured log as an artifact on failure.

Acceptance: The job is green on a clean run and red when 2.18 is reverted.

Draft commit:
```
ci(secrets): add tracing leak heuristic gate
```

## Phase acceptance tests

The phase is "done" when all of the following pass on a clean Windows 11 dev machine and on CI:

### Functional

- A token account with a manual cookie kind round trips through `add`, `set_active`, `get_active`, `edit`, `remove`. The on disk `config.json` shows `dpapi:v1:<base64>` for the `value` field.
- A token account with an OAuth token kind round trips and the same wrapped storage check passes.
- A token account with an API key kind round trips.
- `secrets:set_manual_cookie("claude", raw)` stores the normalized cookie in the cookie cache, retrievable on next launch.
- `cookies:import_for_provider("claude", { domains: ["claude.ai"] })` on a developer machine with Chrome installed and logged into claude.ai returns a non empty `cookie_header`. (If the dev machine runs Chrome 127 plus, the call returns the v20 structured error and the cooldown persists; the dev re runs the test using the manual paste flow and asserts that path succeeds.)
- `cookies:import_for_provider("claude", { domains: ["claude.ai"] })` on Firefox profiles returns a non empty `cookie_header`.
- v20 cooldown survives an app restart.
- Manual cookie paste path bypasses the browser entirely and returns its own normalized header.

### Storage hygiene

- `%APPDATA%\CodexBar4Windows\config.json` exists with an ACL that restricts read to the current user (verified by running `Get-Acl` and checking that no `Everyone` or other user SID has read).
- The cookie cache file at `%LOCALAPPDATA%\CodexBar4Windows\cache\cookie-headers.json` is unreadable as plain JSON. `Get-Content` shows the DPAPI binary header.
- Cookie cache file deletes on `cookies:clear_cache`.
- No DPAPI call passes `LocalMachine`. Static grep gate in CI.
- All DPAPI calls pass `CRYPTPROTECT_UI_FORBIDDEN`. Static grep gate in CI.

### Migration

- A synthetic `config.json` containing plain secrets is rewritten on first launch with the migration flag flipped. The flag is set; the second launch is a byte for byte no op.
- Resetting the flag and running the migration again does not double wrap (the runner detects the `dpapi:v1:` prefix and skips that field).

### Logging

- A full pass of "add three token accounts, import cookies for two providers, refresh one OAuth token (mocked)" produces no log line matching the secret shape heuristic.
- `format!("{}", SensitiveString::new("...")) == "<redacted>"`.
- `tracing::info!(token = ?SensitiveString::new("abc"))` emits `token=<redacted, 3 bytes>` in the captured buffer.

### Security review

- No code path under `crates/codexbar-secrets/` or `crates/codexbar-cookies/` writes a file outside `%LOCALAPPDATA%\CodexBar4Windows\`, `%APPDATA%\CodexBar4Windows\`, or `std::env::temp_dir()` (for cookie SQLite copies that are deleted in `Drop`).
- `cookies:import_for_provider` is the only path that materializes a plaintext `Cookie:` header outside the secrets crate, and the value crosses the Tauri IPC boundary only when the renderer asked for it as the result of a user initiated action.
- The renderer cannot fetch a stored token account's `value`. `TokenAccountView` omits the field at the Rust to TypeScript boundary.

## CI gates

The CI workflow gains four jobs in this phase:

1. `cargo test --workspace --target x86_64-pc-windows-msvc` on a `windows-latest` runner. Required.
2. `cargo clippy --workspace --all-targets -- -D warnings`. Required.
3. The secret leak heuristic test from task 2.19. Required.
4. A static `grep` gate (using `ripgrep`): asserts that no source file under `crates/codexbar-secrets/`, `crates/codexbar-cookies/`, or `src-tauri/` contains the literal `CRYPTPROTECT_LOCAL_MACHINE` and that every `dpapi_protect` callsite passes `CRYPTPROTECT_UI_FORBIDDEN`. Implemented as a small Rust test in `crates/codexbar-secrets/tests/static_audit.rs` that scans the source tree (the test crate reads the workspace's own files; this is a project local audit, not a procedural macro).

The macOS and Linux CI legs of these crates are excluded by `#[cfg(windows)]` on the DPAPI and keyring entry points; on non Windows targets the trait impls are absent and the crate exposes only the `SensitiveString`, normalizer, and types. This keeps editor type checking on dev machines (some maintainers code on macOS) without leaking platform code.

## Risks

R1 (v20 App Bound Encryption) is the dominant risk on this surface. The plan does not attempt to defeat v20. The mitigation is the manual paste flow, deliverable 6, and the per browser cooldown so the toast does not re fire. If Chrome 127 plus turns out to be more widespread than expected on the dev team, accept that all Chromium browsers fall back to manual paste and update the README to document this. The provider phases that follow are designed against the manual paste contract anyway.

R2 (SmartScreen and antivirus) is not a Phase 2 concern at the code level, but reading browser cookie SQLite files and calling `CryptUnprotectData` are exactly the behaviors that AV heuristics flag. Note this for the Phase 7 packaging plan: do not strip symbols, do submit binaries to Defender's false positive form, do sign with an OV or EV cert before any public release.

A subtler risk: the macOS code keeps cookie cache entries per managed account UUID for Codex. Phase 2 preserves that schema (the `scope` field on `CookieHeaderCacheEntry`) but does not yet wire it. Phase 4 (Codex) is the first time the scope field is populated. Make sure the Phase 4 plan opens this loop, not Phase 2.

A risk with the `keyring` 3.x crate: in some Windows configurations (corporate domain joined with strict Credential Manager policies), `keyring::Entry::set_password` returns a non actionable error. Treat the Credential Manager mirror as best effort. The DPAPI file blob is canonical. Tests assert this fallback.

A risk with the `windows` 0.61 crate: surface drift between minor versions. Pin the version in `Cargo.toml`, do not use a tilde or caret range. Upgrades are an explicit chore commit later.

A risk with `rusqlite`: the bundled SQLite vendored build adds about 1.5 MiB to the final binary. Acceptable for v1. If size becomes a concern in Phase 7, switch to dynamic linking against `winsqlite3.dll` (present on Windows 10 1903 plus).

## Time estimate

Working from a clean Phase 1 baseline, with a single experienced Rust plus Tauri engineer:

| Tasks | Estimate |
|---|---|
| 2.1 to 2.5 (DPAPI, SecureFile, blob stores)             | 2.0 days |
| 2.6 to 2.10 (Cookie importers and cache)                | 3.0 days |
| 2.11 to 2.13 (TokenAccount, gate, orchestrator)         | 1.5 days |
| 2.14 (Tauri IPC and React hooks)                        | 1.0 days |
| 2.15 to 2.17 (Migration and settings toggles)           | 1.0 days |
| 2.18 to 2.19 (Tracing redaction and CI gate)            | 1.0 days |
| Integration on a real Windows box, manual smoke         | 0.5 days |
| Buffer for v20 surprises and SQLite lock edge cases     | 1.0 days |
| **Total**                                               | **11.0 days** |

Calendar: roughly two weeks of focused work. If split across two engineers (one on `codexbar-secrets`, one on `codexbar-cookies`), seven calendar days is achievable.

## Open questions

1. Do we mirror OAuth refresh tokens to Credential Manager for every provider, or only for the ones we own end to end (Antigravity in Phase 5, none in Phase 4 because Claude CLI owns the file)? The plan currently mirrors for every provider that calls `OAuthRefreshTokenStore::save`. Confirm with the Phase 4 owner before merging the v1 IPC contract.
2. The cookie header cache currently has no TTL. The macOS code also has no TTL. Confirm we are intentionally relying on auth probe failure to invalidate. The Phase 4 Claude integration should write the auth probe that triggers cache invalidation; flag this in the Phase 4 plan.
3. The settings field `allow_browser_cookie_import` defaults to `true`. Confirm this is the desired default. The Phantom wallet polish principle from the spec argues for an explicit first run opt in for any access that touches another app's data; on the other hand, the entire macOS experience defaults to allowed. Decision needed before Phase 3 builds the onboarding flow.
4. The `windowsSecretEncryptionV1` migration handles plain to DPAPI wrapping. There is no current path for users importing a macOS `config.json` directly. If we ship a migration tool later, it should run before this migration. Note for the Phase 7 packaging plan.
5. Should `cookies:import_for_provider` cache the result in `CookieHeaderCache` always, or only when the caller asks for it? The plan caches always. Confirm with the popup refresh logic in Phase 3.
6. Identifier normalization for `SecretKey`: do we lowercase non ASCII characters via `to_lowercase` or via Unicode case folding? The plan uses `to_lowercase`. Providers in v1 are all ASCII so this does not matter; revisit if any future provider has a non ASCII identifier.

## What the next phases inherit

Phase 3 (Tray icon and popup polish): receives the typed React hooks in `src/hooks/useSecretsApi.ts`. The popup's "Add account" flow calls `secrets:add_token_account`. The "Manual cookie" pane calls `secrets:set_manual_cookie`. The banner system displays the "Disable secret storage" debug banner and the v20 toast. No new Rust code is required from Phase 2 for the popup to work; the contract is locked.

Phase 4 (Claude provider, the first real consumer): uses `CookieImporter::import_for("claude", ...)`, `TokenAccountStore` for sessionKey accounts, and `OAuthRefreshTokenStore` for the refresh token mirror. Validates that the `ClaudeCredentialRouting` style logic (sessionKey vs `sk-ant-oat` classification) belongs in the provider crate, not in `codexbar-secrets`. The provider crate exposes a single classifier function that consumes a `SensitiveString` and returns a `ClaudeCredentialKind` enum.

Phase 5 onwards (Codex, Cursor, Copilot, Gemini, OpenRouter, Factory): each provider crate is a thin layer over the platform surface this phase delivers. If any new provider requires a new auth pattern (smartcard, mTLS), reopen this phase rather than inlining the new pattern into the provider crate.

## Done definition

A reviewer ticks every box in the "Phase acceptance tests" section above. CI is green on the `main` branch with all four new gates active. The plan document at this path is updated only if the API surface drifts during implementation (rare; the surface is locked here). At that point the phase is closed and Phase 3 begins.
