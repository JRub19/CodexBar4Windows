//! Cursor web path. Ported from
//! `Sources/CodexBarCore/Providers/Cursor/CursorStatusProbe.swift`. Uses
//! the same `WebClient`/`CookieResolver` traits as the Claude web path
//! so the reqwest transport and Chromium cookie importer are shared
//! across providers.

pub mod endpoints;
pub mod fold;
pub mod response;
pub mod strategy;
