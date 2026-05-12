---
summary: "The headline recommendation in one page. Read this first if you read nothing else."
read_when:
  - Briefing a stakeholder
  - Before approving the refactor direction
---

# 00 — Recommendation

## What I’d do

**Replace the Swift sources in this fork with a Tauri 2 + React + shared Rust core architecture, rebased on `Finesssee/Win-CodexBar`.**

In one sentence: don’t port Swift to Windows; adopt the architecture that the only working Windows port already converged on, keep the upstream Mac project as the *behavioral spec*, and own the result.

## Why, in five bullets

1. **The Mac code is unportable in the way you want.** ~30–40 of its ~250 Swift files are AppKit/Sparkle/Keychain/WidgetKit/Vortex/SwiftUI-in-NSMenu plumbing that has no Windows counterpart. Swift compiles on Windows but **AppKit and SwiftUI do not**, and that’s where most of the UI lives. The remaining ~70% (provider HTTP/JSON/cookies/CLI parsing) is logic that can be re-expressed in any language.

2. **Someone already did the hard part.** `Finesssee/Win-CodexBar` ships **40 providers**, a dynamic two-bar tray meter, DPAPI secret storage, Chromium cookie decryption, Tauri popup, ConPTY runner, signed installer, auto-updater — all under MIT, actively maintained (v0.25.1 on 2026-05-11, 392★, 0 open issues). Re-doing that is months of work for one engineer.

3. **The other Windows attempts confirm the choice.** Two C# ports stalled (one is a 3-commit stub, one wraps the CLI via WSL). The Python port has 2 providers. The Electron "inspired-by" project has 4 stars and 3 tested providers. **Every healthy build is web-tech-in-a-native-shell.** Tauri is the lightest, native-est member of that family.

4. **Behavior parity costs us only the audit.** Because both projects organize providers as one-folder-per-provider, you can diff `steipete/CodexBar:Sources/CodexBarCore/Providers/` against `Win-CodexBar:rust/src/providers/` directly and produce a parity matrix in an afternoon (see [02](02-existing-forks-analysis.md)). Any drift becomes a normal "land this feature in our Rust crate" PR.

5. **The user’s experience requirement is achievable.** "Lives where the Volume / Ethernet icons live, dynamic, looks native" maps cleanly to `Shell_NotifyIcon` + a regenerated multi-size ICO per refresh + a Mica-styled WebView2 popup. Tauri’s tray + the `tiny-skia`/`resvg` renderer Win-CodexBar already uses gives you that out of the box.

## What you give up

- **You don’t get to keep the Swift codebase.** This fork’s `Sources/` directory gets wiped at Phase 0. The Swift code lives on at `steipete/CodexBar` and as a *spec* for behavior — that’s its enduring value.
- **You take on a downstream debt.** Some of the imported Rust is Win-CodexBar’s code, not ours. That’s normal for forks and the license allows it; we just need to actually read and own it (Phase 1).
- **Two-language project.** Rust for backend + tray, TypeScript for the popup. Tauri makes the seam minimal but it is a seam.
- **No Widgets parity.** WidgetKit is Apple-only and the Windows 11 widget surface is not worth shipping at v1.

## What you keep

- **Provider count**: 30+ at v1.
- **Dynamic tray icon**: yes — two-bar meter, brand mode, stale dim, incident overlay, theme-aware.
- **Same CLI**: one Rust workspace produces `codexbar-desktop.exe` and `codexbar.exe`.
- **OAuth / cookies / CLI auth paths**: all of them.
- **Cost-scan** for Claude + Codex.
- **Status polling** with incident badges.
- **Refresh cadence** presets.
- **Notifications** + reset celebration (as a tray-icon morph + toast — not full-screen confetti, which doesn’t fit Windows).
- **Merge Icons mode** + per-provider mode.
- **Localization** including Brazilian Portuguese.

## Alternative if the recommendation is unacceptable

If "no Rust" is a hard constraint, build on **.NET 8 + WinUI 3 + `H.NotifyIcon`** (option C in [03](03-tech-stack-options.md)). You will be writing every provider from scratch and shouldering more risk — the two C# attempts before us both stalled — but it’s the second-best stack and the second-best community precedent.

## What I’d ask you to decide before any code moves

The eight questions at the end of [07-risks-and-open-questions.md](07-risks-and-open-questions.md), specifically:

1. Project name / identity (affects everything downstream).
2. Code-signing org and budget (gates the install story, not the code).
3. Path 1 (rebase) vs Path 2 (clean rebuild).
4. v1 provider scope: all 30+, or a launch set with the rest in v1.1.

The rest can be resolved during Phase 0–1.

## What this looks like in time

- **Day 0–7**: repo reset, baseline import, CI green, first build.
- **Week 1–3**: own the imported code.
- **Week 3–5**: parity audit, gap issues.
- **Week 5–7**: Windows polish, theming, hotkeys, onboarding.
- **Week 7–8**: signed installer + auto-update.
- **Week 8–10**: beta.
- **Week 10–12**: GA + Winget.

Detailed plan in [06-roadmap.md](06-roadmap.md).

---

If you accept this recommendation, the next concrete step is Phase 0: confirm the four decisions above, then strip this fork’s Swift sources and import Win-CodexBar’s tree as the baseline. From there, every change is a normal PR against a Rust + TypeScript project.
