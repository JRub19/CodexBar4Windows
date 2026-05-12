//! Codex Web strategy. Uses chatgpt.com session cookies (imported from
//! Chrome/Edge/Brave/Firefox or pasted manually) to talk to the same
//! `wham/usage` endpoint the OAuth path hits.

pub mod cookie_resolver;
pub mod endpoints;
pub mod strategy;
