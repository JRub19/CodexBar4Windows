//! Codex credential storage. Phase 5 Tasks 1.1 through 1.6 land here:
//! - 1.1 credentials.rs (read/write auth.json)
//! - 1.2 identity.rs + jwt.rs (extract email, plan, account id)
//! - 1.3 refresh.rs + errors.rs (8-day refresh flow)
//! - 1.6 dpapi_mirror.rs (DPAPI sidecar at %APPDATA%)

pub mod credentials;
