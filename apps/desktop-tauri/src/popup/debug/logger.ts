// Diagnostic logger — every meaningful event in the popup boot flow
// funnels through here while we hunt the blank-popup regression.
//
// Writes to three sinks so we can't miss anything:
//   1. `console.*` so DevTools (auto-opened by the Rust side) shows it
//      live with stack-traceable origins.
//   2. The Tauri `log_from_ui` command which appends to
//      `%APPDATA%\CodexBar4Windows\popup.log` — survives crashes and
//      can be tailed from PowerShell.
//   3. An in-memory ring buffer that an on-screen overlay reads, so
//      the user sees the last ~40 events even if devtools is closed.
//
// The Tauri invoke is fire-and-forget; if it throws we never want a
// log call to take down the UI.

import { invoke } from "@tauri-apps/api/core";

type Level = "info" | "warn" | "error";

interface LogEntry {
  ts: number;
  level: Level;
  scope: string;
  message: string;
}

const ring: LogEntry[] = [];
const listeners = new Set<() => void>();
const MAX_RING = 80;

function fanout(entry: LogEntry) {
  ring.push(entry);
  if (ring.length > MAX_RING) ring.shift();
  for (const fn of listeners) {
    try {
      fn();
    } catch {
      /* never throw out of a logger */
    }
  }
}

function consoleEmit(entry: LogEntry) {
  const tag = `[${entry.scope}]`;
  if (entry.level === "error") console.error(tag, entry.message);
  else if (entry.level === "warn") console.warn(tag, entry.message);
  else console.log(tag, entry.message);
}

function backendEmit(entry: LogEntry) {
  // Don't await — backend write is best-effort. Swallow errors so a
  // disconnected invoke channel (e.g. very early in boot) never
  // crashes the UI.
  try {
    void invoke("log_from_ui", {
      level: entry.level,
      scope: entry.scope,
      message: entry.message,
    }).catch(() => {
      /* swallow */
    });
  } catch {
    /* swallow */
  }
}

function emit(level: Level, scope: string, message: string) {
  const entry: LogEntry = { ts: Date.now(), level, scope, message };
  consoleEmit(entry);
  backendEmit(entry);
  fanout(entry);
}

export const debugLog = {
  info(scope: string, message: string) {
    emit("info", scope, message);
  },
  warn(scope: string, message: string) {
    emit("warn", scope, message);
  },
  error(scope: string, message: string) {
    emit("error", scope, message);
  },
  /** Snapshot of the in-memory ring; pass to React via `useSyncExternalStore`. */
  snapshot(): readonly LogEntry[] {
    return ring;
  },
  subscribe(fn: () => void): () => void {
    listeners.add(fn);
    return () => {
      listeners.delete(fn);
    };
  },
};

// Capture window-level errors so React crashes outside the boundary
// (top-level useEffect throws, async errors, etc.) still surface.
if (typeof window !== "undefined") {
  window.addEventListener("error", (ev) => {
    debugLog.error(
      "window.onerror",
      `${ev.message} @ ${ev.filename}:${ev.lineno}:${ev.colno}`,
    );
  });
  window.addEventListener("unhandledrejection", (ev) => {
    const reason =
      ev.reason instanceof Error
        ? `${ev.reason.message}\n${ev.reason.stack ?? ""}`
        : String(ev.reason);
    debugLog.error("window.unhandledrejection", reason);
  });
}
