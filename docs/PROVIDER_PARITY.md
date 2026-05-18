---
summary: "Provider coverage status for CodexBar4Windows after v1.0.2."
read_when:
  - Planning provider parity work
  - Comparing this fork with macOS CodexBar or Win-CodexBar
  - Writing release notes about provider scope
---

# Provider Parity

CodexBar4Windows v1.0.2 intentionally ships a curated provider set. It does
not try to match the full provider count of either the macOS upstream or the
larger Finesssee Windows fork.

## v1.0.2 Shipped Providers

| Provider | Status | Primary source |
|---|---|---|
| Claude | Shipped | OAuth, web cookie, CLI fallback |
| Codex | Shipped | OAuth, CLI TUI fallback, best-effort web |
| Cursor | Shipped | Web cookie |
| Copilot | Shipped | GitHub OAuth device flow |
| Gemini | Shipped | Gemini CLI OAuth credentials |
| OpenRouter | Shipped | API key / token account |
| OpenAI API | Shipped | Admin/API key |
| Factory | Shipped | WorkOS cookie / refresh token |
| DeepSeek | Shipped | API key / token account |
| Moonshot | Shipped | API key / token account |
| Z.ai | Shipped | API key / token account |
| Venice | Shipped | API key / token account |
| MiniMax | Shipped | Cookie/API token account |
| Mistral | Shipped | Browser cookie token account |
| Kimi | Shipped | Browser cookie token account |
| Kimi K2 | Shipped | API key / token account |
| Augment | Shipped | `auggie` CLI |
| Manus | Shipped | Browser cookie token account |
| Codebuff | Shipped | API key or CLI credentials |

## Upstream Gap

The macOS upstream and Finesssee Windows fork each track many more providers.
Providers still missing after v1.0.2 are parity
backlog, not release blockers.

## Priority Buckets

| Priority | Providers | Rationale |
|---|---|---|
| P1 | Harden live auth/parsing for OpenAI API, Kimi, Kimi K2, MiniMax, Mistral, Augment, Manus, Codebuff | High-visibility providers now registered locally, but need real-account smoke coverage. |
| P2 | MiMo, Command Code, Crof, Alibaba/Qwen, Doubao | Useful provider coverage with known endpoint or cookie paths. |
| P3 | Amp, Antigravity, Kilo, Kiro, Ollama, OpenCode, OpenCode Go, Perplexity, StepFun, Vertex AI, Warp, Windsurf, Abacus, Synthetic | Larger auth, CLI, local-service, or parser risk; schedule after P1/P2. |

## Release Policy

- v1.0.2 ships the 19 providers above.
- New providers should land one at a time with focused parser/auth tests.
- Provider docs must state Windows-specific auth constraints, especially Chrome
  v20 cookie behavior and whether manual cookie paste is required.
- README provider counts must be updated in the same PR as provider registration.
