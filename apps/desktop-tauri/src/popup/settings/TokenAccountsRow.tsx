import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Rust-side Tauri commands surfaced here:
//   list_token_accounts(provider_id) -> ListedAccounts
//   add_token_account(provider_id, kind, label, value) -> TokenAccountDto
//   remove_token_account(provider_id, account_id) -> ()
//   set_active_token_account(provider_id, account_id) -> ()

type TokenKind = "cookie" | "oauth_token" | "api_key";

interface TokenAccountDto {
  id: string;
  kind: TokenKind | string;
  label: string;
  created_at_unix_secs: number;
}

interface ListedAccounts {
  accounts: TokenAccountDto[];
  active_id: string | null;
}

interface Props {
  providerId: string;
  title: string;
  subtitle?: string | null;
}

const KIND_LABEL: Record<string, string> = {
  cookie: "Cookie",
  oauth_token: "OAuth token",
  api_key: "API key",
};

// Per-provider default kind. Cookie providers default to "cookie";
// pure API-key providers default to "api_key". Everyone else defaults
// to "oauth_token" since that is the GitHub/Anthropic norm.
function defaultKindFor(providerId: string): TokenKind {
  switch (providerId) {
    case "cursor":
    case "factory":
      return "cookie";
    case "openrouter":
      return "api_key";
    default:
      return "oauth_token";
  }
}

export function TokenAccountsRow({ providerId, title, subtitle }: Props) {
  const [accounts, setAccounts] = useState<TokenAccountDto[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const [label, setLabel] = useState("");
  const [value, setValue] = useState("");
  const [kind, setKind] = useState<TokenKind>(defaultKindFor(providerId));
  const [revealed, setRevealed] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await invoke<ListedAccounts>("list_token_accounts", {
        providerId,
      });
      setAccounts(next.accounts);
      setActiveId(next.active_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [providerId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const submit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!value.trim()) return;
      try {
        await invoke("add_token_account", {
          providerId,
          kind,
          label: label.trim() || `Saved ${new Date().toLocaleString()}`,
          value: value.trim(),
        });
        setValue("");
        setLabel("");
        setAdding(false);
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [providerId, kind, label, value, refresh],
  );

  const onRemove = useCallback(
    async (id: string) => {
      try {
        await invoke("remove_token_account", {
          providerId,
          accountId: id,
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [providerId, refresh],
  );

  const onSetActive = useCallback(
    async (id: string) => {
      try {
        await invoke("set_active_token_account", {
          providerId,
          accountId: id,
        });
        await refresh();
      } catch (e) {
        setError(String(e));
      }
    },
    [providerId, refresh],
  );

  return (
    <div className="settings-row settings-row--accounts">
      <span className="settings-row__title">{title}</span>
      {subtitle ? (
        <span className="settings-row__subtitle">{subtitle}</span>
      ) : null}
      {error ? <p className="settings-row__error">{error}</p> : null}
      {loading ? (
        <p className="settings-row__loading">Loading…</p>
      ) : accounts.length === 0 ? (
        <p className="settings-row__empty">No accounts saved yet.</p>
      ) : (
        <ul className="settings-row__account-list">
          {accounts.map((a) => (
            <li key={a.id} className="settings-row__account">
              <label className="settings-row__account-label">
                <input
                  type="radio"
                  name={`active-${providerId}`}
                  checked={activeId === a.id}
                  onChange={() => void onSetActive(a.id)}
                />
                <span className="settings-row__account-name">{a.label}</span>
                <span className="settings-row__account-kind">
                  {KIND_LABEL[a.kind] ?? a.kind}
                </span>
              </label>
              <button
                type="button"
                className="settings-row__account-remove"
                onClick={() => void onRemove(a.id)}
                aria-label={`Remove ${a.label}`}
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}
      {adding ? (
        <form className="settings-row__add-form" onSubmit={submit}>
          <label className="settings-row__field">
            <span>Label</span>
            <input
              type="text"
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder="e.g. Personal account"
            />
          </label>
          <label className="settings-row__field">
            <span>Kind</span>
            <select
              value={kind}
              onChange={(e) => setKind(e.target.value as TokenKind)}
            >
              <option value="oauth_token">OAuth token</option>
              <option value="api_key">API key</option>
              <option value="cookie">Cookie</option>
            </select>
          </label>
          <label className="settings-row__field settings-row__field--value">
            <span>Value</span>
            <div className="settings-row__value-wrapper">
              <input
                type={revealed ? "text" : "password"}
                value={value}
                onChange={(e) => setValue(e.target.value)}
                placeholder="Paste token or cookie header"
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
              Save
            </button>
            <button
              type="button"
              className="settings-action"
              onClick={() => {
                setAdding(false);
                setValue("");
                setLabel("");
                setError(null);
              }}
            >
              Cancel
            </button>
          </div>
        </form>
      ) : (
        <button
          type="button"
          className="settings-action"
          onClick={() => setAdding(true)}
        >
          Add account
        </button>
      )}
    </div>
  );
}
