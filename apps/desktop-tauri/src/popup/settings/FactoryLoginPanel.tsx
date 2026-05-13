import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

// Two-step "paste the wos-session cookie" login.
//
// Factory does not expose a public OAuth device-code flow, so the user
// finishes WorkOS sign-in in their default browser, then copies the
// `wos-session` cookie value into the form here. The Rust side trades
// the cookie for an access_token + refresh_token via WorkOS and
// stores both in the DPAPI-wrapped TokenAccountStore.

interface LoginResultDto {
  bearer_account_id: string;
  refresh_account_id: string | null;
}

const SIGN_IN_URL = "https://app.factory.ai";
const DEVTOOLS_HINT =
  "DevTools → Application → Cookies → app.factory.ai → wos-session";

type Phase =
  | { kind: "idle" }
  | { kind: "submitting" }
  | { kind: "success"; result: LoginResultDto }
  | { kind: "error"; message: string };

export function FactoryLoginPanel({ onLogin }: { onLogin?: () => void }) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [cookie, setCookie] = useState("");
  const [revealed, setRevealed] = useState(false);

  const openSignIn = useCallback(() => {
    void openUrl(SIGN_IN_URL).catch(() => {});
  }, []);

  const submit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!cookie.trim()) return;
      setPhase({ kind: "submitting" });
      try {
        const result = await invoke<LoginResultDto>(
          "complete_factory_workos_login",
          { cookieValue: cookie.trim() },
        );
        setPhase({ kind: "success", result });
        setCookie("");
        onLogin?.();
      } catch (e) {
        setPhase({ kind: "error", message: String(e) });
      }
    },
    [cookie, onLogin],
  );

  if (phase.kind === "submitting") {
    return <p className="settings-row__loading">Exchanging cookie with WorkOS…</p>;
  }

  if (phase.kind === "success") {
    return (
      <div className="factory-login factory-login--ok">
        <p>
          Saved WorkOS bearer
          {phase.result.refresh_account_id ? " + refresh token" : ""}. The next
          refresh tick will use it.
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
    <div className="factory-login">
      <ol className="factory-login__steps">
        <li>
          <button
            type="button"
            className="settings-action settings-action--primary"
            onClick={openSignIn}
          >
            Open app.factory.ai
          </button>
        </li>
        <li>Sign in via WorkOS in the browser window that opened.</li>
        <li>
          Open <code>{DEVTOOLS_HINT}</code> and copy the Value.
        </li>
        <li>Paste it below and click Save.</li>
      </ol>
      <form className="factory-login__form" onSubmit={submit}>
        <label className="settings-row__field settings-row__field--value">
          <span>wos-session cookie value</span>
          <div className="settings-row__value-wrapper">
            <input
              type={revealed ? "text" : "password"}
              value={cookie}
              onChange={(e) => setCookie(e.target.value)}
              placeholder="paste cookie value"
            />
            <button
              type="button"
              className="settings-row__reveal"
              onClick={() => setRevealed((r) => !r)}
            >
              {revealed ? "Hide" : "Show"}
            </button>
          </div>
        </label>
        <div className="settings-row__add-actions">
          <button type="submit" className="settings-action">
            Save and verify
          </button>
        </div>
      </form>
      {phase.kind === "error" ? (
        <p className="settings-row__error">{phase.message}</p>
      ) : null}
    </div>
  );
}
