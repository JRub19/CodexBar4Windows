---
summary: "Performance budgets, profiling tools, and workload model for CodexBar4Windows. Phase 9 §A baseline."
read_when:
  - Investigating a perf regression
  - Onboarding to the perf-sensitive parts of the codebase
  - Updating budgets after a release
---

# Performance baseline

This document is the **single source of truth** for what "fast enough" means on Windows. Any PR that changes one of the budgets must update this doc in the same commit.

The budgets are tuned for **Windows 11 24H2 on a typical developer laptop** (8-core / 16 GB / NVMe). They're floors for shipping, not ceilings; faster is always better.

## 1. Workload model

CodexBar4Windows runs in two modes the user toggles between:

| Mode | Trigger | Refresh cadence | Expected duty cycle |
|---|---|---|---|
| **Foreground** | Popup is open (visible to the user) | Driven by `RefreshFrequency` (default: 5 min) | <2% CPU avg, <1% idle |
| **Background** | Popup hidden, tray icon only | Same cadence | <0.5% CPU avg over 5 min |
| **Suspended** | Popup hidden + Manual cadence (no auto-refresh) | None | ~0% CPU; no disk writes for 30s after launch |

The refresh-loop bursts to ~5-15% CPU for ~200 ms per provider during a refresh. With seven providers that's a one-second flurry every cycle. The budgets below are for the **steady state between flurries**, not the flurries themselves.

## 2. The six budgets

| # | Budget | Target | Failure mode |
|---|---|---|---|
| **1** | **Cold-launch tray paint** | <500 ms from process start to `Shell_NotifyIcon NIM_ADD` | White-flash icon visible during boot |
| **2** | **Steady-state RSS (background)** | <80 MB total (Rust core + WebView2) | Memory pressure on low-RAM laptops |
| **3** | **CPU at idle, popup hidden** | <0.5% avg over 5 min | Battery drain |
| **4** | **WebView2 footprint when popup hidden** | <50 MB RSS, ~0% CPU (after suspend kicks in) | The biggest single perf risk |
| **5** | **Disk IO at Manual cadence** | 0 bytes written for 30 s after launch | Power user disabled refresh; respect it |
| **6** | **GDI handle count** | <100 handles steady-state | Handle leak triggers Windows-wide UI corruption |

### Why these specifically?

- **Budget 1** ships from spec 80 §1 (no white flash on cold launch). The tray icon must be rendered synchronously in Rust before `NIM_ADD` is called, never relying on an async paint.
- **Budgets 2-4** are the WebView2-related triad. WebView2 is the largest single dependency in the bundle (~120 MB DLL footprint) and the easiest place to bleed memory + CPU. Suspending the process when the popup is hidden is the only way to hit Budget 4.
- **Budget 5** honours the explicit user intent of selecting Manual cadence. Power users (laptop on battery, lab machines, kiosks) pick Manual precisely to stop the refresh-write-refresh cycle; we must respect that.
- **Budget 6** is the silent killer — GDI handle leaks via the tray-icon rebuild path are notorious. A single missed `DeleteObject` per refresh adds up to hundreds in an 8-hour day.

## 3. Profiling tools

The Windows perf stack we use:

| Tool | What it answers | When to reach for it |
|---|---|---|
| **Windows Performance Recorder (WPR)** + **Windows Performance Analyzer (WPA)** | CPU sampling, disk IO, GPU usage | Steady-state perf regressions |
| **PerfView** | .NET-friendly ETW + GC events | Memory growth investigations |
| **Process Explorer** (Sysinternals) | Handle counts, RSS over time, child processes | First-pass triage |
| **Snapshot via Task Manager** | Quick gut-check | Sanity-check after a change |
| **xperf / xperfview** | Specific ETW providers | Targeted boot-trace work |
| **tauri-inspector** | WebView2-side perf timeline | React-side regression |

### Standard ETW capture for a regression

```powershell
# 1. Start a capture session focused on CPU + Disk + Memory.
wpr -start CPU -start DiskIO -start Heap -filemode

# 2. Reproduce the regression (e.g. open popup, wait 60s, close).

# 3. Stop + save.
wpr -stop trace.etl

# 4. Open in WPA, drag in trace.etl, focus on the codexbar4windows-desktop.exe
#    process across all four views.
```

Compare against a known-good baseline from a prior release: keep the
`.etl` files for the last three GA versions under `perf-baselines/`
(gitignored — too big to commit; archive on a network share).

## 4. Boot timeline (Budget 1)

The current cold-launch sequence:

| Step | Owner | Budget | Notes |
|---|---|---|---|
| 1. Process start | OS | n/a | First instruction in `main()` |
| 2. Logging init | Rust core | <20 ms | `tracing_subscriber` with file-rotated writer |
| 3. Settings load | Rust core | <30 ms | Read + parse `config.json` (<2 KB typically) |
| 4. Tray icon render | Rust core | <50 ms | **Synchronous**: produce ICO bytes before NIM_ADD |
| 5. `Shell_NotifyIcon NIM_ADD` | Rust core | <100 ms | Win32 call into shell32.dll |
| 6. Tauri builder | Tauri | <300 ms | Plugin registration, manager state |
| 7. WebView2 spawn | WebView2 | (async, off the critical path) | Hidden window — no paint yet |

**Total budget: 500 ms from step 1 to step 5.** The user sees the icon appear in the tray. Steps 6-7 finish lazily; the popup is invisible at this point.

If step 4 takes >50ms on a release build, the icon-render path has regressed — look first at `rust/src/renderer/` (likely cache invalidation gone wrong).

## 5. Verifying the budgets

After each release tag (or whenever a perf-sensitive PR lands), run:

```powershell
# Budget 1: cold-launch tray paint
.\target\release\codexbar4windows-desktop.exe --measure-cold-launch
# (--measure-cold-launch is a debug flag that prints the timeline)

# Budgets 2-3-4: steady-state RSS / CPU / handle count
# Open Process Explorer, add Working Set + GDI columns, watch for 5 min

# Budget 5: disk IO at Manual cadence
# Set RefreshFrequency=Manual in settings, restart, then:
Get-Process codexbar4windows-desktop |
  Select-Object Name,
                @{N="DiskBytesRead";E={(Get-Counter "\Process(codexbar4windows-desktop)\IO Read Bytes/sec").CounterSamples.CookedValue}},
                @{N="DiskBytesWritten";E={(Get-Counter "\Process(codexbar4windows-desktop)\IO Write Bytes/sec").CounterSamples.CookedValue}}
# Both columns must read 0 for 30 seconds after launch.
```

A future iteration will land `scripts/measure-perf-baseline.ps1` that batches all six checks into one report.

## 6. Reference

- `docs/windows/spec/80-feel-and-polish.md` — the polish bar this document defends.
- `docs/windows/plan/phase-9-release.md` §A — the original budget rationale.
- `rust/src/renderer/cache.rs` — tray-icon ICO cache, the hot path for Budget 1.
- `apps/desktop-tauri/src-tauri/src/perf.rs` — runtime perf counters surface.
