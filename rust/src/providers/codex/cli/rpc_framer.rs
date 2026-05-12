//! Line-delimited JSON-RPC framer.
//!
//! Each outgoing object is encoded on one UTF-8 line terminated by `\n`.
//! Incoming bytes are buffered and split on `\n`; partial reads carry
//! over to the next chunk. The framer is sync because the caller wraps
//! it around the PTY's blocking reader.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl Request {
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Response {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: u64,
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Notification {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Incoming {
    Response(Response),
    Notification(Notification),
    /// The line was valid JSON but did not match either shape. We hand
    /// it back so callers can log without crashing.
    Unknown(serde_json::Value),
}

#[derive(Debug, thiserror::Error)]
pub enum FramerError {
    #[error("frame is not valid utf-8")]
    Utf8,
    #[error("frame is not valid JSON: {0}")]
    Json(String),
}

/// Encode an outgoing `Request` to a single LF-terminated line.
pub fn encode(request: &Request) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(request).expect("request always serializes; static schema");
    bytes.push(b'\n');
    bytes
}

/// Stateful decoder. Feed bytes via `feed`, drain complete messages via
/// `next`. Survives reads that split a single frame in the middle.
#[derive(Default)]
pub struct LineDecoder {
    buffer: Vec<u8>,
}

impl LineDecoder {
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    pub fn poll(&mut self) -> Option<Result<Incoming, FramerError>> {
        loop {
            let newline = self.buffer.iter().position(|b| *b == b'\n')?;
            let line: Vec<u8> = self.buffer.drain(..=newline).collect();
            let mut trimmed = line.as_slice();
            if trimmed.last() == Some(&b'\n') {
                trimmed = &trimmed[..trimmed.len() - 1];
            }
            if trimmed.last() == Some(&b'\r') {
                trimmed = &trimmed[..trimmed.len() - 1];
            }
            if trimmed.is_empty() {
                continue;
            }
            let text = match std::str::from_utf8(trimmed) {
                Ok(t) => t,
                Err(_) => return Some(Err(FramerError::Utf8)),
            };
            return Some(decode_line(text));
        }
    }
}

fn decode_line(text: &str) -> Result<Incoming, FramerError> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| FramerError::Json(e.to_string()))?;
    if value.get("id").is_some() && (value.get("result").is_some() || value.get("error").is_some())
    {
        let response: Response =
            serde_json::from_value(value).map_err(|e| FramerError::Json(e.to_string()))?;
        return Ok(Incoming::Response(response));
    }
    if value.get("method").is_some() {
        let notification: Notification =
            serde_json::from_value(value).map_err(|e| FramerError::Json(e.to_string()))?;
        return Ok(Incoming::Notification(notification));
    }
    Ok(Incoming::Unknown(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn encode_emits_lf_terminated_line() {
        let request = Request::new(1, "account/read", None);
        let bytes = encode(&request);
        assert!(bytes.ends_with(b"\n"));
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"method\":\"account/read\""));
    }

    #[test]
    fn decoder_emits_one_response_per_line() {
        let mut decoder = LineDecoder::default();
        let line = b"{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"ok\":true}}\n";
        decoder.feed(line);
        match decoder.poll().unwrap().unwrap() {
            Incoming::Response(resp) => {
                assert_eq!(resp.id, 7);
                assert_eq!(resp.result, Some(json!({"ok": true})));
            }
            other => panic!("expected response, got {other:?}"),
        }
        assert!(decoder.poll().is_none());
    }

    #[test]
    fn decoder_reassembles_split_frames() {
        let mut decoder = LineDecoder::default();
        let full = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":42}\n";
        let mid = full.len() / 2;
        decoder.feed(&full[..mid]);
        assert!(decoder.poll().is_none());
        decoder.feed(&full[mid..]);
        assert!(matches!(
            decoder.poll().unwrap().unwrap(),
            Incoming::Response(_)
        ));
    }

    #[test]
    fn decoder_handles_notification_lines() {
        let mut decoder = LineDecoder::default();
        decoder.feed(b"{\"jsonrpc\":\"2.0\",\"method\":\"log\",\"params\":{\"m\":1}}\n");
        match decoder.poll().unwrap().unwrap() {
            Incoming::Notification(n) => assert_eq!(n.method, "log"),
            other => panic!("expected notification, got {other:?}"),
        }
    }

    #[test]
    fn decoder_handles_crlf_terminator() {
        let mut decoder = LineDecoder::default();
        decoder.feed(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":1}\r\n");
        assert!(matches!(
            decoder.poll().unwrap().unwrap(),
            Incoming::Response(_)
        ));
    }

    #[test]
    fn decoder_emits_unknown_for_unrecognized_shape() {
        let mut decoder = LineDecoder::default();
        decoder.feed(b"{\"hello\":\"world\"}\n");
        match decoder.poll().unwrap().unwrap() {
            Incoming::Unknown(v) => assert_eq!(v, json!({"hello": "world"})),
            other => panic!("expected unknown, got {other:?}"),
        }
    }

    #[test]
    fn decoder_returns_json_error_for_malformed_line() {
        let mut decoder = LineDecoder::default();
        decoder.feed(b"not-json\n");
        let err = decoder.poll().unwrap().unwrap_err();
        assert!(matches!(err, FramerError::Json(_)));
    }

    #[test]
    fn decoder_skips_empty_lines() {
        let mut decoder = LineDecoder::default();
        decoder.feed(b"\n\n");
        assert!(decoder.poll().is_none());
    }
}
