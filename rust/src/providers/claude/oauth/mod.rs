//! Claude OAuth glue. P4-11 ships credential discovery and the DPAPI
//! cache; P4-12 layers the fetch strategy and the reqwest-backed
//! transport on top.

pub mod cache;
pub mod credentials;
pub mod response;
pub mod strategy;
pub mod transport;
