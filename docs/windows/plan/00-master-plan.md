---
summary: "Master execution plan: 10 phases, 297 atomic commits, critical path, parallel tracks, gates."
read_when:
  - Daily standup, sprint planning, status reporting
  - Sequencing the next concrete task
  - Deciding what to parallelize
---

# CodexBar4Windows: Master Execution Plan

This document is the single entry point for executing the CodexBar4Windows refactor. It stitches together 10 phase plans plus a cross phase test strategy into a coherent sequence with critical path, parallel tracks, gates between phases, and ownership of open questions.

The 14 subsystem blueprints in [../spec/](../spec/) are the behavioral source of truth. The 10 phase plans in this folder are the execution sequence. This master is the navigation layer.

## 1. The plan at a glance

| # | Phase | Doc | Lines | Tasks | Solo days | Headline deliverable |
|---|---|---|---:|---:|---:|---|
| 0 | Bootstrap | [phase-0-bootstrap.md](phase-0-bootstrap.md) | 442 | 22 | 2 to 4 | Green CI, Tauri tray app boots from a clone |
| 1 | Foundations | [phase-1-foundations.md](phase-1-foundations.md) | 523 | 12 | 8 | Config dir, logging, settings, refresh skeleton, IPC contract |
| 2 | Auth subsystem | [phase-2-auth.md](phase-2-auth.md) | 726 | 19 | 11 | DPAPI, Credential Manager, cookie readers, v20 manual fallback |
| 3 | Tray + popup | [phase-3-tray-popup.md](phase-3-tray-popup.md) | 1200 | 44 | 47 | Dynamic icon renderer plus Mica popup with mock cards |
| 4 | Provider framework + Claude | [phase-4-provider-framework-claude.md](phase-4-provider-framework-claude.md) | 869 | 20 | 5 to 7 (FT) | First real provider, three auth paths, watchdog binary |
| 5 | Codex | [phase-5-codex.md](phase-5-codex.md) | 1380 | 44 | 33 to 45 | OAuth, CLI, dashboard scrape, account promotion state machine |
| 6 | Tier 1 cohort | [phase-6-tier1-cohort.md](phase-6-tier1-cohort.md) | 1472 | 37 | 26 | Cursor, Copilot, Gemini, OpenRouter, Factory live |
| 7 | Cost, status, notifications | [phase-7-cost-status-notifications.md](phase-7-cost-status-notifications.md) | 1199 | 34 | 16 to 22 | JSONL scanner with pricing, status overlay, toast pipeline |
| 8 | Prefs, onboarding, hotkeys | [phase-8-prefs-onboarding-hotkeys.md](phase-8-prefs-onboarding-hotkeys.md) | 821 | 33 | 15 to 23 | Seven pane Mica prefs window, first run flow, Win+Shift+U |
| 9 | Polish, packaging, release | [phase-9-release.md](phase-9-release.md) | 1160 | 32 | 28 | Inno installer, Authenticode, updater, Winget, v1.0.0 |
| T | Test strategy | [test-strategy.md](test-strategy.md) | 1160 | rolling | rolling | Per phase gates, three CI tiers, 1100 unit tests by GA |

Plan corpus: **10,952 lines, 297 atomic commit tasks** across 10 phases plus the cross phase test layer.

## 2. Phase dependency graph

```
Phase 0 (Bootstrap)
   |
   v
Phase 1 (Foundations)
   |
   +---+
   |   |
   v   v
Phase 2 (Auth)        Phase 3 (Tray + Popup)
        \                /
         \              /
          v            v
          Phase 4 (Framework + Claude)
                 |
        +--------+--------+----------+
        |                 |          |
        v                 v          v
   Phase 5 (Codex)   Phase 6      Phase 7 (Cost,
                    (Tier 1)      Status, Notif.)
        \                |          /
         \               |         /
          v              v        v
                Phase 8 (Prefs, Onboarding, Hotkeys)
                                   |
                                   v
                          Phase 9 (Release v1.0)
```

Notes:

- Phases 2 and 3 can run in parallel after Phase 1.
- After Phase 4 lands the framework plus Claude, Phases 5, 6, 7 can run in parallel by different engineers (or by one engineer in interleaved sprints).
- Phase 8 requires every provider to exist (so per provider settings panes can be populated).
- Phase 9 requires everything else green.

## 3. Critical path (single engineer)

The longest dependency chain dictates the calendar floor. Numbers in solo engineering days at full time:

```
0 (3) -> 1 (8) -> 3 (47) -> 4 (6) -> 5 (39) -> 7 (19) -> 8 (19) -> 9 (28)
```

Sum on the critical path: **169 days, roughly 8 calendar months at full time, 16 months at 50 percent**. Phase 6 (Tier 1 cohort, 26 days) is **off the critical path** because the popup card stack from Phase 3 already supports new providers; tier 1 can land between or alongside Phases 5 and 7 without delaying GA.

If Phase 3 (47 days) is the long pole, you have three levers:

1. **Two engineers on Phase 3.** Renderer (Rust, tiny skia) on one track, popup UI (React, components) on the other. Cuts Phase 3 to 28 to 30 days. Saves about 17 days off the critical path.
2. **Defer some Phase 3 polish to Phase 9.** Charts and animation variants can ship at v1.1. Cuts Phase 3 to about 30 days.
3. **Both.** Cuts Phase 3 to about 18 to 20 days.

## 4. Parallel track view (two engineers)

After Phase 1 lands:

| Calendar week | Engineer A | Engineer B |
|---|---|---|
| 2 to 3 | Phase 2 auth subsystem | Phase 3 renderer (Rust half) |
| 4 to 7 | Phase 4 framework + Claude | Phase 3 popup UI (React half) |
| 8 to 12 | Phase 5 Codex | Phase 6 Tier 1 cohort |
| 13 to 15 | Phase 7 cost, status, notifications | Phase 8 prefs + onboarding (parallel start) |
| 16 to 19 | Phase 8 continued + Phase 9 packaging | Phase 9 polish + beta |
| 20 | GA v1.0.0 |

Calendar duration with two engineers at full time: **about 5 calendar months**. With both at 50 percent: roughly 10 calendar months.

## 5. Total time and staffing options

| Staffing | Calendar to GA | Notes |
|---|---|---|
| 1 engineer at 50 percent | 14 to 18 months | The default "side project" cadence. |
| 1 engineer at 100 percent | 8 to 10 months | Solo focused. Most realistic for a passion port. |
| 2 engineers at 100 percent | 5 to 6 months | Renderer track plus framework track unlocks Phase 3. |
| 2 engineers, one shoulders Codex full time | 4 to 5 months | Codex is the biggest single phase. Isolating it parallelizes well. |

The plan does not assume employees. Solo full time is the budgeted baseline.

## 6. Test gates between phases

Each phase has its own acceptance section. The cross phase test strategy at [test-strategy.md](test-strategy.md) defines the gates between phases. Summary of the gates:

| Gate | Before phase | Must pass |
|---|---|---|
| G0 to G1 | Phase 1 | `npm run tauri dev` opens tray icon, CI tier 1 green, branch protection live |
| G1 to G2 or G3 | Phase 2 or 3 | Settings persist round trip, log file rotates, refresh tick fires with zero providers, IPC contract typed end to end |
| G2 to G4 | Phase 4 | DPAPI round trip, Chrome cookie import succeeds on dev machine, manual paste path works, v20 detection surfaces clear error |
| G3 to G4 | Phase 4 | Icon renders at 16 to 64 px theme aware, popup opens next to tray rect at less than 100 ms, six loading patterns visually verified |
| G4 to G5 to G6 | Phase 5, 6 | Claude session and weekly bars live in tray, identity siloing enforced on writes, per strategy timeout wraps every fetch |
| G5 to G7 | Phase 7 | All Codex promotion decision matrix cells exercised, multi account scenario works |
| G6 to G7 | Phase 7 | All seven v1 providers show live data simultaneously |
| G7 to G8 | Phase 8 | Cost numbers within one cent of Mac on a reference dataset, status overlay shows on a simulated incident, toast fires on threshold |
| G8 to G9 | Phase 9 | Brand new user installs and is set up in under five minutes without docs |
| G9 to GA | Release | Inno installer signed, Tauri updater verifies signature, Winget manifest accepted, two week beta with zero unresolved crash reports |

The test strategy specifies the exact commands and fixture sets for each gate.

## 7. CI tier reminder

From [test-strategy.md](test-strategy.md):

- **Tier 1, every push and merge** (~7 to 8 min cold, 3 min warm): cargo fmt, clippy `-D warnings`, cargo test, tsc, lint, vitest, tauri build debug.
- **Tier 2, nightly on main** (~15 min): tauri build release, Playwright e2e, performance smoke against `perf/budgets.json`.
- **Tier 3, release tags only** (~35 min): full Playwright matrix, accessibility audit, security audit (cargo audit, npm audit), signing verification.

## 8. Top cross cutting risks (ranked)

Distilled from the spec inconsistency list in [../spec/00-index.md](../spec/00-index.md) plus every phase plan's risk section.

| Rank | Risk | First exposed in | Mitigation owner | Hard or soft |
|---|---|---|---|---|
| 1 | **Code signing cert procurement delay.** Without an EV or OV cert in hand by Phase 9 start, SmartScreen kills first install adoption. | Phase 9 | Project owner (legal entity, payment) | Hard |
| 2 | **Chrome v20 App Bound Encryption** on `claude.ai`, `chatgpt.com` cookies. Manual paste fallback is mandatory polish, not optional. | Phase 2, recurs in 4, 5 | Engineering | Soft (manual paste path is the contract) |
| 3 | **OpenAI dashboard DOM drift.** Codex web extras scraper is fragile. | Phase 5 | Engineering | Soft (fixtures + last good cache) |
| 4 | **Claude CLI TUI changes** break the ANSI parser. | Phase 4 | Engineering | Soft (fixtures + positional fallback) |
| 5 | **Gemini packaged JS layout drift** (Homebrew, npm, Nix, bun, fnm, bundle). | Phase 6 | Engineering | Soft (5 layouts + bundle walker) |
| 6 | **Per monitor DPI plus multi monitor popup anchor math** untestable in CI. | Phase 3 | Engineering (manual test) | Soft |
| 7 | **`inventory!` link time registration drop under LTO.** | Phase 1 | Engineering (`#[used]` shim + startup count assertion) | Soft |
| 8 | **Per strategy timeout missing** can stall the JoinSet for 60 seconds. | Phase 1, enforced in 4 | Engineering (45 s wrap) | Soft |
| 9 | **Mac source token plaintext deviation** plus Explorer browsable `config.json` requires DPAPI on Windows even though Mac does not. | Phase 2 | Engineering | Hard (security contract) |
| 10 | **WebView2 evergreen version drift in CI** cannot be locked; the runner pins one build. | Across phases | Engineering (manual diversity test) | Soft |
| 11 | **OS Glass sound timing** plays before silent toast so muting notifications does not silence quota warning. | Phase 7 | Engineering (rule preserved verbatim) | Soft |
| 12 | **Provider scope creep** beyond the seven v1 providers risks slipping GA. | Across phases | Project owner | Hard (defer to Phase 10 backlog) |

Hard risks block calendar; soft risks accept residual.

## 9. Open questions ranked by urgency

| When needed | Question | Owner | Default if no answer |
|---|---|---|---|
| Phase 0 start | Final identifier `com.codexbar4windows.app` confirmed | Project owner | Use the proposed identifier |
| Phase 0 end | Code signing cert procurement started | Project owner | Track as GitHub issue, do not block |
| Phase 0 | Branch protection require reviews? Solo maintainer trap | Project owner | Require CI only, not reviews |
| Phase 1 | MSRV pin in `rust-toolchain.toml` | Engineering | `stable` channel |
| Phase 2 | Accept Chrome v127+ requires manual paste? | Project owner | Yes, accept |
| Phase 2 | Keep `keyring` Credential Manager mirror? | Project owner | Keep (canonical is DPAPI file) |
| Phase 3 | Defer chart variants to v1.1? | Project owner | Defer secondary charts |
| Phase 4 | Drop web probe binary to v1.1? | Project owner | Keep diagnostic, drop if Phase 4 overruns |
| Phase 5 | Codex web extras toggle visible in UI? | Project owner | Yes, expose in Advanced |
| Phase 6 | StepFun password mode shipped at v1? (Note: StepFun is out of v1 scope but applies for v1.x) | Project owner | No, drop |
| Phase 7 | Status feed cadence: independent timer or ride usage tick? | Engineering | Ride usage tick (Mac parity) |
| Phase 7 | Sentry error reporter at v1? | Project owner | Yes, opt in, no analytics |
| Phase 8 | Default `Win+Shift+U` ok? Conflict with Windows accessibility hotkeys? | Project owner | Yes, but support rebind |
| Phase 8 | Launch at sign in via registry or MSIX startup task? | Project owner | Registry at v1 |
| Phase 9 | Stable plus Beta channels or just Stable? | Project owner | Both from launch |
| Phase 9 | Winget submission timing: at GA or post GA? | Project owner | At GA |
| Phase 9 | Microsoft Store listing? | Project owner | Defer to v1.1 |

The 12 hardest decisions are concentrated in Phases 0, 2, 9. None block the engineering work for more than a few days at a time.

## 10. How to use this plan day to day

The CLAUDE.md rules (atomic commits, conventional commit messages, push after every commit) make this plan a working checklist. Each phase doc lists tasks as a numbered series, each task is one commit. The right loop is:

1. Open the phase doc for the current phase.
2. Pick the next unstarted task.
3. Read its files list, acceptance check, and draft commit message.
4. Implement.
5. Verify the acceptance check.
6. Stage exactly the listed files.
7. Commit with the draft message.
8. Push.
9. Tick the task and move on.

When a phase ends:

1. Run that phase's acceptance test list.
2. Run the matching gate from section 6 above.
3. Tag a milestone in git (`v0.<phase>-checkpoint`).
4. Move to the next phase.

When CI fails:

1. Read the error.
2. Fix it.
3. Commit, push.
4. Do not wait for instructions, per CLAUDE.md.

When something is ambiguous between a phase plan and a spec doc:

1. Spec doc wins (it is derived from the Mac source).
2. Update the phase plan if the divergence is real.

## 11. Out of scope for this plan

- **Phase 10 (long tail providers).** The remaining 25 plus providers in `../spec/43-providers-catalog.md` ship after v1.0 GA. Tracked as a separate backlog.
- **Windows on ARM.** Tauri supports it. Defer until v1.1 demand.
- **Linux build.** Tauri supports it. Defer indefinitely; cookie decryption story varies per distro.
- **Mobile.** Not in scope.
- **Windows 11 Widgets board integration.** Not in scope at v1.
- **Microsoft Store listing.** Defer to v1.1.

## 12. Provenance

Generated 2026-05-12. Plan corpus derived from 14 subsystem blueprints in `../spec/` which themselves came from a parallel deep read of the macOS Swift sources at commit `009420a7`. Each phase plan was written by a dedicated agent, then committed atomically. The test strategy weaves through all phases.

The plan is living. When a phase plan diverges from reality during execution, update the plan in the same PR that delivers the divergence.
