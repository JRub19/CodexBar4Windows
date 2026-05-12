//! Claude CLI strategy. Spawns the `claude` binary inside a 50x160 PTY,
//! sends `/usage`, parses the resulting panel.

pub mod auto_responder;
pub mod parser;
pub mod pty_actor;
pub mod reset_parser;
pub mod strategy;
