# Contributing to CodexBar4Windows

Welcome. This guide covers what you need to start contributing.

## Code of conduct

Be kind, be specific, do not be a jerk. The maintainer reserves the right to remove people who cannot follow that.

## Workflow

CodexBar4Windows follows the rules in [`CLAUDE.md`](CLAUDE.md). In short:

- All work lands on `main`. No long lived feature branches.
- Atomic commits. One logical change per commit. If you touched unrelated things, split them.
- Conventional commit format: `type(scope): description`. Lowercase, under 72 chars, no period at the end. Allowed types: `feat`, `fix`, `refactor`, `chore`, `docs`, `test`, `ci`, `style`, `perf`, `build`.
- Push after every commit. If CI fails, read the error, fix it, commit, push again.
- No em dashes or single dashes in prose. Use commas, periods, colons. Bullet markers and command line flags are fine.
- Do not ask permission for git ops, running tests, installing deps, or reading files. Just do it. If something breaks, fix it.

Examples of good commit subjects:

```
feat(rust): scaffold cargo workspace with placeholder core crate
feat(desktop): land validated tray icon and native menu
fix(claude): handle utilization as either int or float
docs(plan): add phase 1 foundations plan
ci(github): cache cargo registry for windows runner
```

## Development setup

Requirements:

- Windows 10 1903 or newer, Windows 11 recommended.
- Rust stable, install via `rustup` (`rustup install stable`, `rustup target add x86_64-pc-windows-msvc`).
- Node 22 or newer.
- MSVC Build Tools 2019 or 2022 with the C++ desktop workload (`winget install Microsoft.VisualStudio.2022.BuildTools --override "--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`).
- WebView2 evergreen runtime (preinstalled on Win 11, install from Microsoft on Win 10).

First build:

```powershell
git clone https://github.com/JRub19/CodexBar4Windows.git
cd CodexBar4Windows\apps\desktop-tauri
npm install
npm run tauri dev
```

The tray icon may live in the overflow flyout on first run. Click the chevron on the taskbar to find it, then drag it next to the Wi Fi icon to pin it.

## Code style

Rust:

- `cargo fmt --all` before commit. CI runs `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`. New warnings fail CI.
- Prefer small typed structs and enums over stringly typed code.
- 4 space indent, default rustfmt settings.

TypeScript and React:

- ESLint defaults from the scaffold.
- Prefer functional components plus hooks.
- Strict TypeScript, no `any` without justification.

Docs:

- Markdown, kebab case filenames.
- Tables for catalogs, bullets for sequences.
- No em dashes (style rule above).

## Tests

```powershell
cargo test --workspace --all-features
```

UI and end to end coverage uses Playwright via Tauri webdriver where practical. Provider fixtures land per provider; see `docs/windows/plan/test-strategy.md`.

## Project layout

- `rust/`, shared core crate. Providers, settings, secrets, refresh loop.
- `apps/desktop-tauri/`, Tauri 2 desktop shell. React TypeScript popup plus Rust tray host.
- `docs/windows/`, Windows port planning and behavioral spec.
- `docs/windows/plan/`, 10 phase execution plan plus the cross phase test strategy.
- `docs/windows/spec/`, 14 subsystem blueprints derived from a deep read of the macOS sources.
- `.github/workflows/ci.yml`, the CI pipeline.

## Where to start

If you want to pick up work:

1. Read `docs/windows/plan/00-master-plan.md` for the big picture.
2. Find the current in flight phase plan in `docs/windows/plan/phase-N-*.md`.
3. Pick an unstarted task. Each task lists files, an acceptance check, and a draft commit message.
4. Implement.
5. Verify the acceptance check.
6. Commit with the draft message, push.

## Reporting issues

File issues at https://github.com/JRub19/CodexBar4Windows/issues. Use the templates. Security issues go through the private advisory path (see [`SECURITY.md`](SECURITY.md)).

## License

By contributing, you agree your changes are licensed under the project MIT license.
