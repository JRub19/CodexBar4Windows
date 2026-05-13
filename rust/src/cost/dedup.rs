//! Deduplication. Two layers:
//!
//! 1. In-file dedup keyed by `messageId:requestId`. Streaming chunks
//!    re-emit cumulative usage; we keep the last row per key.
//! 2. Cross-file dedup keyed by `sessionId:messageId:requestId` with
//!    the macOS tie-break (non-sidechain > sidechain, parent path >
//!    subagent path, lexicographic file path otherwise).

use std::collections::HashMap;

use super::claude_parser::{ClaudeUsageRow, PathRole};

/// Sort + dedupe a single file's rows. Rows that have both a message
/// id and a request id collapse to the last-seen chunk per key; rows
/// missing either id are kept verbatim (older Claude CLI logs).
///
/// Returns the rows in deterministic order: keyed rows sorted by
/// `messageId:requestId`, then unkeyed rows in insertion order.
pub fn dedup_in_file(rows: Vec<ClaudeUsageRow>) -> Vec<ClaudeUsageRow> {
    let mut keyed: HashMap<String, ClaudeUsageRow> = HashMap::new();
    let mut unkeyed: Vec<ClaudeUsageRow> = Vec::new();
    for row in rows {
        match (&row.message_id, &row.request_id) {
            (Some(msg), Some(req)) => {
                let key = format!("{msg}:{req}");
                keyed.insert(key, row);
            }
            _ => unkeyed.push(row),
        }
    }
    let mut sorted: Vec<(String, ClaudeUsageRow)> = keyed.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<ClaudeUsageRow> = sorted.into_iter().map(|(_, r)| r).collect();
    out.extend(unkeyed);
    out
}

/// Canonical cross-file dedup key `sessionId:messageId:requestId`.
/// Returns `None` for rows missing any of the three components — those
/// rows are not deduped across files (spec §3.3).
pub fn canonical_row_key(row: &ClaudeUsageRow) -> Option<String> {
    let session = row.session_id.as_deref()?;
    let message = row.message_id.as_deref()?;
    let request = row.request_id.as_deref()?;
    Some(format!("{session}:{message}:{request}"))
}

/// `true` if `lhs` should beat `rhs` in cross-file dedup. The macOS
/// `claudeRowWins` tie-break order:
///
/// 1. Prefer non-sidechain over sidechain.
/// 2. Prefer Parent path-role over Subagent.
/// 3. Else lexicographic compare on source path.
pub trait RowWinsOver {
    fn wins_over(&self, other: &Self) -> bool;
}

impl RowWinsOver for ClaudeUsageRow {
    fn wins_over(&self, other: &Self) -> bool {
        match (self.is_sidechain, other.is_sidechain) {
            (false, true) => return true,
            (true, false) => return false,
            _ => {}
        }
        match (self.path_role, other.path_role) {
            (PathRole::Parent, PathRole::Subagent) => return true,
            (PathRole::Subagent, PathRole::Parent) => return false,
            _ => {}
        }
        self.source_path < other.source_path
    }
}

/// Apply cross-file dedup across many files' rows. Returns the winning
/// row per canonical key plus every row that lacks a canonical key
/// (those contribute unconditionally per spec §3.3).
pub fn dedup_cross_file(rows: Vec<ClaudeUsageRow>) -> Vec<ClaudeUsageRow> {
    let mut winners: HashMap<String, ClaudeUsageRow> = HashMap::new();
    let mut un_keyed: Vec<ClaudeUsageRow> = Vec::new();
    for row in rows {
        match canonical_row_key(&row) {
            Some(key) => {
                if let Some(existing) = winners.get(&key) {
                    if row.wins_over(existing) {
                        winners.insert(key, row);
                    }
                } else {
                    winners.insert(key, row);
                }
            }
            None => un_keyed.push(row),
        }
    }
    let mut out: Vec<ClaudeUsageRow> = winners.into_values().collect();
    out.extend(un_keyed);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost::claude_parser::PathRole;

    fn base_row() -> ClaudeUsageRow {
        ClaudeUsageRow {
            day_key: "2026-05-13".into(),
            timestamp_unix_secs: 0,
            model: "claude-sonnet-4-7".into(),
            input_tokens: 100,
            output_tokens: 50,
            cache_read_input_tokens: 0,
            cache_creation_input_tokens: 0,
            message_id: Some("msg-1".into()),
            request_id: Some("req-1".into()),
            session_id: Some("sess-1".into()),
            is_sidechain: false,
            is_vertex: false,
            path_role: PathRole::Parent,
            source_path: "/a/parent.jsonl".into(),
        }
    }

    #[test]
    fn in_file_dedup_keeps_last_chunk_per_message_request_pair() {
        let mut a = base_row();
        a.input_tokens = 100;
        a.timestamp_unix_secs = 1;
        let mut b = base_row();
        b.input_tokens = 250; // cumulative final value
        b.timestamp_unix_secs = 2;
        let out = dedup_in_file(vec![a, b]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].input_tokens, 250);
    }

    #[test]
    fn in_file_dedup_preserves_unkeyed_rows() {
        let mut keyed = base_row();
        let mut unkeyed = base_row();
        unkeyed.message_id = None; // unkeyed
        unkeyed.input_tokens = 999;
        let out = dedup_in_file(vec![keyed.clone(), unkeyed.clone()]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|r| r.input_tokens == 999));
        keyed.input_tokens = 0; // silence unused-mut
    }

    #[test]
    fn canonical_row_key_excludes_rows_missing_any_id() {
        let mut row = base_row();
        assert!(canonical_row_key(&row).is_some());
        row.session_id = None;
        assert!(canonical_row_key(&row).is_none());
        row.session_id = Some("sess".into());
        row.message_id = None;
        assert!(canonical_row_key(&row).is_none());
    }

    #[test]
    fn cross_file_dedup_prefers_non_sidechain() {
        let mut parent = base_row();
        let mut side = base_row();
        side.is_sidechain = true;
        side.source_path = "/a/side.jsonl".into();
        let out = dedup_cross_file(vec![side, parent.clone()]);
        assert_eq!(out.len(), 1);
        assert!(!out[0].is_sidechain);
        // silence unused-mut warning
        parent.input_tokens = 100;
    }

    #[test]
    fn cross_file_dedup_prefers_parent_path_role() {
        let parent = base_row();
        let mut subagent = base_row();
        subagent.path_role = PathRole::Subagent;
        subagent.source_path = "/a/sub.jsonl".into();
        let out = dedup_cross_file(vec![subagent, parent]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path_role, PathRole::Parent);
    }

    #[test]
    fn cross_file_dedup_falls_back_to_lex_path_comparison() {
        let mut a = base_row();
        a.source_path = "/a/z.jsonl".into();
        let mut b = base_row();
        b.source_path = "/a/a.jsonl".into();
        let out = dedup_cross_file(vec![a, b]);
        assert_eq!(out.len(), 1);
        // Lex earlier path wins.
        assert_eq!(out[0].source_path, "/a/a.jsonl");
    }

    #[test]
    fn cross_file_dedup_preserves_rows_without_canonical_key() {
        let mut row = base_row();
        row.session_id = None;
        let mut other = base_row();
        other.message_id = None;
        let out = dedup_cross_file(vec![row.clone(), other.clone()]);
        // Both contribute (neither has a full canonical key).
        assert_eq!(out.len(), 2);
    }
}
