//! Redacting tracing writer.
//!
//! The cleanest way to scrub structured log lines is at the byte boundary.
//! `tracing_subscriber::fmt` with the JSON formatter emits one event per
//! `\n` terminated line. We wrap the underlying writer with a buffer that
//! collects bytes until a newline, runs [`Redactor::all`] on the complete
//! line, then forwards the scrubbed bytes to the real sink.
//!
//! This buys us redaction across every span and event in the
//! application, without each call site needing to wrap values in
//! `SensitiveString`. `SensitiveString` remains the recommended primary
//! defense; this layer is the safety net.

use std::io::{self, Write};
use std::sync::Mutex;

use tracing_subscriber::fmt::MakeWriter;

use super::Redactor;

/// A `MakeWriter` that wraps an inner writer in a line buffered
/// redactor. Each call to `make_writer` returns a fresh per event handle
/// so writes from concurrent threads do not interleave inside a single
/// line buffer.
pub struct RedactingMakeWriter<M> {
    inner: M,
}

impl<M> RedactingMakeWriter<M> {
    pub fn new(inner: M) -> Self {
        Self { inner }
    }
}

impl<'a, M> MakeWriter<'a> for RedactingMakeWriter<M>
where
    M: MakeWriter<'a>,
    M::Writer: Send + 'static,
{
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter::new(self.inner.make_writer())
    }
}

pub struct RedactingWriter<W: Write> {
    inner: Mutex<W>,
    buffer: Mutex<Vec<u8>>,
}

impl<W: Write> RedactingWriter<W> {
    pub fn new(inner: W) -> Self {
        Self {
            inner: Mutex::new(inner),
            buffer: Mutex::new(Vec::with_capacity(512)),
        }
    }

    fn flush_complete_lines(&self) -> io::Result<()> {
        let mut buf = self.buffer.lock().map_err(|_| poisoned_err())?;
        let mut sink = self.inner.lock().map_err(|_| poisoned_err())?;
        while let Some(idx) = buf.iter().position(|b| *b == b'\n') {
            let mut line: Vec<u8> = buf.drain(..=idx).collect();
            // Strip the newline before scrubbing, append it back after.
            let had_newline = line.last() == Some(&b'\n');
            if had_newline {
                line.pop();
            }
            let scrubbed = match std::str::from_utf8(&line) {
                Ok(text) => {
                    let redacted = Redactor::all(text);
                    redacted.into_bytes()
                }
                Err(_) => line,
            };
            sink.write_all(&scrubbed)?;
            if had_newline {
                sink.write_all(b"\n")?;
            }
        }
        Ok(())
    }
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        {
            let mut local = self.buffer.lock().map_err(|_| poisoned_err())?;
            local.extend_from_slice(buf);
        }
        self.flush_complete_lines()?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flush_complete_lines()?;
        let mut buf = self.buffer.lock().map_err(|_| poisoned_err())?;
        if !buf.is_empty() {
            // Trailing bytes without a newline: still scrub before
            // flushing so a process killed mid line does not leak.
            let mut sink = self.inner.lock().map_err(|_| poisoned_err())?;
            let scrubbed = match std::str::from_utf8(&buf) {
                Ok(text) => Redactor::all(text).into_bytes(),
                Err(_) => buf.clone(),
            };
            sink.write_all(&scrubbed)?;
            buf.clear();
            sink.flush()?;
        } else {
            self.inner.lock().map_err(|_| poisoned_err())?.flush()?;
        }
        Ok(())
    }
}

impl<W: Write> Drop for RedactingWriter<W> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn poisoned_err() -> io::Error {
    io::Error::other("redacting writer mutex poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_line_with_secret_is_scrubbed() {
        let sink: Vec<u8> = Vec::new();
        let mut writer = RedactingWriter::new(sink);
        writer
            .write_all(b"contact alice@example.com today\n")
            .expect("write");
        writer.flush().expect("flush");
        let bytes = writer.inner.lock().expect("inner").clone();
        let out = String::from_utf8(bytes).expect("utf8");
        assert!(out.contains("<redacted-email>"));
        assert!(!out.contains("alice@example.com"));
    }

    #[test]
    fn bearer_and_api_key_are_scrubbed_together() {
        let sink: Vec<u8> = Vec::new();
        let mut writer = RedactingWriter::new(sink);
        writer
            .write_all(b"auth Bearer abc.def-ghi key sk-ant-aaaaaaaaaaaaaaaaaaaaa\n")
            .expect("write");
        writer.flush().expect("flush");
        let out = String::from_utf8(writer.inner.lock().expect("inner").clone()).expect("utf8");
        assert!(out.contains("Bearer <redacted>"));
        assert!(out.contains("<redacted-api-key>"));
    }

    #[test]
    fn partial_writes_are_buffered_until_newline() {
        let sink: Vec<u8> = Vec::new();
        let mut writer = RedactingWriter::new(sink);
        writer.write_all(b"prefix alice@").expect("p1");
        // Mid line; nothing should be flushed yet.
        writer.write_all(b"example.com suffix\n").expect("p2");
        writer.flush().expect("flush");
        let out = String::from_utf8(writer.inner.lock().expect("inner").clone()).expect("utf8");
        assert!(out.contains("<redacted-email>"));
        assert!(!out.contains("alice@example.com"));
    }

    #[test]
    fn flush_emits_trailing_bytes_without_newline() {
        let sink: Vec<u8> = Vec::new();
        let mut writer = RedactingWriter::new(sink);
        writer
            .write_all(b"no newline alice@example.com")
            .expect("w");
        writer.flush().expect("flush");
        let out = String::from_utf8(writer.inner.lock().expect("inner").clone()).expect("utf8");
        assert!(out.contains("<redacted-email>"));
    }
}
