---
summary: "Codex provider notes for the Windows Rust/Tauri port."
read_when:
  - Debugging Codex usage or credits
  - Updating Codex OAuth, CLI TUI, or web-cookie behavior
  - Reviewing Codex local cost scanning
---

# Codex Provider

CodexBar4Windows v1.0.2 supports Codex usage through OAuth credentials, a local
Codex CLI TUI fallback, and a best-effort web-cookie path.

## Preferred Sources

1. **OAuth API** reads `~\.codex\auth.json` or `%CODEX_HOME%\auth.json`, refreshes
   stale access tokens, and calls the OpenAI usage endpoint with the Codex CLI
   user agent.
2. **CLI TUI fallback** runs the local `codex` binary when available and parses
   the rendered status panel. This is the practical fallback for users who have
   Codex installed but no usable OAuth file.
3. **Web-cookie path** is best-effort. Raw `chatgpt.com` requests can fail behind
   Cloudflare, login interstitials, or Chromium cookie encryption changes. It is
   kept as a fallback and future integration point, not as the primary v1.0.2
   data source.

## Windows Auth Notes

- Browser cookie import uses the shared Windows cookie pipeline. New Chromium
  `v20` App-Bound Encryption can require manual cookie paste.
- Token and cookie account values are stored through the DPAPI-backed token
  account store, not plaintext settings.
- Provider identity must stay siloed: Codex snapshots must never borrow account
  identity or plan fields from another provider.

## Local Cost Scan

The cost scanner reads local Codex and pi JSONL logs, aggregates daily and
monthly totals, and exposes per-day/per-model rollups for the popup cost
popover. Pricing is bundled for known OpenAI/Codex models and falls back safely
for unknown model names.

## Not Shipped in v1.0.2

- A standalone `codexbar.exe` CLI peer.
- Full ChatGPT dashboard rendering through a hidden WebView.
- Full multi-account promotion UX for managed Codex homes.

These are post-1.0 roadmap items, not v1.0.2 release blockers.
