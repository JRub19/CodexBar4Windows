//! Claude.ai web probe. Reads a cookie value from `--cookie` (or
//! `CLAUDE_PROBE_COOKIE`), hits the canonical endpoint list, and prints
//! one report per response. Used to diagnose field renames before they
//! affect end users.
//!
//! Tokens are never echoed; the report only prints redacted email and
//! plan hints, plus the top-level JSON keys.

mod endpoints;
mod report;

use std::process::ExitCode;

use endpoints::ENDPOINTS;
use report::distill;
use reqwest::Client;
use tracing::error;

fn main() -> ExitCode {
    init_logging();
    let cookie = match read_cookie() {
        Ok(c) => c,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            error!("runtime build failed: {e}");
            return ExitCode::from(1);
        }
    };
    rt.block_on(run(&cookie))
}

fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("CODEXBAR_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

fn read_cookie() -> Result<String, String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--cookie=") {
            return Ok(value.to_string());
        }
        if arg == "--cookie" {
            return args
                .next()
                .ok_or_else(|| "missing value after --cookie".to_string());
        }
    }
    match std::env::var("CLAUDE_PROBE_COOKIE") {
        Ok(v) => Ok(v),
        Err(_) => Err("provide --cookie <header> or set CLAUDE_PROBE_COOKIE".to_string()),
    }
}

async fn run(cookie: &str) -> ExitCode {
    let client = match Client::builder()
        .gzip(true)
        .user_agent("codexbar-webprobe/0.1")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            error!("client build failed: {e}");
            return ExitCode::from(1);
        }
    };
    const MAX_BODY_BYTES: usize = 200 * 1024;
    for probe in ENDPOINTS {
        let response = match client
            .get(probe.url)
            .header("Cookie", cookie)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                eprintln!("---");
                eprintln!("URL:           {}", probe.url);
                eprintln!("Error:         {e}");
                continue;
            }
        };
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let bytes = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("---");
                eprintln!("URL:           {}", probe.url);
                eprintln!("Error reading body: {e}");
                continue;
            }
        };
        let body: &[u8] = if bytes.len() > MAX_BODY_BYTES {
            &bytes[..MAX_BODY_BYTES]
        } else {
            &bytes
        };
        let report = distill(probe.url, status, content_type, body);
        println!("---");
        println!("[{}]", probe.label);
        print!("{}", report.format());
    }
    ExitCode::SUCCESS
}
