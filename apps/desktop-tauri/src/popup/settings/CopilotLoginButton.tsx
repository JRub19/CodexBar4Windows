import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { openUrl } from "@tauri-apps/plugin-opener";

// Two-step GitHub device-code flow:
//   1. start_copilot_device_login → { session_id, user_code, verification_uri }
//   2. poll_copilot_device_login(session_id)  — blocks until the user
//      finishes the GitHub flow, then stores the access_token in the
//      DPAPI-wrapped TokenAccountStore.

interface DeviceCodeDto {
  session_id: string;
  user_code: string;
  verification_uri: string;
  verification_uri_complete: string | null;
  expires_in_secs: number;
  interval_secs: number;
}

interface LoginResultDto {
  account_id: string;
  label: string;
}

type Phase =
  | { kind: "idle" }
  | { kind: "starting" }
  | { kind: "awaiting"; code: DeviceCodeDto }
  | { kind: "success"; result: LoginResultDto }
  | { kind: "error"; message: string };

export function CopilotLoginButton({
  enterpriseHost,
  onLogin,
}: {
  enterpriseHost?: string;
  onLogin?: () => void;
}) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  const start = useCallback(async () => {
    setPhase({ kind: "starting" });
    try {
      const code = await invoke<DeviceCodeDto>("start_copilot_device_login", {
        enterpriseHost: enterpriseHost?.trim() || null,
      });
      setPhase({ kind: "awaiting", code });
      const urlToOpen = code.verification_uri_complete ?? code.verification_uri;
      try {
        await openUrl(urlToOpen);
      } catch {
        // Opening fails on systems without a default browser — the
        // user can still click the link rendered below.
      }
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  }, [enterpriseHost]);

  useEffect(() => {
    if (phase.kind !== "awaiting") return;
    let cancelled = false;
    const sessionId = phase.code.session_id;
    (async () => {
      try {
        const result = await invoke<LoginResultDto>(
          "poll_copilot_device_login",
          { sessionId },
        );
        if (!cancelled) {
          setPhase({ kind: "success", result });
          onLogin?.();
        }
      } catch (e) {
        if (!cancelled) {
          setPhase({ kind: "error", message: String(e) });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [phase, onLogin]);

  const copyCode = useCallback((code: string) => {
    void writeText(code).catch(() => {});
  }, []);

  if (phase.kind === "idle") {
    return (
      <button
        type="button"
        className="settings-action settings-action--primary"
        onClick={() => void start()}
      >
        Sign in with GitHub
      </button>
    );
  }

  if (phase.kind === "starting") {
    return <p className="settings-row__loading">Requesting device code…</p>;
  }

  if (phase.kind === "awaiting") {
    const { verification_uri, verification_uri_complete } = phase.code;
    const link = verification_uri_complete ?? verification_uri;
    return (
      <div className="copilot-login">
        <p className="copilot-login__copy">
          A browser window opened at{" "}
          <a href={link} target="_blank" rel="noopener noreferrer">
            {verification_uri}
          </a>
          . Enter the code shown below.
        </p>
        <div className="copilot-login__code-row">
          <code className="copilot-login__code">{phase.code.user_code}</code>
          <button
            type="button"
            className="settings-action"
            onClick={() => copyCode(phase.code.user_code)}
          >
            Copy code
          </button>
        </div>
        <p className="copilot-login__status">
          Waiting for confirmation on GitHub…
        </p>
        <button
          type="button"
          className="settings-action"
          onClick={() => setPhase({ kind: "idle" })}
        >
          Cancel
        </button>
      </div>
    );
  }

  if (phase.kind === "success") {
    return (
      <div className="copilot-login copilot-login--ok">
        <p>
          Saved <strong>{phase.result.label}</strong>. The next refresh tick
          will use it.
        </p>
        <button
          type="button"
          className="settings-action"
          onClick={() => setPhase({ kind: "idle" })}
        >
          Done
        </button>
      </div>
    );
  }

  return (
    <div className="copilot-login copilot-login--err">
      <p className="settings-row__error">{phase.message}</p>
      <button
        type="button"
        className="settings-action"
        onClick={() => setPhase({ kind: "idle" })}
      >
        Try again
      </button>
    </div>
  );
}
