---
title: "Cross phase Test Strategy and Quality Gates"
goal: "Define the testing philosophy, levels, and CI gates that bind the ten phase plans into one shippable Windows port."
audience: "Every engineer or LLM agent landing code on CodexBar4Windows."
status: "Authoring. Meta layer above the phase plans in this directory."
related:
  - "docs/windows/plan/phase-0-bootstrap.md"
  - "docs/windows/plan/phase-1-foundations.md"
  - "docs/windows/spec/50-refresh-state-pace.md"
  - "docs/windows/spec/80-feel-and-polish.md"
  - "docs/windows/spec/90-cli-widgets-build.md"
---

# Test Strategy and Quality Gates

CodexBar4Windows is shipped in ten phases (phase 0 bootstrap through phase 9 release). Each
phase plan lists its own acceptance tests. This document is the meta layer: how those tests
weave together, what runs between phases, and what guarantees CI gives the project on every
push. The single goal is that no phase can be declared done while a downstream phase is
already broken, and that the polish list in spec 80 is reachable from a deterministic test
somewhere in the suite.

The doc is structured top down: philosophy, then the test pyramid, then per phase gates,
then the three CI tiers with full YAML, then the supporting contracts (provider fixtures,
ConPTY harness, secret handling), then performance budgets, manual test scripts, telemetry,
and the GA pre release checklist.

---

## 0. Philosophy

Four rules. They are not negotiable.

1. **Every behavioral spec line is reachable from a test.** Spec 50, spec 80, and spec 90
   contain numbered checklists. Each numbered item either has an automated test or sits on
   a manual test script with the spec number in its checkbox label. The doc that owns the
   spec line owns the test reference.
2. **Tests live next to the code they cover.** Unit tests are co located in the same crate
   or TS package. Integration tests live under `rust/tests/` per crate or
   `apps/desktop-tauri/tests/`. End to end tests live under `e2e/`. There is no global
   `tests/` graveyard.
3. **A failing test blocks the merge, not the human.** Tier 1 CI is the only gate that
   matters for merging. Tier 2 and tier 3 are observability and release insurance. We never
   ask reviewers to remember to run tests locally.
4. **Fixtures are the contract.** Every provider, every PTY parser, every cookie decoder
   ships with a fixtures directory under `rust/crates/codexbar-providers/tests/fixtures/`.
   New code without fixtures cannot land. Real secrets never enter fixtures. The git
   pre commit hook rejects them.

---

## 1. Test pyramid

CodexBar4Windows uses five layers, in declining test count.

### 1.1 Unit tests

Pure functions, parsers, formatters, math, state reducers. Fast (`cargo test` for the whole
workspace stays under 90 seconds; `vitest run` stays under 30 seconds). No I/O. No network.
No timers other than `tokio::time::pause`.

| Layer | Tool | Lives in | Target count by GA |
|-------|------|----------|---------------------|
| Rust pure logic | `cargo test` (built in) | `rust/crates/<crate>/src/**/*` with `#[cfg(test)]` | 800 |
| Rust property tests | `proptest` | `rust/crates/<crate>/tests/prop_*.rs` | 50 |
| TS pure logic | `vitest` | `apps/desktop-tauri/src/**/*.test.ts` | 250 |
| TS hook tests | `vitest` + `@testing-library/react-hooks` | `apps/desktop-tauri/src/hooks/*.test.ts` | 60 |

Per phase growth (cumulative unit count, Rust + TS together):

| Phase | Rust unit | TS unit | Notes |
|-------|-----------|---------|-------|
| 0 | 1 | 0 | smoke `2 + 2 == 4` placeholder asserts the harness is wired |
| 1 | 60 | 30 | paths, settings, logging, locale, registry shell, refresh ticker math |
| 2 | 180 | 60 | tray icon renderer, popup window manager, hotkey dispatcher |
| 3 | 340 | 110 | UsageStore, RefreshFrequency, pace math, history bucketing |
| 4 | 470 | 150 | Claude provider (PTY, OAuth, web), Codex provider, cookie pipeline |
| 5 | 580 | 180 | tier 2 providers (Cursor, Gemini, Copilot), OpenRouter, Factory |
| 6 | 660 | 215 | preferences UI logic, language picker, threshold dedup |
| 7 | 730 | 235 | notifications, sounds, celebration, error states |
| 8 | 790 | 250 | feel and polish: animation curves, reduced motion, high contrast |
| 9 | 850 | 250 | installer flow, updater verifier, telemetry opt in |

### 1.2 Integration tests

Cross module tests that touch the OS, the filesystem, or a local mock server. Slow enough
that they go under `tests/` and not `src/`. Each integration suite has its own setup.

| Suite | Tool | Lives in | What it exercises |
|-------|------|----------|-------------------|
| Provider HTTP fetch | `cargo test --test providers_http` + `wiremock` | `rust/crates/codexbar-providers/tests/` | Each provider against fixture HTTP responses. |
| ConPTY parser | `cargo test --test conpty_replay` | `rust/crates/codexbar-providers/tests/` | Replay captured `claude` and `codex` PTY streams. |
| DPAPI round trip | `cargo test --test dpapi_secrets` | `rust/crates/codexbar-secrets/tests/` | Encrypt, restart simulated process, decrypt. |
| Cookie decryption | `cargo test --test cookies_browsers` | `rust/crates/codexbar-cookies/tests/` | DPAPI cookie blob decoders for Chrome, Edge, Brave. |
| Settings round trip | `cargo test --test settings_io` | `rust/crates/codexbar-core/tests/` | Write, kill task, re read, schema migration. |
| Tauri command surface | `cargo test --test tauri_commands` | `apps/desktop-tauri/src-tauri/tests/` | Every `invoke()` command has at least one positive and one negative case. |

Target integration test count by GA: **170**, split roughly 120 Rust and 50 TS.

### 1.3 End to end tests

Playwright drives the Tauri popup against a mocked Rust backend. The backend mock lives in
`e2e/mock-backend/` and implements the Tauri command surface using stub data. This keeps
the e2e suite hermetic.

| Scenario | Coverage |
|----------|----------|
| Popup open and close | tray click, Esc, click outside, re click |
| Provider tabs | left and right arrow nav, click, keyboard, focus ring |
| Empty state | no providers configured |
| Error state | provider returns synthetic error, copy error button works |
| Refresh now | button click triggers backend call, state updates |
| Preferences open | Preferences window opens, every tab renders without overflow |
| Threshold notification | mock backend posts threshold crossing, toast XML is emitted |
| Weekly reset celebration | tray morph plays, in popup confetti renders if popup open |
| Update flow | mock updater returns new version, install banner appears |
| Reduced motion | toggling system pref disables tray animation, popup transitions |
| High contrast | toggling system pref strips Mica, swaps to system colors |

Playwright count by GA: **45 specs**, each named with its source spec line. Runtime budget:
under 8 minutes on `windows-2022`.

### 1.4 Manual exploratory test runs

Every phase ships a manual checklist under `docs/windows/test-plans/phase-<N>.md`. The
tester signs the bottom of the file in the PR that closes the phase. Section 9 of this doc
lists the script per release.

### 1.5 Performance tests

A small harness in `perf/` boots the release build, samples for 60 seconds, and writes
`perf-report.json`. The CI workflow asserts against `perf/budgets.json` (see section 8).

### 1.6 Security tests

Three jobs:

- **Redaction**: every test that writes a log line is captured and grep checked against
  the secret regex set. If any test log contains `sk-ant-`, `Bearer eyJ`, a literal cookie
  value, or a captured token, the test fails.
- **Secret at rest**: cookie store and credential manager entries are read raw from disk
  or registry and asserted to be DPAPI ciphertext (high entropy, ASN.1 DPAPI header bytes).
- **Audit**: `cargo audit` and `npm audit --omit dev` run in tier 3.

---

## 2. Per phase gating tests

Each phase has automated tests it adds (the phase plan's acceptance section is the
authoritative source). A phase cannot be declared done until **its own acceptance tests
pass** and **every earlier phase's tests still pass**. The table below pins the gate.

| Phase | Adds these test classes | Must still pass | Gate test command |
|-------|--------------------------|------------------|-------------------|
| 0 bootstrap | placeholder `cargo test`, `npm test` smoke, CI fmt/clippy | n/a (first phase) | `cargo test --workspace && cd apps/desktop-tauri && npm test -- --run` |
| 1 foundations | paths, settings round trip, locale lookup, registry shell, refresh ticker, DTO export, IPC surface, tray click, popup open | phase 0 | `cargo test --workspace && npm run check:bindings && npm test -- --run` |
| 2 tray icon and popup | tray icon renderer (six patterns), popup window manager, hotkey dispatcher, multi monitor anchor math, DPI scaling tests | phases 0 to 1 | gates of 1 plus `cargo test -p codexbar-tray` |
| 3 refresh loop and UsageStore | RefreshFrequency variants, pace math, history bucketing, failure gates, threshold dedup, widget snapshot writer | phases 0 to 2 | gates of 2 plus `cargo test -p codexbar-core --features full-loop` |
| 4 tier 1 providers | Claude (PTY, OAuth, web), Codex, DPAPI cookie decrypt, ConPTY replay harness, provider fixtures contract enforced | phases 0 to 3 | gates of 3 plus `cargo test -p codexbar-providers --test claude_full --test codex_full --test conpty_replay` |
| 5 tier 2 providers | Cursor, Gemini, Copilot, OpenRouter, Factory; each ships fixtures; provider catalog smoke | phases 0 to 4 | gates of 4 plus `cargo test -p codexbar-providers --test providers_http -- --include-ignored` |
| 6 preferences and config | preferences UI logic, language picker (en, pt-BR, zh-Hans), config schema migration, all settings keys serialize | phases 0 to 5 | gates of 5 plus `npm test -- --run --testNamePattern='preferences|locale'` |
| 7 notifications and celebration | toast XML render, threshold dedup table, sound mute, Focus Assist matrix, celebration trigger conditions | phases 0 to 6 | gates of 6 plus `cargo test -p codexbar-notifications` |
| 8 feel and polish | animation curve math, reduced motion suppression matrix, high contrast strip, copy flash timing, hover delay timings | phases 0 to 7 | gates of 7 plus full Playwright suite, `cargo test -p codexbar-anim`, perf smoke under budget |
| 9 release and installer | installer signed and verified, updater minisign verifier, telemetry opt in toggle, code review pre release manual checklist signed | phases 0 to 8 | gates of 8 plus `cargo test -p codexbar-updater`, `signtool verify /pa /v` on bundle, release manual script signed |

Gate enforcement: each phase plan's PR template includes a checkbox `Gate test command in
docs/windows/plan/test-strategy.md section 2 passed on my machine and in CI`. Reviewers
verify by looking at the CI run linked in the PR.

---

## 3. CI gate hierarchy

Three tiers. Tier 1 runs on every push and pull request. Tier 2 runs on every push to main
and nightly. Tier 3 runs on release tag pushes only.

### 3.1 Tier 1: per push (mandatory for merge)

Fast feedback. Target wall time: under 8 minutes per push. Hard ceiling: 12 minutes.

Workflow file: `.github/workflows/ci.yml`.

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

env:
  RUST_BACKTRACE: 1
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  fmt:
    name: fmt and lint (Rust)
    runs-on: windows-2022
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets --all-features -- -D warnings

  ts-lint:
    name: lint and typecheck (TS)
    runs-on: windows-2022
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: apps/desktop-tauri/package-lock.json
      - working-directory: apps/desktop-tauri
        run: npm ci
      - working-directory: apps/desktop-tauri
        run: npm run lint
      - working-directory: apps/desktop-tauri
        run: npx tsc --noEmit

  test-rust:
    name: cargo test (workspace)
    runs-on: windows-2022
    needs: [fmt]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace --no-fail-fast --locked
      - name: enforce log redaction
        run: powershell -File scripts/ci/check-redaction.ps1
      - name: enforce no real secrets in fixtures
        run: powershell -File scripts/ci/check-fixture-secrets.ps1

  test-ts:
    name: vitest
    runs-on: windows-2022
    needs: [ts-lint]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: apps/desktop-tauri/package-lock.json
      - working-directory: apps/desktop-tauri
        run: npm ci
      - working-directory: apps/desktop-tauri
        run: npm test -- --run --reporter=verbose

  bindings:
    name: dto bindings up to date
    runs-on: windows-2022
    needs: [test-rust]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo run -p codexbar-dto-export --release
      - name: fail if generated bindings drift
        shell: pwsh
        run: |
          $diff = git status --porcelain apps/desktop-tauri/src/bindings
          if ($diff) { Write-Host $diff; exit 1 }

  tauri-debug-build:
    name: tauri build (debug)
    runs-on: windows-2022
    needs: [fmt, ts-lint, test-rust, test-ts, bindings]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: apps/desktop-tauri/package-lock.json
      - working-directory: apps/desktop-tauri
        run: npm ci
      - working-directory: apps/desktop-tauri
        run: npm run tauri build -- --debug
```

Tier 1 wall time budget by job:

| Job | Cold | Warm |
|-----|------|------|
| fmt | 1m 30s | 25s |
| ts-lint | 1m 40s | 30s |
| test-rust | 3m 30s | 1m 10s |
| test-ts | 1m 10s | 30s |
| bindings | 1m 00s | 20s |
| tauri-debug-build | 7m 00s | 2m 30s |
| total wall (parallel) | 7m 30s | 3m 00s |

### 3.2 Tier 2: per push to main and nightly (observability)

Runs Playwright, performance smoke, release build, Linux core build. No merge gate. Result
shows up in the readme badge and the dashboard.

Workflow file: `.github/workflows/ci-tier2.yml`.

```yaml
name: CI tier 2

on:
  push:
    branches: [main]
  schedule:
    - cron: "17 5 * * *"
  workflow_dispatch:

env:
  RUST_BACKTRACE: 1
  CARGO_TERM_COLOR: always

jobs:
  tauri-release-build:
    name: tauri build (release)
    runs-on: windows-2022
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: apps/desktop-tauri/package-lock.json
      - working-directory: apps/desktop-tauri
        run: npm ci
      - working-directory: apps/desktop-tauri
        run: npm run tauri build -- --target x86_64-pc-windows-msvc
      - uses: actions/upload-artifact@v4
        with:
          name: codexbar-release-x64
          path: apps/desktop-tauri/src-tauri/target/x86_64-pc-windows-msvc/release/bundle

  playwright:
    name: e2e (Playwright)
    runs-on: windows-2022
    needs: [tauri-release-build]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: e2e/package-lock.json
      - uses: actions/download-artifact@v4
        with:
          name: codexbar-release-x64
          path: e2e/.bundle
      - working-directory: e2e
        run: npm ci
      - working-directory: e2e
        run: npx playwright install --with-deps
      - working-directory: e2e
        env:
          CODEXBAR_BACKEND_MODE: mock
        run: npx playwright test --reporter=github

  perf-smoke:
    name: performance smoke (budgets)
    runs-on: windows-2022
    needs: [tauri-release-build]
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: codexbar-release-x64
          path: perf/.bundle
      - run: cargo run -p codexbar-perf-harness --release -- run --report perf/perf-report.json --duration 60
      - name: assert against budgets
        run: cargo run -p codexbar-perf-harness --release -- check --report perf/perf-report.json --budgets perf/budgets.json
      - uses: actions/upload-artifact@v4
        with:
          name: perf-report
          path: perf/perf-report.json

  linux-core:
    name: rust core on linux (portability)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo test -p codexbar-core -p codexbar-providers --features cross-platform-mock --workspace --no-fail-fast

  linkcheck-docs:
    name: docs link check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: lycheeverse/lychee-action@v1
        with:
          args: --no-progress docs/**/*.md README.md CHANGELOG.md SECURITY.md
```

Tier 2 wall time budget:

| Job | Cold | Warm |
|-----|------|------|
| tauri-release-build | 14m | 6m |
| playwright | 8m | 4m |
| perf-smoke | 3m | 2m |
| linux-core | 3m | 1m |
| linkcheck-docs | 1m | 30s |
| total wall | 15m | 8m |

### 3.3 Tier 3: release tag only (insurance)

Runs the full Playwright matrix (x64 and arm64, three Windows builds: 22H2, 23H2, 24H2 if
available), accessibility audit with `axe-core` over the Playwright pages, `cargo audit`,
`npm audit`, signing verification on every shipped artifact, and the updater minisign
verifier round trip.

Workflow file: `.github/workflows/ci-tier3.yml`.

```yaml
name: CI tier 3 (release)

on:
  push:
    tags: ["v*.*.*"]
  workflow_dispatch:

env:
  RUST_BACKTRACE: 1
  CARGO_TERM_COLOR: always

jobs:
  playwright-matrix:
    name: e2e matrix
    strategy:
      fail-fast: false
      matrix:
        runner: [windows-2022]
        arch: [x86_64, aarch64]
    runs-on: ${{ matrix.runner }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm
          cache-dependency-path: e2e/package-lock.json
      - working-directory: e2e
        run: npm ci
      - working-directory: e2e
        run: npx playwright install --with-deps
      - working-directory: e2e
        env:
          CODEXBAR_TARGET: ${{ matrix.arch }}
        run: npx playwright test --reporter=github

  a11y-audit:
    name: accessibility audit
    runs-on: windows-2022
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - working-directory: e2e
        run: npm ci
      - working-directory: e2e
        run: npx playwright test a11y/ --reporter=html
      - uses: actions/upload-artifact@v4
        with:
          name: a11y-report
          path: e2e/playwright-report

  security-audit:
    name: dependency security audit
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install cargo-audit --locked
      - run: cargo audit --deny warnings
      - uses: actions/setup-node@v4
        with:
          node-version: 20
      - working-directory: apps/desktop-tauri
        run: npm ci
      - working-directory: apps/desktop-tauri
        run: npm audit --audit-level=high --omit=dev

  sign-verify:
    name: signtool verify all artifacts
    runs-on: windows-2022
    steps:
      - uses: actions/checkout@v4
      - uses: actions/download-artifact@v4
        with:
          name: codexbar-release-x64
          path: dist/x64
      - uses: actions/download-artifact@v4
        with:
          name: codexbar-release-arm64
          path: dist/arm64
        continue-on-error: true
      - shell: pwsh
        run: |
          $files = Get-ChildItem dist -Recurse -Include *.exe,*.msi,*.dll
          foreach ($f in $files) {
            $r = & signtool verify /pa /v $f.FullName
            if ($LASTEXITCODE -ne 0) { Write-Host "fail: $($f.FullName)"; exit 1 }
          }

  updater-minisign-verify:
    name: updater manifest signature
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: sudo apt-get update && sudo apt-get install -y minisign
      - run: minisign -Vm latest.json -p .github/keys/updater.pub
      - run: minisign -Vm beta.json   -p .github/keys/updater.pub
        continue-on-error: true
```

Tier 3 wall time budget: under 35 minutes total. Acceptable because tier 3 only runs on
release tags.

---

## 4. Provider fixtures contract

Every provider has a fixtures directory. New providers cannot land without one. The
directory layout is fixed.

```
rust/crates/codexbar-providers/tests/fixtures/providers/<id>/
  README.md                  # one liner: what this provider needs, how captures were made
  metadata.json              # { provider_id, captured_at, app_version, fixture_version }
  http/
    200-usage.json           # canonical happy path
    200-usage-multi-account.json
    401-unauthorized.json
    403-forbidden.json
    429-rate-limited.json
    500-server-error.json
    network-timeout.json     # signals the harness to drop the connection mid response
  cli/
    usage-happy.ansi         # captured PTY bytes, escape sequences preserved
    usage-rate-limit.ansi
    usage-malformed.ansi     # the parser must report a clean error, not panic
  oauth/
    token-refresh-200.json
    token-refresh-401.json
  config/
    minimal.json             # smallest config.json that enables this provider
    multi-account.json
```

### 4.1 Fixture rules

1. **No real secrets.** Every secret in a fixture uses one of the test prefixes from
   section 6. The pre commit hook rejects real prefixes.
2. **Versioned.** Each fixture file lives next to a `metadata.json` that records the
   `fixture_version` integer. When a provider's parser changes shape, bump the version and
   update the fixtures in the same commit.
3. **Loadable from one helper.** Tests use `codexbar_test_fixtures::load(provider, name)`
   so the relative path lives in one place.
4. **Realistic, not synthetic.** Captures come from a real `claude usage` or
   `claude /usage` invocation against a test account, then sensitive fields are surgically
   replaced with placeholder values. The provider documentation in
   `docs/windows/spec/4*.md` records the exact capture command for the maintainer.
5. **Every status code from the section 4 list above is present.** Tests assert that the
   provider returns the correct typed error for each.

### 4.2 Test pattern

A provider fixture test looks like this:

```rust
#[tokio::test]
async fn claude_happy_path_parses_to_expected_snapshot() {
    let server = wiremock::MockServer::start().await;
    let body = codexbar_test_fixtures::load_str("claude", "http/200-usage.json");
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/organizations/test-org-id/usage"))
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;

    let provider = ClaudeProvider::with_base_url(server.uri());
    let snapshot = provider
        .fetch(&FetchContext::test_default())
        .await
        .expect("happy path returns snapshot");

    insta::assert_yaml_snapshot!(snapshot);
}
```

`insta` is the snapshot tool. Snapshot files live next to the test source. Reviewers can
diff snapshot changes in the PR.

---

## 5. ConPTY testing strategy

The Claude PTY parser is the most fragile component. The strategy: capture real `claude`
output, replay it through a deterministic harness, assert the parsed result.

### 5.1 Capture step (one time per fixture)

A maintainer runs:

```powershell
cargo run --release --bin codexbar-pty-capture -- `
  --cmd claude `
  --args "/usage" `
  --output rust/crates/codexbar-providers/tests/fixtures/providers/claude/cli/usage-happy.ansi
```

The capture binary uses ConPTY (`CreatePseudoConsole`) to spawn the child, records every
byte the child writes to its pty, including escape sequences, into a binary file.

### 5.2 Replay harness

A small `replay_pty.exe` reads a `.ansi` file and writes it back into a parent's child pty.
The parent under test (the Claude provider) cannot tell the difference between this and a
real `claude` invocation.

```rust
#[tokio::test]
async fn claude_pty_happy_replay() {
    let fixture = codexbar_test_fixtures::path("claude", "cli/usage-happy.ansi");
    let replay = ReplayCommand::new(&fixture);
    let parser = ClaudePtyParser::new();
    let result = parser.run(replay).await.unwrap();
    assert_eq!(result.usage.session.used_percent, 67.0);
}
```

### 5.3 Mock cmd.exe scripts

For providers that read from `cmd.exe` style child processes (e.g., `where claude`,
`codex --version`), the harness ships small batch scripts under `tests/fixtures/scripts/`
that print pre captured output and exit with a controlled code. Example:

```bat
@echo off
:: scripts/claude-not-found.bat
echo INFO: Could not find files for the given pattern(s).
exit /b 1
```

The provider tests set `PATH` to the fixtures scripts directory so that calls to `claude`
resolve to the batch script instead of a real install.

### 5.4 Determinism rules

- Replays never sleep. The harness writes bytes as fast as the consumer reads.
- Timestamps in fixtures are anchored to a fixed `captured_at`. Tests pin the system clock
  via `mock_instant::MockClock`.
- No randomness. If the parser sees randomness, the harness seeds a known RNG.

---

## 6. Secret handling in tests

The single rule: real secrets never reach the repo. The rules below make that automatic.

### 6.1 Test prefixes

Every secret in a fixture uses one of these prefixes:

| Real shape | Test shape | Where used |
|------------|------------|------------|
| `sk-ant-...` | `sk-ant-test-...` | Claude OAuth fixtures |
| `sk-...` | `sk-test-...` | OpenAI / OpenRouter |
| `Bearer eyJ...` | `Bearer test-eyJ...` | generic JWT bearers |
| `Authorization: Basic ...` | `Authorization: Basic test-...` | Copilot, Gemini |
| cookies (`sessionKey=...`) | `sessionKey=test-cookie-...` | Claude web |
| OAuth refresh tokens | `refresh-test-...` | all OAuth providers |

### 6.2 Pre commit hook

`scripts/git/pre-commit.ps1` (installed by `cargo xtask init-hooks`) runs the regex set in
`scripts/ci/secret-patterns.json` against the staged diff. The regex set:

```json
{
  "patterns": [
    "sk-ant-(?!test-)[A-Za-z0-9_-]{20,}",
    "sk-(?!test-)[A-Za-z0-9]{32,}",
    "Bearer eyJ(?!test-)[A-Za-z0-9._-]{20,}",
    "ghp_[A-Za-z0-9]{36,}",
    "gho_[A-Za-z0-9]{36,}",
    "AIza[A-Za-z0-9_-]{30,}",
    "xox[baprs]-[A-Za-z0-9-]{10,}",
    "-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
    "[A-Za-z0-9+/]{43}=\\s*$"
  ],
  "ignore_paths": [
    "docs/**",
    "*.lock",
    "scripts/ci/secret-patterns.json"
  ]
}
```

### 6.3 CI mirror

The pre commit hook is a developer convenience. CI re runs the same check
(`scripts/ci/check-fixture-secrets.ps1`) so even commits made without the hook fail at the
tier 1 gate.

### 6.4 Test logs

Every test that exercises a code path which logs sets a tracing subscriber that captures
output to a `Vec<String>`. The test asserts the captured output matches **none** of the
secret patterns above. If a test ever logs a real cookie or a real OAuth token, it fails.

```rust
#[test]
fn redaction_never_logs_session_key() {
    let log = TestLog::install();
    let _ = ClaudeProvider::with_session_key("test-cookie-deadbeef")
        .fetch_blocking()
        .ok();
    log.assert_no_match(&codexbar_test_security::SECRET_PATTERNS);
}
```

---

## 7. Cross platform CI plan

`windows-2022` is the primary target. Windows on ARM ships at GA but is built via cross
compile from x64 until GitHub Actions ships an `windows-11-arm` runner.

| Runner | Purpose | Tier | Job count |
|--------|---------|------|-----------|
| `windows-2022` | All tray, popup, ConPTY, DPAPI, signing tests | 1, 2, 3 | majority |
| `ubuntu-latest` | Portability run for `codexbar-core` and `codexbar-providers` only (features = `cross-platform-mock`); catches use of Windows specific syscalls in shared logic | 2 | 1 |
| `ubuntu-latest` | Docs link check, security audit (`cargo audit`, `npm audit`) | 2, 3 | 2 |
| `macos-latest` | **Out of scope at v1.** Reserved for v2 if cross platform launches. | n/a | 0 |

The Linux core run uses the `cross-platform-mock` cargo feature to replace any Windows
only crate (`windows`, `winreg`, `windows-sys`) with stub modules that return `unimplemented`.
This catches the case where a hot path is gated on a Windows API instead of on a trait,
which would block a future macOS or Linux port.

---

## 8. Performance regression budgets

Budgets are stored as JSON. The perf harness asserts against them. Failing the budget fails
the CI job.

### 8.1 `perf/budgets.json`

```json
{
  "$schema": "./budgets.schema.json",
  "version": 1,
  "captured_on": "windows-11-24h2-x64",
  "runner": "windows-2022",
  "budgets": {
    "idle_ram_mb": {
      "p50_max": 70,
      "p95_max": 95,
      "sample_seconds": 60,
      "source": "spec 80 section 0, line 'idle RAM under 70 MB'"
    },
    "cold_start_ms": {
      "p50_max": 800,
      "p95_max": 1200,
      "samples": 5,
      "source": "spec 80 polish item 1 'no white flash on launch'"
    },
    "click_to_popup_ms": {
      "p50_max": 100,
      "p95_max": 160,
      "samples": 25,
      "source": "spec 80 polish item 16 'click to popup latency under 100 ms'"
    },
    "refresh_cpu_percent_spike_500ms": {
      "p50_max": 2.0,
      "p95_max": 4.0,
      "samples": 10,
      "source": "spec 80 perf budgets"
    },
    "tray_redraw_ms": {
      "p50_max": 8,
      "p95_max": 16,
      "samples": 30,
      "source": "spec 80 section 1 30 Hz loading cadence"
    },
    "popup_open_animation_ms": {
      "p50_max": 180,
      "p95_max": 220,
      "samples": 25,
      "source": "spec 80 polish item 17 popup open ease out expo 180 ms"
    },
    "popup_close_animation_ms": {
      "p50_max": 140,
      "p95_max": 180,
      "samples": 25,
      "source": "spec 80 polish item 17 popup close ease in 140 ms"
    },
    "settings_round_trip_ms": {
      "p50_max": 10,
      "p95_max": 30,
      "samples": 50,
      "source": "spec 50 settings change watcher"
    }
  }
}
```

### 8.2 Budget schema

`perf/budgets.schema.json` is a JSON Schema that the perf harness validates against on
startup. Every budget key declares a `p50_max` and `p95_max`. The harness fails the build
if any actual measurement exceeds the corresponding `p95_max`, or if the p50 exceeds the
`p50_max` by more than 15 percent (to absorb runner jitter).

### 8.3 Trend tracking

The perf harness uploads `perf-report.json` as a workflow artifact. A nightly job in
tier 2 appends to `perf/history.jsonl` and posts a delta comment on the latest PR. A
regression of more than 10 percent for two consecutive nights opens an issue.

---

## 9. Manual test scripts

`docs/windows/test-plans/` holds one Markdown file per release. The file is a checklist.
The tester ticks each box, writes notes for anything that needs follow up, and signs the
bottom of the file in a PR.

### 9.1 Per release script template

`docs/windows/test-plans/release-<version>.md`:

```markdown
# Manual test plan v<version>

Tester: <name>
Date: <YYYY-MM-DD>
Builds tested: x64, arm64

## Install fresh

- [ ] Download `CodexBar-<version>-x64.exe` from the release page.
- [ ] Double click. Confirm Authenticode publisher reads `CodexBar` (no SmartScreen
      "Don't run" block on EV cert path; OV cert shows warning, click "More info" then
      "Run anyway").
- [ ] Installer completes without errors. Tray icon visible.
- [ ] `where codexbar.exe` returns the install dir.

## Sign in to each provider

- [ ] Claude: open Preferences > Providers > Claude. Click "Sign in". WebView2 opens
      claude.ai. Sign in. Window closes, account email shows in card.
- [ ] Codex: open Preferences > Providers > Codex. Repeat.
- [ ] Cursor: enter API key from cursor.com/settings. Card populates within 60 s.
- [ ] Copilot: OAuth device flow. Code shown, paste into github.com/login/device.
- [ ] Gemini: API key path.
- [ ] OpenRouter: API key path.
- [ ] Factory: API key path.

## Observe icon

- [ ] Tray icon shows brand mark with bars after first refresh.
- [ ] At 30 Hz the bars do not flicker. Stare for 10 seconds.
- [ ] Stale state: pause network for 5 minutes. Icon dims to 55% alpha.
- [ ] Quota warning flash: trigger a synthetic threshold crossing via Preferences > Debug
      > Force threshold notification. Icon turns red tinted for 60 s.

## Observe popup

- [ ] Click tray icon. Popup opens with 180 ms ease out expo. translateY direction
      matches taskbar edge.
- [ ] Click outside popup. Closes with 140 ms ease in.
- [ ] Re click tray icon while open. Toggles closed then open.
- [ ] Press Esc. Closes.
- [ ] Tab through every focusable element. Focus ring visible on each.
- [ ] Right click tray icon: native menu opens with Refresh now, Show window, About,
      Preferences, Quit Ctrl+Q.

## Test threshold notification

- [ ] In Preferences > Notifications, set threshold to 95% for one provider.
- [ ] In Preferences > Debug, click "Force threshold notification".
- [ ] Toast appears with provider name and threshold percent.
- [ ] Click toast. Popup opens, focused on that provider's card.
- [ ] Snooze 1h button: click. Threshold marker disables for one hour.

## Test weekly reset celebration

- [ ] In Preferences > Debug, click "Force weekly reset celebration".
- [ ] Tray icon plays 1.5 s unbraid morph.
- [ ] Toast with hero image appears, body reads "<Provider> · You have a fresh week".
- [ ] If popup was open at time of trigger, in popup confetti plays for 1.2 s anchored
      to provider card header.

## Test update flow

- [ ] In Preferences > About, click "Check Now". If no update, "You're up to date"
      shows.
- [ ] Install older version. Launch. Popup banner appears: "CodexBar <new> is available".
- [ ] Click "Update now". Tauri updater downloads and verifies minisign signature.
- [ ] Click "Relaunch Now". App restarts on the new version. Confirm new version
      string in Preferences > About.

## Accessibility smoke

- [ ] Narrator on (Win+Ctrl+Enter). Walk every focusable element in the popup. Each
      reads its label and role.
- [ ] Settings > Display > Animation effects: off. Reopen popup. No fade or slide.
- [ ] Settings > Accessibility > Contrast themes: turn on a high contrast theme.
      Popup repaints without Mica or Acrylic, using system colors.

## Voice and tone

- [ ] No exclamation marks anywhere except the About tagline.
- [ ] No emojis in app strings.
- [ ] Middle dot separators `·` present in compound labels.

## Sign off

- [ ] All checks above are green or explicitly noted.

Signed: <name>, <date>
```

### 9.2 Per phase manual script

A shorter checklist per phase under `docs/windows/test-plans/phase-<N>.md` covers only the
behavior introduced in that phase. The release script supersedes them at tag time.

---

## 10. Telemetry policy

The app is privacy first. The default for every user is **opt out of all telemetry**. A
toggle in Preferences > About > Privacy can opt in to crash reports. No usage analytics
ship at all.

### 10.1 What is collected (opt in only)

| Channel | Tool | What | Retention |
|---------|------|------|-----------|
| Crash report | `sentry-rust` | Stack traces, OS version, app version, anonymised user id (hash of install GUID) | 30 days |
| Crash report | `@sentry/react` | JS stack traces, browser version (WebView2 build), app version | 30 days |

### 10.2 What is never collected

- Provider names that the user has configured.
- Cookie values, tokens, or any secret material.
- Path names that contain the user's name.
- Telemetry pings on launch, refresh cadence, click counts, popup open counts.
- Time of day or location.

### 10.3 Redaction

Before any payload leaves the device, a scrubber runs over the JSON body:

```rust
fn scrub(value: &mut serde_json::Value) {
    if let Some(s) = value.as_str() {
        let scrubbed = SECRET_PATTERNS.iter().fold(s.to_string(), |acc, pat| {
            pat.replace_all(&acc, "<redacted>").into_owned()
        });
        *value = serde_json::Value::String(scrubbed);
    }
    // recurse for arrays and maps
}
```

A test asserts the scrubber against a fixture of synthetic stack traces containing every
secret prefix from section 6. If the scrubber fails to redact a known pattern, the test
fails and the telemetry plugin is excluded from the release.

### 10.4 Opt in UX

A single toggle in Preferences > About > Privacy:

```
[ ] Send anonymous crash reports
    Helps us fix crashes you hit. We never collect usage data, never collect
    secrets, and you can turn this off any time. Reports retain for 30 days.
```

Off by default. Toggle state lives in `config.json` under `telemetry.crash_reports`. The
sentry client never initializes if the toggle is false.

---

## 11. Pre release checklist

The final manual gate before tagging `v1.0.0`. Tester signs at the bottom.

```markdown
# Pre release checklist v1.0.0

Tester: <name>
Date: <YYYY-MM-DD>

## Phase gates

- [ ] Tier 1 CI green on main for the commit being tagged.
- [ ] Tier 2 CI green within last 24 hours.
- [ ] All ten phase plans' acceptance sections pass on this branch.
- [ ] `docs/windows/plan/test-strategy.md` section 2 gate command runs green for phase 9.

## Manual run

- [ ] Manual test plan in `docs/windows/test-plans/release-1.0.0.md` complete and signed.
- [ ] All v1 providers tested: Claude, Codex, Cursor, Copilot, Gemini, OpenRouter, Factory.
- [ ] Threshold notification works on at least one provider with a real account.
- [ ] Weekly reset celebration triggered via debug menu and observed.
- [ ] Update flow tested from an older signed build.

## Build hygiene

- [ ] `signtool verify /pa /v` succeeds on every shipped binary.
- [ ] `latest.json` minisign signature verifies with `minisign -Vm latest.json -p .github/keys/updater.pub`.
- [ ] Installer SHA256 in winget manifest matches the GitHub release asset.
- [ ] `version.env` matches the git tag.
- [ ] `CHANGELOG.md` top entry matches the git tag and is dated.

## Compliance

- [ ] No real secrets in the repo (`scripts/ci/check-fixture-secrets.ps1` clean).
- [ ] `cargo audit` and `npm audit` have no high or critical findings.
- [ ] `LICENSE` and `SECURITY.md` referenced from `README.md` and resolve.
- [ ] Privacy toggle in About > Privacy reads "off by default".

## Polish acceptance

- [ ] Every polish item in `docs/windows/spec/80-feel-and-polish.md` section 20 is
      ticked on its source code location in a comment, or has an open issue tracked
      to v1.1.

## Final

- [ ] Tag `v1.0.0` pushed.
- [ ] Tier 3 CI green for the tag.
- [ ] Release notes published.
- [ ] Winget submission queued.

Signed: <name>, <date>
```

---

## 12. Coverage map

A traceability matrix that the doc owner refreshes once per phase. The columns map every
spec checkpoint to the test or test script that exercises it.

| Spec section | Item | Test artifact |
|--------------|------|----------------|
| 50 section 2 | refresh loop fan out | `rust/crates/codexbar-core/tests/refresh_loop.rs::fans_out_to_enabled_providers` |
| 50 section 3 | concurrency: single mutex never held across await | `clippy::await_holding_lock` lint on tier 1 |
| 50 section 4 | UsageStore state shape | `cargo test -p codexbar-core usage_store::serde_round_trip` |
| 50 section 11 | OpenAI dashboard scrape | `codexbar-providers tests::openai_dashboard_fixture` |
| 80 section 1 | 30 Hz loading cadence | `perf/budgets.json tray_redraw_ms` |
| 80 section 2 | six loading patterns | `cargo test -p codexbar-tray loading_pattern::value_at_phase` |
| 80 section 3 | quota flash 60 s | `cargo test -p codexbar-tray quota_flash_duration` |
| 80 section 4 | celebration trigger filter | `cargo test -p codexbar-core celebration::triggers_only_if_utilization_above_one_percent` |
| 80 section 5 | bar fill tween popup | Playwright `popup-bar-fill.spec.ts` |
| 80 section 6 | pace text fade cross | Playwright `pace-fade-cross.spec.ts` |
| 80 section 7 | popup open close timings | `perf/budgets.json popup_open_animation_ms` |
| 80 section 8 | tab indicator slide | Playwright `tab-indicator-slide.spec.ts` |
| 80 section 9 | hover and press timings | `vitest motion-constants.test.ts` |
| 80 section 10 | copy flash 900 ms hold | `vitest copy-flash.test.ts` |
| 80 section 11 | threshold dedup | `cargo test -p codexbar-notifications threshold_dedup` |
| 80 section 12 | sound matrix | `cargo test -p codexbar-notifications sound_matrix_with_focus_assist` |
| 80 section 13 | updater UX | Playwright `updater-banner.spec.ts` |
| 80 section 14 | first run | Playwright `first-run.spec.ts` |
| 80 section 15 | empty and error states | Playwright `empty-state.spec.ts` and `error-state.spec.ts` |
| 80 section 16 | accessibility tree | tier 3 a11y audit |
| 80 section 17 | reduced motion | Playwright `reduced-motion.spec.ts` |
| 80 section 18 | DND matrix | `cargo test -p codexbar-notifications focus_assist_matrix` |
| 80 section 19 | voice rules | manual script section "Voice and tone" |
| 80 section 20 polish 1 to 64 | individual polish items | mapped row by row in `docs/windows/test-plans/polish-traceability.md` |
| 90 section A | CLI flags | `cargo test -p codexbar-cli flag_parsing` |
| 90 section C | watchdog kill on parent death | `cargo test -p codexbar-watchdog job_object_kill_on_close --ignored` (runs in tier 2) |
| 90 section D | localization | `cargo test -p codexbar-locale fallback_chain` and `vitest i18n.test.ts` |
| 90 section E | installer | tier 3 `sign-verify` and manual install script |

A row that has no test artifact is a coverage gap. Section 13 lists known gaps.

---

## 13. Known coverage gaps

Identified while drafting this plan. Each gets an issue at the start of the relevant phase.

1. **WebView2 version drift.** WebView2 ships on the user's machine and updates outside our
   release cadence. Our tier 2 e2e tests pin a WebView2 version (the runner default). A
   real user might see motion or font rendering bugs on a newer WebView2. Mitigation: a
   monthly tier 2 run on a runner with the latest WebView2 evergreen channel. Tracked in
   phase 9 follow up.
2. **Multi monitor DPI mix.** GitHub Actions runners are single monitor. The popup anchor
   math under per monitor DPI cannot be tested in CI. Mitigation: dedicated manual section
   in `docs/windows/test-plans/release-*.md` and a smoke script the tester runs on a
   physical multi monitor setup before tag. Tracked in phase 2 plan.
3. **Real provider API drift.** Our fixtures freeze a provider's response shape. A real
   API change can break the live app without breaking CI. Mitigation: a weekly tier 2 job
   (deferred until phase 5) that hits each provider with a maintainer test account and
   compares the shape to the fixture. Out of scope at v1 because it requires running
   secrets in CI. Tracked as v1.1.
4. **Focus Assist matrix in CI.** `QueryUserNotificationState` returns `QUNS_ACCEPTS` on
   a GitHub Actions runner. We cannot reproduce `QUNS_QUIET_TIME` or `QUNS_PRESENTATION`.
   Mitigation: unit test the decision tree by mocking the return value; manual test
   covers the real OS path.

---

## 14. Acceptance checklist for this document

A self check. Every box must be ticked before this doc is merged.

- [ ] Every phase 0 through 9 has a row in section 2 listing its gate command.
- [ ] Three CI tiers exist, each with full YAML under sections 3.1, 3.2, 3.3.
- [ ] Tier 1 YAML includes `cargo fmt`, `clippy -D warnings`, `cargo test`, `tsc --noEmit`,
      `npm run lint`, `vitest`, and `tauri build --debug`.
- [ ] Tier 2 YAML includes `tauri build --release`, Playwright, performance smoke,
      Linux core portability run.
- [ ] Tier 3 YAML includes full Playwright matrix, accessibility audit, security audit,
      signing verification, updater minisign verification.
- [ ] Provider fixtures contract in section 4 lists the directory layout and the five
      mandatory HTTP status fixtures (200, 401, 403, 429, 500).
- [ ] ConPTY harness in section 5 names a capture command and a replay command.
- [ ] Secret handling rules in section 6 list test prefixes, regex patterns, pre commit
      hook, and CI mirror.
- [ ] Cross platform CI plan in section 7 names `windows-2022` as primary and
      `ubuntu-latest` for the Linux core run.
- [ ] Performance budgets file format in section 8 maps every number in spec 80 to a key.
- [ ] Manual test scripts folder `docs/windows/test-plans/` is defined and a template
      exists in section 9.
- [ ] Telemetry policy in section 10 names `sentry-rust` and is opt in.
- [ ] Pre release checklist in section 11 has phase gates, manual run, build hygiene,
      compliance, polish, and final sign off blocks.
- [ ] Coverage map in section 12 lists every spec 80 polish section.
- [ ] Known coverage gaps in section 13 are enumerated.
- [ ] No em dashes or single dashes appear in prose.

End of test strategy.
