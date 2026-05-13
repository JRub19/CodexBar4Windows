import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { KeyShortcutRecorder } from "../components/KeyShortcutRecorder";

import "../styles/popup.css";
import "../styles/settings.css";
import "../styles/focus.css";

import type {
  ProviderSettingsSnapshot,
  Settings,
  SettingsDescriptor,
} from "../bindings";
import { TokenAccountsRow } from "../popup/settings/TokenAccountsRow";
import { CopilotLoginButton } from "../popup/settings/CopilotLoginButton";
import { AutoImportCookiesButton } from "../popup/settings/AutoImportCookiesButton";
import { FactoryLoginPanel } from "../popup/settings/FactoryLoginPanel";
import { CursorLoginButton } from "../popup/settings/CursorLoginButton";
import {
  PersistentField,
  PersistentPicker,
} from "../popup/settings/PersistentField";

// Phase 8 standalone Settings window. The seven-pane outline from
// docs/windows/spec/20-preferences-ui.md is collapsed here into five
// shipped panes plus an About surface. Onboarding and Cost panes can
// land in follow-ups; the layout reserves their slot in the sidebar.

type PaneId =
  | "general"
  | "providers"
  | "notifications"
  | "shortcuts"
  | "advanced"
  | "about";

const PANES: ReadonlyArray<{ id: PaneId; label: string }> = [
  { id: "general", label: "General" },
  { id: "providers", label: "Providers" },
  { id: "notifications", label: "Notifications" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "advanced", label: "Advanced" },
  { id: "about", label: "About" },
];

const PROVIDER_KV_PREFIXES = [
  "moonshot.",
  "zai.",
  "copilot.",
  "cursor.",
  "deepseek.",
  "gemini.",
  "factory.",
  "openrouter.",
  "venice.",
];

function isProviderKvKey(key: string): boolean {
  return PROVIDER_KV_PREFIXES.some((p) => key.startsWith(p));
}

interface FirstRunStateDto {
  last_settings_pane: string | null;
}

export function SettingsApp() {
  const [pane, setPane] = useState<PaneId>("general");

  // Restore the last-active pane and current window geometry on
  // mount; persist subsequent changes back to state.json so the
  // user lands on the same pane / same window position next time.
  useEffect(() => {
    void invoke<FirstRunStateDto>("first_run_state").then((s) => {
      const restored = s.last_settings_pane as PaneId | null;
      if (restored && PANES.some((p) => p.id === restored)) {
        setPane(restored);
      }
    });
  }, []);

  useEffect(() => {
    void invoke("save_last_settings_pane", { pane }).catch(() => {});
  }, [pane]);

  useEffect(() => {
    const win = getCurrentWindow();
    let timer: ReturnType<typeof setTimeout> | null = null;
    const persist = async () => {
      try {
        const pos = await win.outerPosition();
        const size = await win.outerSize();
        await invoke("save_settings_window_geometry", {
          x: pos.x,
          y: pos.y,
          width: size.width,
          height: size.height,
        });
      } catch {
        // best-effort; OS errors are non-fatal here.
      }
    };
    const debounced = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => void persist(), 500);
    };
    const unlistenResize = win.onResized(debounced);
    const unlistenMove = win.onMoved(debounced);
    const onUnload = () => void persist();
    window.addEventListener("beforeunload", onUnload);
    return () => {
      window.removeEventListener("beforeunload", onUnload);
      void unlistenResize.then((f) => f());
      void unlistenMove.then((f) => f());
      if (timer) clearTimeout(timer);
    };
  }, []);

  return (
    <div className="settings-app">
      <aside className="settings-app__sidebar" aria-label="Preferences sections">
        <h1 className="settings-app__title">Preferences</h1>
        <nav>
          {PANES.map((entry) => (
            <button
              key={entry.id}
              type="button"
              className={
                pane === entry.id
                  ? "settings-app__nav-item settings-app__nav-item--active"
                  : "settings-app__nav-item"
              }
              onClick={() => setPane(entry.id)}
              aria-current={pane === entry.id ? "page" : undefined}
            >
              {entry.label}
            </button>
          ))}
        </nav>
      </aside>
      <main className="settings-app__pane" role="region" aria-label={paneLabel(pane)}>
        <header className="settings-app__pane-header">
          <h2>{paneLabel(pane)}</h2>
        </header>
        <div className="settings-app__pane-body">
          {pane === "general" ? <GeneralPane /> : null}
          {pane === "providers" ? <ProvidersPane /> : null}
          {pane === "notifications" ? <NotificationsPane /> : null}
          {pane === "shortcuts" ? <ShortcutsPane /> : null}
          {pane === "advanced" ? <AdvancedPane /> : null}
          {pane === "about" ? <AboutPane /> : null}
        </div>
      </main>
    </div>
  );
}

function paneLabel(id: PaneId): string {
  return PANES.find((p) => p.id === id)?.label ?? id;
}

function GeneralPane() {
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    void invoke<Settings>("get_settings").then(setSettings);
  }, []);
  if (!settings) return <p className="settings-app__loading">Loading…</p>;

  const setRefreshFreq = async (value: string) => {
    await invoke("update_settings", {
      patch: { refresh_frequency: value },
    });
    const next = await invoke<Settings>("get_settings");
    setSettings(next);
  };

  const togglePause = async () => {
    await invoke("toggle_pause", { paused: !settings.pause_refresh });
    const next = await invoke<Settings>("get_settings");
    setSettings(next);
  };

  return (
    <>
      <p className="settings-app__pane-intro">
        How often the refresh loop polls each provider.
      </p>
      <label className="settings-row settings-row--picker">
        <span className="settings-row__title">Refresh frequency</span>
        <select
          value={settings.refresh_frequency}
          onChange={(e) => void setRefreshFreq(e.target.value)}
        >
          <option value="manual">Manual only</option>
          <option value="one_minute">Every 1 minute</option>
          <option value="two_minutes">Every 2 minutes</option>
          <option value="five_minutes">Every 5 minutes (default)</option>
          <option value="fifteen_minutes">Every 15 minutes</option>
          <option value="thirty_minutes">Every 30 minutes</option>
        </select>
      </label>
      <label className="settings-row settings-row--toggle">
        <span className="settings-row__title">Pause refresh</span>
        <span className="settings-row__subtitle">
          Stop all polling. Useful while debugging or when offline.
        </span>
        <input
          type="checkbox"
          checked={settings.pause_refresh}
          onChange={() => void togglePause()}
        />
      </label>
    </>
  );
}

function ProvidersPane() {
  const [snapshot, setSnapshot] = useState<ProviderSettingsSnapshot | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderSettingsSnapshot>("provider_settings_descriptors")
      .then((next) => {
        if (cancelled) return;
        setSnapshot(next);
        if (next.sections.length > 0) {
          setSelected(next.sections[0].provider_id);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);
  if (error) return <p className="settings-row__error">{error}</p>;
  if (!snapshot) return <p className="settings-app__loading">Loading…</p>;
  if (snapshot.sections.length === 0)
    return <p className="settings-app__empty">No providers configured.</p>;

  const section =
    snapshot.sections.find((s) => s.provider_id === selected) ??
    snapshot.sections[0];

  return (
    <div className="settings-providers">
      <ul className="settings-providers__list">
        {snapshot.sections.map((s) => (
          <li key={s.provider_id}>
            <button
              type="button"
              className={
                s.provider_id === section.provider_id
                  ? "settings-providers__entry settings-providers__entry--active"
                  : "settings-providers__entry"
              }
              onClick={() => setSelected(s.provider_id)}
            >
              {s.section_title}
            </button>
          </li>
        ))}
      </ul>
      <section className="settings-providers__detail">
        <h3>{section.section_title}</h3>
        {section.rows.map((row, idx) => (
          <DescriptorRow key={idx} descriptor={row} />
        ))}
        {section.provider_id === "copilot" ? (
          <div className="settings-row settings-row--login">
            <CopilotLoginButton />
          </div>
        ) : null}
        {section.provider_id === "cursor" ? (
          <div className="settings-row settings-row--login">
            <CursorLoginButton />
          </div>
        ) : null}
        {section.provider_id === "cursor" ||
        section.provider_id === "factory" ? (
          <div className="settings-row settings-row--autoimport">
            <AutoImportCookiesButton providerId={section.provider_id} />
          </div>
        ) : null}
        {section.provider_id === "factory" ? (
          <div className="settings-row settings-row--login">
            <FactoryLoginPanel />
          </div>
        ) : null}
      </section>
    </div>
  );
}

function DescriptorRow({ descriptor }: { descriptor: SettingsDescriptor }) {
  switch (descriptor.kind) {
    case "toggle":
      return (
        <label className="settings-row settings-row--toggle">
          <span className="settings-row__title">{descriptor.title}</span>
          {descriptor.subtitle ? (
            <span className="settings-row__subtitle">{descriptor.subtitle}</span>
          ) : null}
          <input type="checkbox" defaultChecked={descriptor.default} />
        </label>
      );
    case "field":
      if (isProviderKvKey(descriptor.key)) {
        return (
          <PersistentField
            storageKey={descriptor.key}
            title={descriptor.title}
            subtitle={descriptor.subtitle}
            placeholder={descriptor.placeholder}
            secret={descriptor.secret}
          />
        );
      }
      return (
        <label className="settings-row settings-row--field">
          <span className="settings-row__title">{descriptor.title}</span>
          {descriptor.subtitle ? (
            <span className="settings-row__subtitle">{descriptor.subtitle}</span>
          ) : null}
          <input
            type={descriptor.secret ? "password" : "text"}
            placeholder={descriptor.placeholder ?? ""}
          />
        </label>
      );
    case "picker":
      if (isProviderKvKey(descriptor.key)) {
        return (
          <PersistentPicker
            storageKey={descriptor.key}
            title={descriptor.title}
            subtitle={descriptor.subtitle}
            defaultValue={descriptor.default}
            options={descriptor.options}
          />
        );
      }
      return (
        <label className="settings-row settings-row--picker">
          <span className="settings-row__title">{descriptor.title}</span>
          {descriptor.subtitle ? (
            <span className="settings-row__subtitle">{descriptor.subtitle}</span>
          ) : null}
          <select defaultValue={descriptor.default}>
            {descriptor.options.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </label>
      );
    case "actions_row":
      return (
        <div className="settings-row settings-row--actions">
          <span className="settings-row__title">{descriptor.title}</span>
          <div className="settings-row__actions">
            {descriptor.actions.map((action) => (
              <button
                key={action.id}
                type="button"
                className={
                  action.destructive
                    ? "settings-action settings-action--destructive"
                    : "settings-action"
                }
              >
                {action.label}
              </button>
            ))}
          </div>
        </div>
      );
    case "token_accounts":
      return (
        <TokenAccountsRow
          providerId={descriptor.provider_id}
          title={descriptor.title}
          subtitle={descriptor.subtitle}
        />
      );
  }
}

function NotificationsPane() {
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    void invoke<Settings>("get_settings").then((s) => setEnabled(s.notifications_enabled));
  }, []);
  const toggle = async () => {
    try {
      const next = !enabled;
      await invoke("update_settings", {
        patch: { notifications_enabled: next },
      });
      setEnabled(next);
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <>
      <p className="settings-app__pane-intro">
        Desktop toasts for session-quota depletion + threshold crossings.
        Defaults: 50% / 25% / 10% remaining.
      </p>
      <label className="settings-row settings-row--toggle">
        <span className="settings-row__title">Enable notifications</span>
        <input
          type="checkbox"
          checked={enabled ?? false}
          onChange={() => void toggle()}
        />
      </label>
      {error ? <p className="settings-row__error">{error}</p> : null}
    </>
  );
}

function ShortcutsPane() {
  const [registered, setRegistered] = useState<boolean | null>(null);
  const [chord, setChord] = useState<string>("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void invoke<boolean>("hotkey_is_registered")
      .then(setRegistered)
      .catch((e) => setError(String(e)));
    void invoke<Settings>("get_settings").then((s) => {
      setChord(s.popup_toggle_hotkey ?? "");
    });
  }, []);

  const toggle = async () => {
    try {
      const next = !registered;
      if (next) {
        await invoke("hotkey_register");
      } else {
        await invoke("hotkey_unregister");
      }
      setRegistered(next);
    } catch (e) {
      setError(String(e));
    }
  };

  const onChordChange = async (next: string) => {
    try {
      await invoke("hotkey_set_chord", { chord: next });
      await invoke("update_settings", {
        patch: { popup_toggle_hotkey: next },
      });
      setChord(next);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const onChordClear = async () => {
    try {
      // Restore the platform default: re-register the built-in
      // shortcut and clear the persisted override.
      await invoke("hotkey_unregister");
      await invoke("hotkey_register");
      await invoke("update_settings", {
        patch: { popup_toggle_hotkey: null },
      });
      setChord("");
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <>
      <p className="settings-app__pane-intro">
        Global hotkey toggles the tray popup from anywhere on the desktop.
      </p>
      <label className="settings-row settings-row--toggle">
        <span className="settings-row__title">Enable global hotkey</span>
        <span className="settings-row__subtitle">
          Default: <kbd>Win+Shift+U</kbd>. Disable on conflict with another
          app.
        </span>
        <input
          type="checkbox"
          checked={registered ?? false}
          onChange={() => void toggle()}
        />
      </label>
      <KeyShortcutRecorder
        label="Toggle popup"
        value={chord}
        onChange={(c) => void onChordChange(c)}
        onClear={() => void onChordClear()}
        disabled={registered !== true}
      />
      {error ? <p className="settings-row__error">{error}</p> : null}
    </>
  );
}

function AdvancedPane() {
  const [launchAtSignIn, setLaunchAtSignIn] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    void invoke<boolean>("launch_at_signin_is_enabled")
      .then(setLaunchAtSignIn)
      .catch((e) => setError(String(e)));
  }, []);
  const toggleLaunch = async () => {
    try {
      const next = !launchAtSignIn;
      await invoke(next ? "launch_at_signin_enable" : "launch_at_signin_disable");
      setLaunchAtSignIn(next);
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <>
      <p className="settings-app__pane-intro">
        Power-user toggles. Restart the app after changes here.
      </p>
      <label className="settings-row settings-row--toggle">
        <span className="settings-row__title">Launch at sign-in</span>
        <span className="settings-row__subtitle">
          Adds a Run-key registry entry under HKCU. Easy to remove later.
        </span>
        <input
          type="checkbox"
          checked={launchAtSignIn ?? false}
          onChange={() => void toggleLaunch()}
        />
      </label>
      {error ? <p className="settings-row__error">{error}</p> : null}
    </>
  );
}

function AboutPane() {
  return (
    <>
      <p className="settings-app__pane-intro">
        CodexBar4Windows is a Tauri-based Windows port of the macOS
        CodexBar tray app. Open source under the MIT licence.
      </p>
      <ul className="settings-app__about-links">
        <li>
          <button
            type="button"
            className="settings-action"
            onClick={() =>
              void openUrl("https://github.com/").catch(() => {})
            }
          >
            Source on GitHub
          </button>
        </li>
      </ul>
    </>
  );
}
