import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Calls the Rust `auto_import_cookies` command. The Tauri side walks
// Chrome → Edge → Brave → Firefox for the provider's cookie domains
// and persists the resulting Cookie header into the DPAPI-wrapped
// TokenAccountStore as a new active account.

interface OutcomeDto {
  provider_id: string;
  account_id: string;
  label: string;
  source: string;
}

type Phase =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "success"; outcome: OutcomeDto }
  | { kind: "error"; message: string };

export function AutoImportCookiesButton({
  providerId,
  onImport,
}: {
  providerId: string;
  onImport?: () => void;
}) {
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });

  const run = useCallback(async () => {
    setPhase({ kind: "running" });
    try {
      const outcome = await invoke<OutcomeDto>("auto_import_cookies", {
        providerId,
      });
      setPhase({ kind: "success", outcome });
      onImport?.();
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  }, [providerId, onImport]);

  if (phase.kind === "running") {
    return <p className="settings-row__loading">Importing cookies…</p>;
  }

  if (phase.kind === "success") {
    return (
      <div className="settings-row__autoimport settings-row__autoimport--ok">
        <p>
          Imported <strong>{phase.outcome.label}</strong> (
          {prettySource(phase.outcome.source)}).
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

  if (phase.kind === "error") {
    return (
      <div className="settings-row__autoimport settings-row__autoimport--err">
        <p className="settings-row__error">{phase.message}</p>
        <button
          type="button"
          className="settings-action"
          onClick={() => void run()}
        >
          Try again
        </button>
      </div>
    );
  }

  return (
    <button
      type="button"
      className="settings-action settings-action--primary"
      onClick={() => void run()}
    >
      Import cookies from browser
    </button>
  );
}

function prettySource(raw: string): string {
  if (raw === "cache") return "cached";
  if (raw === "manual") return "manual paste";
  if (raw.startsWith("browser:")) {
    const browser = raw.slice("browser:".length);
    return `from ${browser}`;
  }
  return raw;
}
