---
summary: "Phase 0 bootstrap plan for CodexBar4Windows: wipe Swift sources, scaffold the Tauri 2 + React + Rust workspace, land green CI."
read_when:
  - Executing Phase 0 work
  - Verifying acceptance for entry into Phase 1
  - Onboarding a new engineer to the project
---

# Phase 0 Bootstrap

One-line goal: take the repo from "macOS Swift sources sitting at root" to "a clone of CodexBar4Windows builds a green Tauri 2 tray app on a clean Windows 11 box, with CI enforcing fmt, clippy, test, and build on every push."

## 1. Why this phase exists

The fork inherits a fully-formed macOS Swift codebase. Nothing in that tree compiles on Windows, runs on Windows, or even loads cleanly into a Windows toolchain. Every minute spent leaving it in place is a minute of confusion for contributors, a minute of polluted search results, and a minute of CI failures that have no clean reproduction.

Phase 0 is the demolition and pour-the-foundation phase. After Phase 0, the repository is recognizably a Windows Rust + Tauri project. There are no providers, no popup UI, no settings, no preferences, no auth: just a tray icon that lights up on a clean machine after `git clone`, `npm install`, `npm run tauri dev`. CI guarantees that property never regresses.

This phase is also the place where small inconsistencies get caught cheaply: identifier mismatches, gitignore gaps, missing license attribution, the gap between the spike that worked on one machine and the repo that has to work on every contributor's machine.

## 2. Dependencies on earlier phases

None. Phase 0 is the first phase. The only inputs are:

- The validated `C:\Code\tray-spike` scaffold (read `C:\Code\tray-spike\SPIKE.md`).
- The architecture target in `docs/windows/04-recommended-architecture.md`.
- The Path 1 fork strategy choice already made by the project owner: rebase this fork onto a Win-CodexBar-shaped layout, but do not import Win-CodexBar source verbatim; bring in only the Tauri 2 + Rust + React skeleton we built and validated in the spike.

## 3. Deliverables

A numbered list of concrete artifacts that exist on disk at the end of Phase 0.

1. A Swift-free repository root. All of `Sources/`, `Tests/`, `TestsLinux/`, `Package.swift`, `Package.resolved`, `.swiftformat`, `.swiftlint.yml`, `.swiftpm/`, `Makefile`, `appcast.xml`, `Icon.icns`, `Icon.icon/`, `codexbar.png` (the Mac screenshot), and the Swift-specific scripts under `Scripts/` and `bin/install-codexbar-cli.sh` are removed.
2. A Cargo workspace at the repo root: `/Cargo.toml` defining the workspace, `/rust/` core crate placeholder, `/apps/desktop-tauri/` Tauri shell.
3. A working Tauri 2 + React + TypeScript shell at `apps/desktop-tauri/` whose `tauri.conf.json` declares `productName: "CodexBar4Windows"` and `identifier: "com.codexbar4windows.app"`.
4. A green tray icon: the validated `lib.rs` from the spike, ported into `apps/desktop-tauri/src-tauri/src/lib.rs`, with `tauri = { version = "2", features = ["tray-icon"] }` in `Cargo.toml`.
5. A Windows-shaped `.gitignore`: Rust `target/`, Node `node_modules/`, `dist/`, `.DS_Store` (still useful for cross-platform contributors), Tauri build outputs, VSCode/JetBrains scratch dirs.
6. A GitHub Actions workflow at `.github/workflows/ci.yml` running on `windows-latest`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `npm run tauri build`.
7. The two pre-existing Mac-only workflows (`release-cli.yml`, `upstream-monitor.yml`) deleted or rewritten so CI is not red on day one.
8. A rewritten `README.md` aimed at Windows users: install via Inno installer / Winget / portable EXE (placeholders until Phase 4), build from source via `npm run tauri dev`, link to `docs/windows/`.
9. A `SECURITY.md` and `CONTRIBUTING.md` at the repo root.
10. An annotated `CHANGELOG.md` whose top entry marks the cut from macOS history.
11. A documented branch-protection setup recipe in `docs/windows/plan/branch-protection.md`, executable by the repo owner in the GitHub UI.
12. A `version.env` reset to a pre-release marker (e.g. `VERSION=0.1.0-pre.0`).
13. The `C:\Code\tray-spike` scratch directory deleted once the in-repo build is verified green.
14. A code-signing certificate procurement decision tracked as an open issue (OV vs EV, budget, legal entity), but not blocking this phase.

## 4. Tasks

Each task below is one atomic commit. The numbering is the recommended commit order. Each task targets 30 minutes to 2 hours of work.

### Task 1: Snapshot tag the pre-wipe state

- Files touched: none (git operation).
- What changes: tag the current `main` HEAD as `mac-archive-2026-05-12` and push the tag. This is the cheapest possible insurance policy against losing the macOS history.
- Acceptance check: `git ls-remote --tags origin` shows `mac-archive-2026-05-12`. `git show mac-archive-2026-05-12` displays the commit at the top of the recent commit log (`009420a7` at the time of writing).
- Draft commit message: not applicable; this is a tag, not a commit. Run `git tag -a mac-archive-2026-05-12 -m "snapshot of macOS Swift sources before Windows rewrite"` then `git push origin mac-archive-2026-05-12`.

### Task 2: Wipe Swift sources and macOS-only assets

- Files touched: deletes `Sources/`, `Tests/`, `TestsLinux/`, `Package.swift`, `Package.resolved`, `.swiftformat`, `.swiftlint.yml`, `.swiftpm/`, `Makefile`, `appcast.xml`, `Icon.icns`, `Icon.icon/`, `codexbar.png`.
- What changes: every macOS-specific source, build script, or asset goes. Keep `LICENSE`, `README.md` (will rewrite in a later task), `CHANGELOG.md` (will annotate), `version.env`, `CLAUDE.md`, `AGENTS.md`, `.gitignore` (will port), `docs/`, `.github/` (will prune).
- Acceptance check: `git status` shows only deletions. `Get-ChildItem -Path 'C:\Code\CodexBar4Windows' -Filter '*.swift' -Recurse` returns nothing. `Get-ChildItem -Path 'C:\Code\CodexBar4Windows' -Filter 'Package.*'` returns nothing.
- Draft commit message: `chore(repo): remove macos swift sources and assets`

### Task 3: Wipe macOS-only scripts and bin

- Files touched: deletes the macOS-only files inside `Scripts/` (every `.sh` script that calls `swift`, `xcodebuild`, `make_appcast`, `sign-and-notarize`, `setup_dev_signing`, `install_lint_tools`, `compile_and_run`, `launch`, `lint`, `package_app`, `release`, `test_live_update`, `verify_appcast`, `validate_changelog`, `analyze_quotio`, `build_icon`, `changelog-to-html`, `check-release-assets`, `check_upstreams`, `ci_swift_test_by_suite.py`, `prepare_upstream_pr`, `review_upstream`, `docs-list.mjs`, `generate-llms.mjs`) and deletes `bin/install-codexbar-cli.sh` and `bin/docs-list`.
- What changes: kills every script that has no analogue on Windows. The `Scripts/` directory should end the task empty (delete it as well in the same commit). The `bin/` directory should also be empty (delete it).
- Acceptance check: `Test-Path C:\Code\CodexBar4Windows\Scripts` returns `False`. `Test-Path C:\Code\CodexBar4Windows\bin` returns `False`.
- Draft commit message: `chore(repo): remove macos build and release scripts`

### Task 4: Delete or stub the existing GitHub Actions workflows

- Files touched: deletes `.github/workflows/ci.yml` (Swift CI), `.github/workflows/release-cli.yml` (Mac release pipeline), `.github/workflows/upstream-monitor.yml` (cron job that depends on Mac-specific tooling).
- What changes: the `.github/workflows/` directory ends the task empty. Phase 0 Task 12 will re-add a Windows-focused `ci.yml`. We delete in a separate commit to keep history readable: one commit removes Mac CI, a later commit adds Windows CI.
- Acceptance check: `Get-ChildItem 'C:\Code\CodexBar4Windows\.github\workflows'` returns nothing.
- Draft commit message: `chore(ci): remove macos github actions workflows`

### Task 5: Port `.gitignore` to Rust plus Node

- Files touched: `.gitignore`.
- What changes: replace the Swift-focused content with Windows-Rust-Node entries. New file content should cover: `target/`, `**/target/`, `node_modules/`, `dist/`, `.DS_Store` (cross-platform contributors still use macOS), `Thumbs.db`, `*.local`, `.env`, `.env.*`, `.vscode/`, `.idea/`, `*.iml`, `apps/desktop-tauri/src-tauri/gen/`, `apps/desktop-tauri/dist/`, `apps/desktop-tauri/.tauri/`, `bundle/`, `installer/*.exe`, `installer/*.msi`. Keep `docs/*-analysis.md` so private working notes are not committed.
- Acceptance check: `git status --ignored` after a dry-run `cargo new` and `npm install` shows the new `target/` and `node_modules/` entries flagged ignored, not staged. Open the file and confirm no `xcuserdata`, no `*.dmg`, no `.swiftpm-cache/` lines remain.
- Draft commit message: `chore(repo): port gitignore to rust and node`

### Task 6: Reset `version.env`

- Files touched: `version.env`.
- What changes: set `VERSION=0.1.0-pre.0` (or whatever convention the project owner picks; the constraint is a value that signals "pre-release, nothing shipped yet").
- Acceptance check: `Get-Content C:\Code\CodexBar4Windows\version.env` shows the new value.
- Draft commit message: `chore(repo): reset version to 0.1.0-pre.0 for windows port`

### Task 7: Annotate `CHANGELOG.md` with the macOS to Windows cut

- Files touched: `CHANGELOG.md`.
- What changes: prepend a new top section dated today: `## [Unreleased] - Windows port` with a single line explaining that this fork pivots from the macOS Swift project to a Windows Rust + Tauri rewrite, and that the prior changelog lines are the upstream Mac history preserved for attribution.
- Acceptance check: the first heading in the file is the new entry. The previous Mac entries are still in the file below it.
- Draft commit message: `docs(changelog): annotate cut to windows rewrite`

### Task 8: Scaffold the Cargo workspace skeleton

- Files touched: new `/Cargo.toml` at the repo root; new `/rust/Cargo.toml` and `/rust/src/lib.rs` placeholder.
- What changes: create a workspace `Cargo.toml` at the root with `members = ["rust", "apps/desktop-tauri/src-tauri"]` and a stub `rust` crate containing a single `pub fn version() -> &'static str { env!("CARGO_PKG_VERSION") }`. This is the skeleton the Phase 1 core crate will grow into.
- Acceptance check: `cargo check --workspace` from the repo root succeeds, even though only the placeholder crate exists. `cargo metadata --no-deps --format-version 1` lists `codexbar` (the rust crate) as a workspace member.
- Draft commit message: `feat(rust): scaffold cargo workspace with placeholder core crate`

### Task 9: Scaffold the Tauri 2 + React + TS shell

- Files touched: new `apps/desktop-tauri/` tree, including `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/main.tsx`, `src/App.tsx`, `src/styles.css`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/icons/` (use the spike's default icons for now), `src-tauri/capabilities/default.json`.
- What changes: scaffold the Tauri 2 React TS template into `apps/desktop-tauri/` using `create-tauri-app` globally installed, with project name `CodexBar4Windows`, identifier `com.codexbar4windows.app`. Then nudge the generated files into the agreed layout. Set `productName: "CodexBar4Windows"` in `tauri.conf.json`. Set `"name": "codexbar4windows-desktop"` in `package.json`. Set `name = "codexbar4windows-desktop"` in `src-tauri/Cargo.toml`. Reference the `rust` workspace member as a path dependency `codexbar = { path = "../../../rust" }` so the shell can call into the core crate.
- Acceptance check: `cd apps/desktop-tauri; npm install; npm run tauri build` succeeds (release build). The output EXE is at `apps/desktop-tauri/src-tauri/target/release/codexbar4windows-desktop.exe`. Run `Get-FileInfo` (or `Get-Item`) and confirm the path exists.
- Draft commit message: `feat(desktop): scaffold tauri 2 react ts shell as codexbar4windows`

### Task 10: Port the validated tray-icon code from the spike

- Files touched: `apps/desktop-tauri/src-tauri/Cargo.toml`, `apps/desktop-tauri/src-tauri/src/lib.rs`.
- What changes: replace the generated `lib.rs` body with the spike's tray-icon `setup()` block (the contents of `C:\Code\tray-spike\src-tauri\src\lib.rs`). Update strings: change `tray-spike` tooltip and menu labels to `CodexBar4Windows`. Add `features = ["tray-icon"]` to the `tauri` dependency in `Cargo.toml`. Keep the `greet` command for now as a sanity-check IPC handler; Phase 1 replaces it.
- Acceptance check: `npm run tauri dev` from `apps/desktop-tauri/` boots within 30 seconds (warm cache). The console prints `[tray] icon registered with id 'main'`. The tray icon is visible (may be in the overflow flyout). Left-click the icon toggles the window. Right-click shows the native menu with `Refresh now`, `Show window`, `About CodexBar4Windows`, `Quit Ctrl+Q`. `Quit` exits the app.
- Draft commit message: `feat(desktop): land validated tray icon and native menu`

### Task 11: Confirm identifier consistency across all manifests

- Files touched: `apps/desktop-tauri/src-tauri/tauri.conf.json`, `apps/desktop-tauri/src-tauri/Cargo.toml`, `apps/desktop-tauri/package.json`, `apps/desktop-tauri/index.html`.
- What changes: walk every manifest and confirm the canonical strings are aligned. The mapping:

  | Field | Value |
  |---|---|
  | `tauri.conf.json.productName` | `CodexBar4Windows` |
  | `tauri.conf.json.identifier` | `com.codexbar4windows.app` |
  | `tauri.conf.json.app.windows[0].title` | `CodexBar4Windows` |
  | `apps/desktop-tauri/package.json.name` | `codexbar4windows-desktop` |
  | `apps/desktop-tauri/src-tauri/Cargo.toml [package].name` | `codexbar4windows-desktop` |
  | Root `Cargo.toml` workspace member entries | `rust`, `apps/desktop-tauri/src-tauri` |
  | AUMID (used in Phase 3) | `com.codexbar4windows.app` (record in `docs/windows/plan/phase-0-bootstrap.md` Open Questions for future verification) |

  Resolve any drift. Many will already be correct from Task 9; this task exists to enforce the audit.
- Acceptance check: `grep -R "codexbar4windows" apps/ rust/ Cargo.toml | wc -l` (or PowerShell `Select-String`) returns a positive number and no other casing variants (`Codexbar4windows`, `codexBar4windows`, etc.). `grep -R "tray.spike\|tray_spike" .` returns zero matches (no leftover spike strings).
- Draft commit message: `chore(desktop): align product name and identifier across manifests`

### Task 12: Add the Windows CI workflow

- Files touched: new `.github/workflows/ci.yml`.
- What changes: add the full workflow (see Section 7 below for the exact YAML). It runs on `windows-latest`, installs Rust stable, installs Node 22, caches Cargo and node_modules, runs `cargo fmt -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm ci`, `npm run tauri build`.
- Acceptance check: push the branch; the Actions run shows green for `fmt`, `clippy`, `test`, and `tauri build`. If fmt fails, run `cargo fmt --all` locally and amend (no, create a new commit per project policy). If clippy fails on the placeholder crate, fix the lint and commit the fix.
- Draft commit message: `ci(github): add windows ci workflow for fmt clippy test build`

### Task 13: Rewrite the README for Windows

- Files touched: `README.md`.
- What changes: replace the macOS-focused README with a Windows-aimed one. Sections, in order: a one-line description, a screenshot placeholder (TODO until Phase 1), Install (Inno installer link placeholder, Winget command placeholder, portable EXE placeholder, all clearly labeled "coming in Phase 4"), Build from source (`git clone`, `cd apps/desktop-tauri`, `npm install`, `npm run tauri dev`), Requirements (Windows 10 1903 or newer, WebView2 runtime, MSVC build tools for `cargo`), Project layout (one-paragraph map of `rust/`, `apps/desktop-tauri/`, `docs/windows/`), Documentation (link to `docs/windows/README.md`), Attribution (forked from `steipete/CodexBar` MIT, references `Finesssee/Win-CodexBar` as inspiration), License (MIT).
- Acceptance check: open the README in a Markdown previewer. No mentions of `Homebrew`, `swift`, `xcodebuild`, `appcast`, or `Mac`. Every link target resolves (`docs/windows/README.md` exists, the `LICENSE` link works).
- Draft commit message: `docs(readme): rewrite for windows audience and tauri build`

### Task 14: Add `SECURITY.md`

- Files touched: new `SECURITY.md` at the repo root.
- What changes: a short security policy. How to report vulnerabilities (email or a private GitHub security advisory), supported versions (only the current `main` until v0.1.0 ships), the project's stance on secrets (DPAPI plus Windows Credential Manager, never logged), the project's stance on telemetry (none in Phase 0 through 4; opt-in error reports only later).
- Acceptance check: `Test-Path C:\Code\CodexBar4Windows\SECURITY.md` returns `True`. The file contains the email address or advisory link.
- Draft commit message: `docs(security): add security policy`

### Task 15: Add `CONTRIBUTING.md`

- Files touched: new `CONTRIBUTING.md` at the repo root.
- What changes: a contributor guide that mirrors the branch-and-commit policy from `CLAUDE.md`: work on `main`, atomic commits, conventional commit format (`type(scope): description`, lowercase, under 72 chars, no terminal period), push after every commit, no em dashes in prose. Add a "Development setup" section: install Rust stable, install Node 22, install MSVC build tools, run `cd apps/desktop-tauri; npm install; npm run tauri dev`. Add a "Code style" section: `cargo fmt`, `cargo clippy -D warnings`, ESLint defaults from the scaffold. Add a "Tests" section: `cargo test --workspace` plus the Phase 1 Playwright suite (placeholder).
- Acceptance check: `Test-Path C:\Code\CodexBar4Windows\CONTRIBUTING.md` returns `True`. The file references the conventional-commits format.
- Draft commit message: `docs(contributing): add contributor guide for windows port`

### Task 16: Document branch protection setup

- Files touched: new `docs/windows/plan/branch-protection.md`.
- What changes: a short doc whose audience is the repo owner. Walks through the GitHub UI clicks for: Settings -> Branches -> Add branch protection rule -> Branch name pattern `main` -> Require status checks to pass before merging -> Require branches to be up to date before merging -> Select `ci / windows-build` as a required check -> Require pull request before merging is OPTIONAL for a solo maintainer but documented as a recommended toggle once the team grows beyond one. We do not try to enforce protection via the GitHub API; we document the UI steps instead.
- Acceptance check: `Test-Path C:\Code\CodexBar4Windows\docs\windows\plan\branch-protection.md` returns `True`. The owner walks the steps once and confirms protection is live.
- Draft commit message: `docs(plan): document branch protection setup`

### Task 17: Track the code-signing decision as a tracked open item

- Files touched: `docs/windows/plan/phase-0-bootstrap.md` (the document you are reading); a new GitHub issue created via `gh issue create`.
- What changes: in the Open Questions section of this document, point to the GitHub issue. Use `gh issue create` with title `decide code-signing certificate provider, type, and legal entity` and a body referencing risks R2 from `docs/windows/07-risks-and-open-questions.md`. Choices: OV (around $200/yr) versus EV (around $300+/yr), reseller (SSL.com, Certum, DigiCert), legal entity to be named on the cert. Decision is needed before Phase 4 (installer + signing), not before Phase 1.
- Acceptance check: `gh issue list` shows the new issue. This document's Open Questions section links to the issue URL.
- Draft commit message: `docs(plan): track code-signing decision as github issue`

### Task 18: Smoke-test a clean clone build

- Files touched: none (verification only).
- What changes: from a temporary directory (`C:\Code\clean-clone\`), `git clone` the repo and run the documented build steps. Confirm: `cargo check --workspace` passes; `cd apps/desktop-tauri; npm install; npm run tauri build` produces a release EXE; that EXE launches and registers a tray icon. Document any deviation from `CONTRIBUTING.md` and fix it as a separate follow-up commit.
- Acceptance check: the clean-clone build path produces a runnable EXE with the tray icon. Any deviation discovered (e.g., a missing dependency the contributor needs to install) is patched into `CONTRIBUTING.md` in a follow-up.
- Draft commit message: not a commit on its own; any patches discovered ship as `docs(contributing): add note about <X>` or `fix(desktop): <Y>` commits.

### Task 19: Delete the `C:\Code\tray-spike` scratch project

- Files touched: none in the repo. Disk cleanup only.
- What changes: `Remove-Item -Recurse -Force C:\Code\tray-spike`. The in-repo build has been verified green in Task 10 and Task 18; the spike has no further role.
- Acceptance check: `Test-Path C:\Code\tray-spike` returns `False`. The spike's `SPIKE.md` has already been archived in this plan and in `docs/windows/04-recommended-architecture.md`.
- Draft commit message: no commit; out-of-repo cleanup. Mention in the Phase 0 retro that the spike was removed.

### Task 20: Tag `v0.1.0-pre.0`

- Files touched: none (git operation).
- What changes: tag the green commit on `main` as `v0.1.0-pre.0` and push the tag. This is the entry-point baseline for Phase 1.
- Acceptance check: `git ls-remote --tags origin` shows the tag. The tag points at the commit that has the green CI run.
- Draft commit message: not applicable; this is a tag.

### Task 21: Open Phase 1 planning issue

- Files touched: none (GitHub operation).
- What changes: `gh issue create --title "phase 1 foundations: config dir, logging, settings store, ipc contract, refresh loop"` linking to `docs/windows/06-roadmap.md` and `docs/windows/04-recommended-architecture.md`. Phase 1 starts from the green Phase 0 baseline; this issue is the handoff.
- Acceptance check: `gh issue list --state open` shows the new Phase 1 planning issue.
- Draft commit message: no commit.

### Task 22 (optional, time permitting): Add a smoke test for the tray binary

- Files touched: new `apps/desktop-tauri/src-tauri/tests/smoke.rs`.
- What changes: a minimal `cargo test` that verifies the `lib.rs` `run()` function symbol exists and can be referenced. Real headless tests of the tray are out of scope until Phase 3 (the `tray-icon` crate does not lend itself to unit tests without a Windows desktop session). This task adds a `#[test] fn run_symbol_exists() { let _ = crate::run; }` style placeholder so `cargo test` exercises the crate.
- Acceptance check: `cargo test --workspace` shows at least one passing test, not just zero tests.
- Draft commit message: `test(desktop): add placeholder smoke test for tray binary`

## 5. Acceptance tests for Phase 0 as a whole

A reviewer (the project owner, or any contributor with admin access) must be able to walk through this checklist and tick every box before Phase 1 begins.

- [ ] Fresh clone test: on a clean Windows 11 machine with Rust stable, Node 22, MSVC build tools, and WebView2 runtime preinstalled, the following sequence completes without manual intervention:
  - `git clone https://github.com/JRub19/CodexBar4Windows`
  - `cd CodexBar4Windows/apps/desktop-tauri`
  - `npm install`
  - `npm run tauri dev`
  - Within 90 seconds, a tray icon is visible (possibly in the overflow flyout) and the console emits `[tray] icon registered with id 'main'`.
- [ ] Release build test: `cd apps/desktop-tauri; npm run tauri build` produces `apps/desktop-tauri/src-tauri/target/release/codexbar4windows-desktop.exe`. The EXE launches; the tray icon appears; left-click toggles the window; right-click shows the native menu.
- [ ] CI green: the latest commit on `main` has a green Actions run for the `ci.yml` workflow.
- [ ] No Swift residue: `git ls-files | Select-String -Pattern '\.swift$|Package\.swift|Package\.resolved|\.swiftformat|\.swiftlint' | Measure-Object -Line` reports 0 lines. `git ls-files Scripts bin 2>$null | Measure-Object -Line` reports 0 lines.
- [ ] Identifier consistency: `git grep -i "codexbar4windows"` shows matches. `git grep -i "tray.spike\|tray_spike"` shows no matches.
- [ ] Documentation present and consistent: `README.md` reads like a Windows project; `SECURITY.md` is present; `CONTRIBUTING.md` documents conventional commits; `docs/windows/plan/branch-protection.md` walks the GitHub UI steps.
- [ ] Branch protection live: the repo owner has clicked through the GitHub UI and `main` requires the CI workflow to pass before merge.
- [ ] Version reset: `version.env` reads `VERSION=0.1.0-pre.0` (or the agreed equivalent).
- [ ] Changelog annotated: the top entry of `CHANGELOG.md` is the Windows cut entry.
- [ ] Code-signing tracking: a GitHub issue exists titled like "decide code-signing certificate provider, type, and legal entity"; it is linked from this document's Open Questions section.
- [ ] Spike retired: `C:\Code\tray-spike` no longer exists on the maintainer's machine.
- [ ] Baseline tagged: `git tag --list | Select-String v0.1.0-pre.0` shows the tag; `git tag --list | Select-String mac-archive-2026-05-12` also shows the archive tag.

### Reproducible verification steps (PowerShell)

```powershell
# 1. Workspace builds clean.
Set-Location 'C:\Code\CodexBar4Windows'
cargo fmt --all -- --check
if (-not $?) { Write-Error 'cargo fmt failed' }
cargo clippy --workspace --all-targets -- -D warnings
if (-not $?) { Write-Error 'cargo clippy failed' }
cargo test --workspace
if (-not $?) { Write-Error 'cargo test failed' }

# 2. Desktop shell builds and produces an EXE.
Set-Location 'C:\Code\CodexBar4Windows\apps\desktop-tauri'
npm ci
if (-not $?) { Write-Error 'npm ci failed' }
npm run tauri build
if (-not $?) { Write-Error 'tauri build failed' }
Test-Path 'src-tauri\target\release\codexbar4windows-desktop.exe'

# 3. No swift residue.
Set-Location 'C:\Code\CodexBar4Windows'
git ls-files | Select-String -Pattern '\.swift$|Package\.swift|Package\.resolved'

# 4. Identifier consistency.
git grep -i 'codexbar4windows' -- apps rust Cargo.toml
git grep -i 'tray.spike|tray_spike'
```

## 6. CI gates introduced by this phase

The single workflow `.github/workflows/ci.yml`. It runs on every push to `main` and on every pull request targeting `main`. It is the only required status check for branch protection on `main`.

```yaml
name: ci

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-Dwarnings"

jobs:
  windows-build:
    name: windows build
    runs-on: windows-latest
    timeout-minutes: 45

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust toolchain (stable)
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
          targets: x86_64-pc-windows-msvc

      - name: Cache cargo registry and target
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: ". -> target"
          shared-key: windows-stable

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: "22"
          cache: npm
          cache-dependency-path: apps/desktop-tauri/package-lock.json

      - name: Cargo fmt (check)
        run: cargo fmt --all -- --check

      - name: Cargo clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: Cargo test
        run: cargo test --workspace --all-features

      - name: Install desktop deps
        working-directory: apps/desktop-tauri
        run: npm ci

      - name: Tauri build (release)
        working-directory: apps/desktop-tauri
        run: npm run tauri build

      - name: Upload desktop EXE artifact
        if: github.event_name == 'push' && github.ref == 'refs/heads/main'
        uses: actions/upload-artifact@v4
        with:
          name: codexbar4windows-desktop-${{ github.sha }}
          path: apps/desktop-tauri/src-tauri/target/release/codexbar4windows-desktop.exe
          if-no-files-found: error
          retention-days: 14
```

Notes on this workflow:

- `RUSTFLAGS: "-Dwarnings"` is set globally so any new warning fails CI. The same effect is also enforced in the explicit clippy step; the env var catches warnings from non-clippy builds (notably the `cargo test` step) which would otherwise slip through.
- `Swatinem/rust-cache@v2` is the standard rust-cache action for GitHub Actions. It handles cache key derivation from `Cargo.lock`.
- `npm ci` (not `npm install`) enforces a clean install against `package-lock.json`. Phase 0 commits the lock file.
- Artifact upload runs only on pushes to `main`, not on PRs, to keep PR runs fast and to avoid leaking artifacts from forked PRs.
- The required-status-check name to enter into branch protection is `windows build` (the job's `name:` value), which GitHub presents in the UI as `ci / windows build`.

## 7. Risks specific to this phase

### Risk PR-0-A: `tray-icon` feature drift across Tauri patch releases

The spike validated against `tauri = "2"` resolving to 2.11.1. A future patch release could change the `tray-icon` feature surface. If Phase 0 lands on a different patch version than the spike, the spike's `lib.rs` may need small adjustments.

- Mitigation: pin to a known-good minor version (`tauri = "=2.11"`) in `apps/desktop-tauri/src-tauri/Cargo.toml` for Phase 0. Phase 1 can relax the pin once a test harness exists.

### Risk PR-0-B: WebView2 runtime missing on the CI runner

GitHub's `windows-latest` runner ships with WebView2 but the version drifts. A Tauri build that links against an unexpected WebView2 SDK version can produce surprising warnings or failures.

- Mitigation: the CI workflow does a release build; if WebView2 issues appear, pin the Tauri patch version (as above) and add a `windows-2022` runner pin (already implicit in `windows-latest`, but worth testing both `windows-latest` and `windows-2022` once to compare).

### Risk PR-0-C: `cargo fmt --check` failing on the generated Tauri template

The `create-tauri-app` template emits Rust code that may not match the project's `rustfmt` defaults (we use stable defaults, but the template may use different idioms).

- Mitigation: run `cargo fmt --all` once after Task 10 and commit the result before Task 12 lands the fmt-check step into CI. If a re-format is needed, ship it as `style(desktop): apply rustfmt to scaffold`.

### Risk PR-0-D: `clippy -D warnings` failing on placeholder code

The placeholder `rust` core crate may trigger `clippy::missing_docs_in_private_items` or similar pedantic lints once we add real content. Phase 0 only ships a single `pub fn version()`, but Phase 1 will add more.

- Mitigation: Phase 0 starts with default clippy levels (no pedantic). The `.clippy.toml` or `[lints]` block at the root `Cargo.toml` stays empty. Phase 1 can opt into stricter lints.

### Risk PR-0-E: Tray icon hidden in the overflow flyout misreads as "broken"

Windows 11 hides new tray icons by default. A contributor who follows the build steps and sees nothing in the system tray will think the build failed.

- Mitigation: document this in `CONTRIBUTING.md` ("After `npm run tauri dev`, click the chevron on the taskbar to find the CodexBar4Windows icon. Drag it onto the visible tray to pin it."). Phase 3 adds the first-run toast that walks users through this.

### Risk PR-0-F: Identifier mismatches between the spike and the repo

The spike used `com.spike.tray`. The repo uses `com.codexbar4windows.app`. Any leftover `com.spike.tray` or `tray-spike` strings cause subtle bugs (AUMID-based pinning fails, registry keys split across two apps).

- Mitigation: Task 11 explicitly audits every manifest. Add a `grep`-style guard to CI in a Phase 1 follow-up if drift recurs.

### Risk PR-0-G: Branch protection locks out the solo maintainer

If the maintainer enables "Require pull request reviews before merging" on a solo project, every change becomes a forced PR with no reviewer.

- Mitigation: the branch-protection doc explicitly recommends `Require status checks` and `Require branches to be up to date` only. Pull-request review enforcement is documented as optional and is the call of the maintainer when the team grows.

## 8. Time estimate

Working assumption from `docs/windows/06-roadmap.md`: one engineer at 50 percent capacity. Phase 0 in that roadmap is sized at week 0 to week 1, which is 5 calendar days at 50 percent capacity, equivalent to roughly 20 engineering hours.

Bottom-up estimate from the task list:

| Task | Hours |
|---|---|
| 1 Snapshot tag | 0.1 |
| 2 Wipe Swift sources | 0.5 |
| 3 Wipe Mac scripts | 0.5 |
| 4 Delete Mac workflows | 0.2 |
| 5 Port .gitignore | 0.5 |
| 6 Reset version.env | 0.1 |
| 7 Annotate CHANGELOG | 0.5 |
| 8 Scaffold workspace | 1.0 |
| 9 Scaffold Tauri shell | 2.0 |
| 10 Port tray-icon code | 1.5 |
| 11 Identifier audit | 1.0 |
| 12 CI workflow | 2.0 |
| 13 Rewrite README | 1.5 |
| 14 SECURITY.md | 0.5 |
| 15 CONTRIBUTING.md | 1.0 |
| 16 Branch-protection doc | 0.5 |
| 17 Code-signing issue | 0.3 |
| 18 Clean-clone smoke test | 1.5 |
| 19 Delete spike | 0.1 |
| 20 Tag v0.1.0-pre.0 | 0.1 |
| 21 Phase 1 planning issue | 0.3 |
| 22 Smoke test (optional) | 1.0 |
| Total | ~16.2 |

At 50 percent capacity (4 hours of focused work per calendar day), that is 4 calendar days of dedicated time, distributed across 5 to 6 calendar days to absorb context-switching and CI iteration cycles. The roadmap's "week 0 to week 1" estimate holds.

If the engineer is unfamiliar with Rust or Tauri, double Tasks 8, 9, 10, and 12 (the Rust- and CI-heavy ones) and budget 8 to 9 calendar days.

## 9. Open questions for the project owner

1. **Final project name on Windows**: `CodexBar4Windows` is the working name and identifier in this plan. Confirm before Task 9 lands; renaming after the scaffold is in is painful. If the owner picks a different name (e.g., `CodexBar for Windows` as a display name with the same identifier), say so before Phase 0 starts.
2. **Identifier**: this plan uses `com.codexbar4windows.app`. Confirm or replace. The string also drives the AUMID, registry keys, install path under `%LOCALAPPDATA%\Programs\`, and the Tauri updater endpoint.
3. **Code-signing certificate**: tracked as a GitHub issue per Task 17. Decision needed before Phase 4 (Packaging and distribution). Budget around $200/yr OV, $300+/yr EV. Legal entity to be named on the cert.
4. **Branch policy**: `CLAUDE.md` says "all work on main, atomic commits, push after every commit." Phase 0 follows that policy. The `branch-protection.md` doc still walks the GitHub UI steps for `main`-only protection; the maintainer can decide whether to require PRs for non-solo contributors when the team grows.
5. **MSRV (minimum supported Rust version)**: the spike used 1.93.1. Pin `rust-toolchain.toml` to `stable` and re-evaluate in Phase 1, or pin to a specific MSRV now to lock CI determinism? Default recommendation: `channel = "stable"`, no specific version, since Tauri does not currently publish a tight MSRV claim.
6. **Node version**: the spike used Node 22.22.2. Pin `engines.node` in `package.json` to `>=22` and CI to `22` for reproducibility. Confirm.
7. **License attribution wording**: `README.md` will say "Forked from steipete/CodexBar (MIT)." Should it also explicitly thank `Finesssee/Win-CodexBar` as inspiration even though we did not import their source? Default recommendation: yes, in the Acknowledgements section.
8. **Telemetry stance from day one**: confirm no telemetry in Phase 0. The Mac upstream has none; we match that default. Opt-in error reports return as an option in Phase 5 (Beta).
