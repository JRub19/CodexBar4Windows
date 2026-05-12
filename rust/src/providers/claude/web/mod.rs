//! Claude web path. Reads a `sessionKey=...` Cookie header from the
//! shared cookie cache or via a live browser import, then calls the
//! claude.ai JSON API.

pub mod cookie_cache;
pub mod endpoints;
pub mod org_selection;
pub mod strategy;
pub mod transport;
