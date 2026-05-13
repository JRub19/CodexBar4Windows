import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Update banner. Phase 9 §F.
//
// On mount, polls `check_for_update` once. When an update is
// available, renders a thin banner across the popup top with
// "Update now" / "Later". "Update now" calls `install_update`
// which downloads, verifies the minisign signature against the
// pubkey embedded in tauri.conf.json, and runs the installer
// silently. The installer kills + restarts the app.
//
// Dismissals are session-local: the banner returns on the next
// app launch if the update is still pending.

interface UpdateInfoDto {
  current_version: string;
  available_version: string | null;
  release_notes: string | null;
  release_date: string | null;
}

type Phase =
  | { kind: "idle" }
  | { kind: "available"; info: UpdateInfoDto }
  | { kind: "installing" }
  | { kind: "error"; message: string }
  | { kind: "dismissed" };

export function UpdateBanner() {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  useEffect(() => {
    let cancelled = false;
    void invoke<UpdateInfoDto>("check_for_update")
      .then((info) => {
        if (cancelled) return;
        if (info.available_version) {
          setPhase({ kind: "available", info });
        }
      })
      .catch(() => {
        // Update checks are best-effort. A network failure or a
        // missing manifest should not pollute the popup with an
        // error toast — the user can re-check from About.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const install = useCallback(async () => {
    setPhase({ kind: "installing" });
    try {
      await invoke("install_update");
      // The installer kills + relaunches the app, so reaching here
      // means the relaunch is imminent. Hold the banner state.
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  }, []);

  if (phase.kind === "idle" || phase.kind === "dismissed") {
    return null;
  }

  if (phase.kind === "installing") {
    return (
      <div className="update-banner update-banner--installing" role="status">
        <span className="update-banner__copy">
          Downloading update… The app will restart automatically.
        </span>
      </div>
    );
  }

  if (phase.kind === "error") {
    return (
      <div className="update-banner update-banner--error" role="alert">
        <span className="update-banner__copy">{phase.message}</span>
        <button
          type="button"
          className="update-banner__button"
          onClick={() => setPhase({ kind: "dismissed" })}
        >
          Dismiss
        </button>
      </div>
    );
  }

  return (
    <div className="update-banner update-banner--available" role="status">
      <span className="update-banner__copy">
        CodexBar4Windows <strong>{phase.info.available_version}</strong> is
        available. You are on {phase.info.current_version}.
      </span>
      <div className="update-banner__actions">
        <button
          type="button"
          className="update-banner__button update-banner__button--primary"
          onClick={() => void install()}
        >
          Update now
        </button>
        <button
          type="button"
          className="update-banner__button"
          onClick={() => setPhase({ kind: "dismissed" })}
        >
          Later
        </button>
      </div>
    </div>
  );
}
