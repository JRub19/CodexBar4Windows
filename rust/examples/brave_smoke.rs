//! Smoke test: pull cookies for chatgpt.com from the live Brave install.
//! Verifies the v10 decryption path against real user data.
//!
//! Run with:
//!   cargo run --example brave_smoke --manifest-path rust/Cargo.toml

use codexbar::cookies::{BrowserCookieImporter, BrowserDetection, BrowserId, ChromiumCookieReader};

fn main() {
    let presence = BrowserDetection::probe(BrowserId::Brave);
    println!("Brave installed: {}", presence.is_installed());
    println!("Local State:     {:?}", presence.local_state_path);
    println!("Cookie DB:       {:?}", presence.cookie_db_path);
    println!();

    if !presence.is_installed() {
        eprintln!("Brave not detected on this machine. Aborting.");
        std::process::exit(1);
    }

    let reader = ChromiumCookieReader::new(presence);
    let domains = ["chatgpt.com", ".chatgpt.com", "openai.com", ".openai.com"];
    let result = reader.import_for(&domains);
    match result {
        Ok(cookies) => {
            println!(
                "Imported {} cookie(s) for chatgpt.com / openai.com domains.",
                cookies.len()
            );
            for cookie in &cookies {
                // Print metadata only; never echo the value itself.
                println!(
                    "  host={:<30} name={:<28} path={:<10} secure={} httpOnly={} value.len={}",
                    cookie.host,
                    cookie.name,
                    cookie.path,
                    cookie.is_secure,
                    cookie.is_http_only,
                    cookie.value.len(),
                );
            }

            // Build a minimal Cookie header out of the session-relevant
            // ones so we can confirm the format the strategy would emit.
            let session_keys = [
                "__Secure-next-auth.session-token",
                "session",
                "__Host-next-auth.csrf-token",
                "_account",
                "_puid",
            ];
            let header: Vec<String> = cookies
                .iter()
                .filter(|c| session_keys.iter().any(|key| c.name == *key))
                .map(|c| format!("{}=<len={}>", c.name, c.value.len()))
                .collect();
            println!();
            println!("Session-shaped cookies present:");
            if header.is_empty() {
                println!("  (none)");
            } else {
                for line in &header {
                    println!("  {line}");
                }
            }
        }
        Err(err) => {
            eprintln!("Cookie import failed: {err:?}");
            std::process::exit(2);
        }
    }
}
