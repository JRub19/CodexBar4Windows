import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// Generic provider-kv field. Loads its initial value from
// `get_provider_kv` (returns null when unset) and persists changes
// via `set_provider_kv`. Persists on blur to avoid one IPC per
// keystroke; an explicit save button is not needed because the
// refresh loop reads the latest snapshot on its next tick anyway.

export function PersistentField({
  storageKey,
  title,
  subtitle,
  placeholder,
  defaultValue,
  secret,
}: {
  storageKey: string;
  title: string;
  subtitle?: string | null;
  placeholder?: string | null;
  defaultValue?: string | null;
  secret?: boolean;
}) {
  const [value, setValue] = useState("");
  const [revealed, setRevealed] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const persisted = useRef<string>("");

  useEffect(() => {
    let cancelled = false;
    void invoke<string | null>("get_provider_kv", { key: storageKey })
      .then((stored) => {
        if (cancelled) return;
        const v = stored ?? defaultValue ?? "";
        setValue(v);
        persisted.current = v;
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [storageKey, defaultValue]);

  const save = useCallback(async () => {
    if (value === persisted.current) return;
    try {
      await invoke("set_provider_kv", { key: storageKey, value });
      persisted.current = value;
    } catch (e) {
      setError(String(e));
    }
  }, [storageKey, value]);

  return (
    <label className="settings-row settings-row--field">
      <span className="settings-row__title">{title}</span>
      {subtitle ? <span className="settings-row__subtitle">{subtitle}</span> : null}
      <div className="settings-row__value-wrapper">
        <input
          type={secret && !revealed ? "password" : "text"}
          placeholder={placeholder ?? ""}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onBlur={() => void save()}
        />
        {secret ? (
          <button
            type="button"
            className="settings-row__reveal"
            onClick={() => setRevealed((r) => !r)}
          >
            {revealed ? "Hide" : "Show"}
          </button>
        ) : null}
      </div>
      {error ? <p className="settings-row__error">{error}</p> : null}
    </label>
  );
}

export function PersistentPicker({
  storageKey,
  title,
  subtitle,
  defaultValue,
  options,
}: {
  storageKey: string;
  title: string;
  subtitle?: string | null;
  defaultValue: string;
  options: { value: string; label: string }[];
}) {
  const [value, setValue] = useState(defaultValue);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invoke<string | null>("get_provider_kv", { key: storageKey })
      .then((stored) => {
        if (!cancelled) setValue(stored ?? defaultValue);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [storageKey, defaultValue]);

  const onChange = useCallback(
    async (next: string) => {
      setValue(next);
      try {
        await invoke("set_provider_kv", { key: storageKey, value: next });
      } catch (e) {
        setError(String(e));
      }
    },
    [storageKey],
  );

  return (
    <label className="settings-row settings-row--picker">
      <span className="settings-row__title">{title}</span>
      {subtitle ? <span className="settings-row__subtitle">{subtitle}</span> : null}
      <select value={value} onChange={(e) => void onChange(e.target.value)}>
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      {error ? <p className="settings-row__error">{error}</p> : null}
    </label>
  );
}
