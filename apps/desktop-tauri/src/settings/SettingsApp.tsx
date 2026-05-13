import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { KeyShortcutRecorder } from "../components/KeyShortcutRecorder";
import { Icon, type IconName } from "../components/Icon";
import { SUPPORTED_LOCALES, useT, type LocaleCode } from "../i18n";

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
  | "appearance"
  | "providers"
  | "notifications"
  | "shortcuts"
  | "cost"
  | "advanced"
  | "about";

const PANES: ReadonlyArray<{ id: PaneId; label: string; icon: IconName }> = [
  { id: "general", label: "General", icon: "cog" },
  { id: "providers", label: "Providers", icon: "layers" },
  { id: "notifications", label: "Notifications", icon: "bell" },
  { id: "appearance", label: "Appearance", icon: "palette" },
  { id: "shortcuts", label: "Shortcuts", icon: "keyboard" },
  { id: "cost", label: "Cost & Storage", icon: "chart" },
  { id: "advanced", label: "Advanced", icon: "sliders" },
  { id: "about", label: "About", icon: "info" },
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
  const [focusedProviderId, setFocusedProviderId] = useState<string | null>(
    null,
  );

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

  // Listen for `preferences:focus_provider` so the popup-side
  // onboarding flow can hand-off to a specific provider row. The
  // Tauri command emits this event when `open_preferences` is
  // called with a `providerId` argument.
  useEffect(() => {
    const unlisten = listen<{ provider_id: string }>(
      "preferences:focus_provider",
      (event) => {
        const id = event.payload?.provider_id;
        if (!id) return;
        setPane("providers");
        setFocusedProviderId(id);
      },
    );
    return () => {
      void unlisten.then((f) => f());
    };
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
        <nav className="settings-app__nav">
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
              <span className="settings-app__nav-icon">
                <Icon name={entry.icon} size={16} />
              </span>
              {entry.label}
            </button>
          ))}
        </nav>
        <div className="settings-app__sidebar-bottom">
          <button
            type="button"
            className="settings-app__nav-item"
            onClick={() => void invoke("quit_app")}
            style={{ color: "var(--text-error)" }}
          >
            <span className="settings-app__nav-icon">
              <Icon name="logout" size={16} />
            </span>
            Quit CodexBar
          </button>
        </div>
      </aside>
      <main className="settings-app__pane" role="region" aria-label={paneLabel(pane)}>
        <header className="settings-app__pane-header">
          <h2>{paneLabel(pane)}</h2>
        </header>
        <div className="settings-app__pane-body">
          {pane === "general" ? <GeneralPane /> : null}
          {pane === "appearance" ? <AppearancePane /> : null}
          {pane === "providers" ? (
            <ProvidersPane focusedProviderId={focusedProviderId} />
          ) : null}
          {pane === "notifications" ? <NotificationsPane /> : null}
          {pane === "shortcuts" ? <ShortcutsPane /> : null}
          {pane === "cost" ? <CostPane /> : null}
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

function AppearancePane() {
  const t = useT();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void invoke<Settings>("get_settings").then(setSettings).catch((e) => {
      setError(String(e));
    });
  }, []);

  if (error) return <p className="settings-row__error">{error}</p>;
  if (!settings) return <p className="settings-app__loading">Loading…</p>;

  /** "system" represents an empty `app_language` (auto-detect). */
  const currentValue: LocaleCode | "system" =
    settings.app_language && SUPPORTED_LOCALES.includes(settings.app_language as LocaleCode)
      ? (settings.app_language as LocaleCode)
      : "system";

  const onLanguageChange = async (value: LocaleCode | "system") => {
    try {
      const patch =
        value === "system"
          ? { app_language: null }
          : { app_language: value };
      const next = await invoke<Settings>("update_settings", { patch });
      setSettings(next);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <>
      <p className="settings-app__pane-intro">
        {t("settings.appearance.intro")}
      </p>
      <label className="settings-row settings-row--picker">
        <span className="settings-row__title">
          {t("settings.appearance.language")}
        </span>
        <select
          value={currentValue}
          onChange={(e) =>
            void onLanguageChange(
              e.target.value as LocaleCode | "system",
            )
          }
        >
          <option value="system">
            {t("settings.appearance.language.system")}
          </option>
          <option value="en">
            {t("settings.appearance.language.en")}
          </option>
          <option value="zh-Hans">
            {t("settings.appearance.language.zh_hans")}
          </option>
          <option value="pt-BR">
            {t("settings.appearance.language.pt_br")}
          </option>
        </select>
      </label>
    </>
  );
}

interface ProvidersPaneProps {
  /** When set (e.g. from `preferences:focus_provider`), pre-selects
   * this provider on mount and overrides the first-item default. */
  focusedProviderId: string | null;
}

interface ProviderDescriptorMini {
  id: string;
  display_name: string;
  accent_hex: string;
}

interface ProviderToggle {
  id: string;
  enabled: boolean;
  order?: number;
}

function ProvidersPane({ focusedProviderId }: ProvidersPaneProps) {
  const [snapshot, setSnapshot] = useState<ProviderSettingsSnapshot | null>(
    null,
  );
  const [descriptors, setDescriptors] = useState<ProviderDescriptorMini[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderSettingsSnapshot>("provider_settings_descriptors")
      .then((next) => {
        if (cancelled) return;
        setSnapshot(next);
        const preferred =
          focusedProviderId &&
          next.sections.some((s) => s.provider_id === focusedProviderId)
            ? focusedProviderId
            : next.sections[0]?.provider_id ?? null;
        setSelected(preferred);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });

    // Provider descriptors give us name + accent for the sidebar
    // swatches that the settings snapshot doesn't expose.
    void invoke<
      Array<{
        id: string;
        metadata: { display_name: string };
        branding: { accent_hex: string };
      }>
    >("provider_descriptors")
      .then((arr) => {
        if (cancelled) return;
        setDescriptors(
          arr.map((d) => ({
            id: d.id,
            display_name: d.metadata.display_name,
            accent_hex: d.branding.accent_hex,
          })),
        );
      })
      .catch(() => {});

    void invoke<Settings>("get_settings")
      .then((s) => {
        if (!cancelled) setSettings(s);
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [focusedProviderId]);

  useEffect(() => {
    if (!snapshot || !focusedProviderId) return;
    const found = snapshot.sections.find(
      (s) => s.provider_id === focusedProviderId,
    );
    if (found) setSelected(found.provider_id);
  }, [focusedProviderId, snapshot]);

  if (error) return <p className="settings-row__error">{error}</p>;
  if (!snapshot || !settings)
    return <p className="settings-app__loading">Loading…</p>;
  if (snapshot.sections.length === 0)
    return <p className="settings-app__empty">No providers configured.</p>;

  const section =
    snapshot.sections.find((s) => s.provider_id === selected) ??
    snapshot.sections[0];

  // Treat empty `settings.providers[]` as "all providers enabled" —
  // mirrors the refresh-loop semantics in rust/src/core/refresh.rs.
  const isEnabled = (providerId: string): boolean => {
    if (settings.providers.length === 0) return true;
    const toggle = settings.providers.find((p) => p.id === providerId);
    return toggle ? toggle.enabled : true;
  };

  // Persist a single provider's enabled state. If the settings list
  // is empty (fresh install), seed it with every known provider all
  // enabled, then flip the target — this lets the user toggle off a
  // single provider without accidentally disabling the others.
  const setEnabled = async (providerId: string, next: boolean) => {
    const allIds = snapshot.sections.map((s) => s.provider_id);
    const seed: ProviderToggle[] =
      settings.providers.length === 0
        ? allIds.map((id, idx) => ({ id, enabled: true, order: idx }))
        : settings.providers.slice();
    // Ensure every known provider has a slot before flipping ours.
    for (const id of allIds) {
      if (!seed.some((t) => t.id === id)) {
        seed.push({ id, enabled: true, order: seed.length });
      }
    }
    const updated = seed.map((t) =>
      t.id === providerId ? { ...t, enabled: next } : t,
    );
    try {
      const result = await invoke<Settings>("update_settings", {
        patch: { providers: updated },
      });
      setSettings(result);
    } catch (e) {
      setError(String(e));
    }
  };

  // Bulk action: set every provider to the same enabled state.
  const setAllEnabled = async (next: boolean) => {
    const allIds = snapshot.sections.map((s) => s.provider_id);
    const updated: ProviderToggle[] = allIds.map((id, idx) => ({
      id,
      enabled: next,
      order: idx,
    }));
    try {
      const result = await invoke<Settings>("update_settings", {
        patch: { providers: updated },
      });
      setSettings(result);
    } catch (e) {
      setError(String(e));
    }
  };

  const accentFor = (id: string): string =>
    descriptors.find((d) => d.id === id)?.accent_hex ?? "#888";

  const enabledCount = snapshot.sections.filter((s) =>
    isEnabled(s.provider_id),
  ).length;

  const sectionEnabled = isEnabled(section.provider_id);

  return (
    <>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          marginBottom: "var(--space-3)",
          padding: "0 var(--space-1)",
        }}
      >
        <span className="settings-row__subtitle">
          {enabledCount} of {snapshot.sections.length} providers enabled
        </span>
        <div style={{ display: "flex", gap: "var(--space-2)" }}>
          <button
            type="button"
            className="settings-action"
            onClick={() => void setAllEnabled(true)}
            disabled={enabledCount === snapshot.sections.length}
          >
            Enable all
          </button>
          <button
            type="button"
            className="settings-action"
            onClick={() => void setAllEnabled(false)}
            disabled={enabledCount === 0}
          >
            Disable all
          </button>
        </div>
      </div>
      <div className="settings-providers">
        <ul className="settings-providers__list">
          {snapshot.sections.map((s) => {
            const enabled = isEnabled(s.provider_id);
            const accent = accentFor(s.provider_id);
            return (
              <li key={s.provider_id}>
                <button
                  type="button"
                  className={
                    s.provider_id === section.provider_id
                      ? "settings-providers__entry settings-providers__entry--active"
                      : "settings-providers__entry"
                  }
                  ref={(el) => {
                    if (
                      el &&
                      focusedProviderId &&
                      s.provider_id === focusedProviderId
                    ) {
                      el.scrollIntoView({
                        block: "nearest",
                        behavior: "smooth",
                      });
                    }
                  }}
                  onClick={() => setSelected(s.provider_id)}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "var(--space-2)",
                    opacity: enabled ? 1 : 0.5,
                  }}
                >
                  <span
                    aria-hidden="true"
                    style={{
                      display: "inline-block",
                      width: 8,
                      height: 8,
                      borderRadius: 999,
                      background: accent,
                      flexShrink: 0,
                    }}
                  />
                  <span style={{ flex: 1, textAlign: "left" }}>
                    {s.section_title}
                  </span>
                  {!enabled ? (
                    <Icon
                      name="close"
                      size={12}
                      style={{ color: "var(--text-tertiary)" }}
                    />
                  ) : null}
                </button>
              </li>
            );
          })}
        </ul>
        <section className="settings-providers__detail">
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "var(--space-3)",
            }}
          >
            <span
              aria-hidden="true"
              style={{
                display: "inline-block",
                width: 12,
                height: 12,
                borderRadius: 999,
                background: accentFor(section.provider_id),
                flexShrink: 0,
              }}
            />
            <h3 style={{ flex: 1, margin: 0 }}>{section.section_title}</h3>
          </div>
          <div className="settings-section__card">
            <label className="settings-row settings-row--toggle">
              <div className="settings-row__text">
                <span className="settings-row__title">Enable this provider</span>
                <span className="settings-row__subtitle">
                  {sectionEnabled
                    ? "Showing in the popup. Toggle off to hide it from the popup and stop polling."
                    : "Hidden from the popup. Toggle on to start polling and show its card."}
                </span>
              </div>
              <input
                type="checkbox"
                checked={sectionEnabled}
                onChange={(e) =>
                  void setEnabled(section.provider_id, e.target.checked)
                }
              />
            </label>
          </div>
          {sectionEnabled ? (
            <>
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
            </>
          ) : (
            <p className="settings-app__empty" style={{ marginTop: "var(--space-3)" }}>
              Enable this provider to configure sign-in and per-provider
              settings.
            </p>
          )}
        </section>
      </div>
    </>
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

interface StorageComponent {
  id: string;
  path: string;
  total_bytes: number;
}

interface ProviderStorageFootprint {
  provider: string;
  total_bytes: number;
  paths: string[];
  missing_paths: string[];
  unreadable_paths: string[];
  components: StorageComponent[];
  updated_at: string;
}

export function formatBytes(n: number): string {
  if (n === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const k = 1024;
  const i = Math.min(units.length - 1, Math.floor(Math.log(n) / Math.log(k)));
  const value = n / Math.pow(k, i);
  const rounded = i === 0 ? value.toFixed(0) : value.toFixed(value >= 10 ? 1 : 2);
  return `${rounded} ${units[i]}`;
}

function CostPane() {
  const [reports, setReports] = useState<ProviderStorageFootprint[] | null>(
    null,
  );
  const [scanning, setScanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runScan = async () => {
    setScanning(true);
    setError(null);
    try {
      const next = await invoke<ProviderStorageFootprint[]>(
        "storage_footprint_scan",
      );
      setReports(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };

  // Trigger an initial scan on mount; the spec §11.5 throttle is
  // pinned at 5 minutes upstream but the React side just kicks it
  // off optimistically.
  useEffect(() => {
    void runScan();
  }, []);

  return (
    <>
      <p className="settings-app__pane-intro">
        Provider-owned disk usage. Surfaces only — nothing here ever
        deletes anything. Click <kbd>Open folder</kbd> on any row to
        explore it in Windows Explorer.
      </p>
      <div className="settings-row settings-row--actions">
        <span className="settings-row__title">Scan</span>
        <div className="settings-row__actions">
          <button
            type="button"
            className="settings-action"
            onClick={() => void runScan()}
            disabled={scanning}
          >
            {scanning ? "Scanning…" : "Re-scan"}
          </button>
        </div>
      </div>
      {error ? <p className="settings-row__error">{error}</p> : null}
      {reports?.map((r) => (
        <section key={r.provider} className="storage-footprint">
          <header className="storage-footprint__header">
            <h3 className="storage-footprint__provider">{r.provider}</h3>
            <span className="storage-footprint__total">
              {formatBytes(r.total_bytes)}
            </span>
          </header>
          {r.paths.length === 0 && r.components.length === 0 ? (
            <p className="storage-footprint__empty">
              No on-disk data found for this provider.
            </p>
          ) : null}
          {r.components.length > 0 ? (
            <ul className="storage-footprint__components">
              {r.components.map((c) => (
                <li key={c.path} className="storage-footprint__component">
                  <span className="storage-footprint__component-name">
                    {c.id}
                  </span>
                  <span className="storage-footprint__component-size">
                    {formatBytes(c.total_bytes)}
                  </span>
                  <button
                    type="button"
                    className="storage-footprint__component-open"
                    onClick={() =>
                      void invoke("open_in_explorer", {
                        path: c.path,
                      }).catch(() => {})
                    }
                  >
                    Open folder
                  </button>
                </li>
              ))}
            </ul>
          ) : null}
          {r.unreadable_paths.length > 0 ? (
            <details className="storage-footprint__unreadable">
              <summary>
                {r.unreadable_paths.length} unreadable
                {r.unreadable_paths.length === 1 ? " path" : " paths"}
              </summary>
              <ul>
                {r.unreadable_paths.slice(0, 10).map((p) => (
                  <li key={p}>{p}</li>
                ))}
              </ul>
            </details>
          ) : null}
        </section>
      ))}
    </>
  );
}

function AdvancedPane() {
  const [launchAtSignIn, setLaunchAtSignIn] = useState<boolean | null>(null);
  const [telemetry, setTelemetry] = useState<boolean | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    void invoke<boolean>("launch_at_signin_is_enabled")
      .then(setLaunchAtSignIn)
      .catch((e) => setError(String(e)));
    void invoke<Settings>("get_settings")
      .then((s) => setTelemetry(s.telemetry_enabled))
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
  const toggleTelemetry = async () => {
    try {
      const next = !telemetry;
      await invoke("update_settings", {
        patch: { telemetry_enabled: next },
      });
      setTelemetry(next);
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
      <label className="settings-row settings-row--toggle">
        <span className="settings-row__title">Share anonymous crash reports</span>
        <span className="settings-row__subtitle">
          Off by default. When on, panics + caught errors are reported
          to the maintainer&apos;s Sentry instance. No usage data, no
          provider tokens — only the crash backtrace + Windows build.
        </span>
        <input
          type="checkbox"
          checked={telemetry ?? false}
          onChange={() => void toggleTelemetry()}
        />
      </label>
      {error ? <p className="settings-row__error">{error}</p> : null}
    </>
  );
}

interface UpdateInfo {
  current_version: string;
  available_version: string | null;
  release_notes: string | null;
  release_date: string | null;
}

type UpdateUiStage =
  | "idle"
  | "checking"
  | "current"
  | "available"
  | "downloading"
  | "installing"
  | "relaunching"
  | "error";

function AboutPane() {
  const [version, setVersion] = useState<string>("");
  const [reonboardError, setReonboardError] = useState<string | null>(null);
  // Update state — single state machine drives the entire Update card.
  const [stage, setStage] = useState<UpdateUiStage>("idle");
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [progress, setProgress] = useState<{
    downloaded: number;
    total: number | null;
  } | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [stageDetail, setStageDetail] = useState<string | null>(null);

  useEffect(() => {
    void invoke<string>("current_version")
      .then(setVersion)
      .catch(() => setVersion(""));
  }, []);

  // Auto-check on mount so the user sees "Update available" immediately
  // without having to hit a button. Failure is silent — we just stay
  // in `idle`.
  useEffect(() => {
    let cancelled = false;
    const run = async () => {
      setStage("checking");
      try {
        const info = await invoke<UpdateInfo>("check_for_update");
        if (cancelled) return;
        setUpdateInfo(info);
        setStage(info.available_version ? "available" : "current");
      } catch (e) {
        if (cancelled) return;
        setErrorMessage(String(e));
        setStage("error");
      }
    };
    void run();
    return () => {
      cancelled = true;
    };
  }, []);

  // Subscribe to Rust-side progress + stage events emitted by
  // `install_update`. They drive the UI from "downloading" through
  // "installing" → "relaunching" without us needing to poll.
  useEffect(() => {
    const unlistenProgress = listen<{
      downloaded: number;
      total: number | null;
    }>("updater:progress", (e) => setProgress(e.payload));
    const unlistenStage = listen<{
      stage: string;
      detail: string | null;
    }>("updater:stage", (e) => {
      const { stage: s, detail } = e.payload;
      setStageDetail(detail ?? null);
      if (
        s === "checking" ||
        s === "downloading" ||
        s === "installing" ||
        s === "relaunching"
      ) {
        setStage(s);
      } else if (s === "error") {
        setErrorMessage(detail ?? "Update failed");
        setStage("error");
      } else if (s === "done") {
        setStage("current");
      }
    });
    return () => {
      void unlistenProgress.then((f) => f());
      void unlistenStage.then((f) => f());
    };
  }, []);

  const rerunOnboarding = async () => {
    try {
      await invoke("onboarding_reset");
      try {
        await getCurrentWindow().close();
      } catch {
        /* best-effort */
      }
    } catch (e) {
      setReonboardError(String(e));
    }
  };

  const checkForUpdates = async () => {
    setStage("checking");
    setErrorMessage(null);
    try {
      const info = await invoke<UpdateInfo>("check_for_update");
      setUpdateInfo(info);
      setStage(info.available_version ? "available" : "current");
    } catch (e) {
      setErrorMessage(String(e));
      setStage("error");
    }
  };

  const installUpdate = async () => {
    setStage("downloading");
    setProgress(null);
    setErrorMessage(null);
    try {
      await invoke("install_update");
      // On success the Rust side restarts the process, so this line
      // is usually unreachable. Set a safe state in case the restart
      // is deferred (e.g. installer prompts the user).
      setStage("relaunching");
    } catch (e) {
      setErrorMessage(String(e));
      setStage("error");
    }
  };

  // Formatted progress text — null when total unknown.
  const progressPercent =
    progress && progress.total && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null;
  const progressLabel =
    progress != null
      ? progressPercent != null
        ? `${progressPercent}% · ${formatBytes(progress.downloaded)} / ${formatBytes(progress.total ?? 0)}`
        : `${formatBytes(progress.downloaded)} downloaded`
      : null;

  return (
    <div className="settings-about">
      <div className="settings-about__hero">
        <div className="settings-about__icon">
          <Icon name="sparkles" size={40} />
        </div>
        <h3 className="settings-about__name">CodexBar4Windows</h3>
        {version ? (
          <div className="settings-about__version">Version {version}</div>
        ) : null}
        <p className="settings-about__tagline">
          Track your AI coding quota at a glance — Claude, Codex, Cursor,
          and more, right in your tray.
        </p>
      </div>

      <div className="settings-section" style={{ width: "100%", maxWidth: 420 }}>
        <div className="settings-section__caption">Updates</div>
        <div className="settings-section__card">
          <div className="settings-row">
            <div className="settings-row__text">
              <span className="settings-row__title">
                {stage === "available" && updateInfo?.available_version
                  ? `Update available: v${updateInfo.available_version}`
                  : stage === "downloading"
                    ? "Downloading update"
                    : stage === "installing"
                      ? "Installing update"
                      : stage === "relaunching"
                        ? "Restarting CodexBar4Windows…"
                        : stage === "error"
                          ? "Update failed"
                          : "Check for updates"}
              </span>
              <span className="settings-row__subtitle">
                {stage === "available" && updateInfo?.release_date
                  ? `Released ${new Date(updateInfo.release_date).toLocaleDateString()}`
                  : stage === "downloading"
                    ? (progressLabel ?? "Starting…")
                    : stage === "installing"
                      ? (stageDetail ?? "Running installer…")
                      : stage === "relaunching"
                        ? (stageDetail ?? "Hang tight…")
                        : stage === "error"
                          ? (errorMessage ?? "Try again later")
                          : stage === "current"
                            ? "You're on the latest release."
                            : stage === "checking"
                              ? "Looking for a newer version…"
                              : "Verify you're on the latest signed release."}
              </span>
              {/* Progress bar — visible while downloading when we
                  have a total content length. */}
              {stage === "downloading" && progressPercent != null ? (
                <div
                  style={{
                    height: 4,
                    width: "100%",
                    background: "var(--bar-track)",
                    borderRadius: 999,
                    overflow: "hidden",
                    marginTop: 6,
                  }}
                >
                  <div
                    style={{
                      height: "100%",
                      width: `${progressPercent}%`,
                      background: "var(--accent)",
                      transition: "width 120ms linear",
                    }}
                  />
                </div>
              ) : null}
            </div>
            <div className="settings-row__control">
              {stage === "available" ? (
                <button
                  type="button"
                  className="settings-action settings-action--primary"
                  onClick={() => void installUpdate()}
                >
                  <Icon name="download" size={14} />
                  Install &amp; restart
                </button>
              ) : (
                <button
                  type="button"
                  className="settings-action"
                  onClick={() => void checkForUpdates()}
                  disabled={
                    stage === "checking" ||
                    stage === "downloading" ||
                    stage === "installing" ||
                    stage === "relaunching"
                  }
                >
                  <Icon name="refresh" size={14} />
                  {stage === "checking" ? "Checking…" : "Check now"}
                </button>
              )}
            </div>
          </div>
          {/* Release notes for the available version. */}
          {stage === "available" && updateInfo?.release_notes ? (
            <details
              style={{
                padding: "0 var(--space-3) var(--space-3)",
                color: "var(--text-secondary)",
                fontSize: "var(--fs-caption)",
              }}
            >
              <summary
                style={{
                  cursor: "pointer",
                  color: "var(--text-tertiary)",
                  marginBottom: 4,
                }}
              >
                Release notes
              </summary>
              <pre
                style={{
                  whiteSpace: "pre-wrap",
                  wordBreak: "break-word",
                  fontFamily: "inherit",
                  margin: 0,
                  maxHeight: 200,
                  overflowY: "auto",
                }}
              >
                {updateInfo.release_notes}
              </pre>
            </details>
          ) : null}
        </div>
      </div>

      <ul className="settings-about__links">
        <li>
          <button
            type="button"
            className="settings-about__link-row"
            onClick={() =>
              void openUrl("https://github.com/JRub19/CodexBar4Windows").catch(
                () => {},
              )
            }
          >
            <span className="settings-about__link-icon">
              <Icon name="externalLink" size={16} />
            </span>
            <span className="settings-about__link-label">
              Source on GitHub
            </span>
            <Icon
              name="chevronRight"
              size={14}
              style={{ color: "var(--text-tertiary)" }}
            />
          </button>
        </li>
        <li>
          <button
            type="button"
            className="settings-about__link-row"
            onClick={() =>
              void openUrl(
                "https://github.com/JRub19/CodexBar4Windows/issues",
              ).catch(() => {})
            }
          >
            <span className="settings-about__link-icon">
              <Icon name="error" size={16} />
            </span>
            <span className="settings-about__link-label">
              Report an issue
            </span>
            <Icon
              name="chevronRight"
              size={14}
              style={{ color: "var(--text-tertiary)" }}
            />
          </button>
        </li>
        <li>
          <button
            type="button"
            className="settings-about__link-row"
            onClick={() => void rerunOnboarding()}
          >
            <span className="settings-about__link-icon">
              <Icon name="sparkles" size={16} />
            </span>
            <span className="settings-about__link-label">
              Run onboarding again
            </span>
            <Icon
              name="chevronRight"
              size={14}
              style={{ color: "var(--text-tertiary)" }}
            />
          </button>
        </li>
      </ul>

      <p className="settings-about__footnote">
        MIT licensed · © 2026
      </p>

      {reonboardError ? (
        <p className="settings-row__error">{reonboardError}</p>
      ) : null}
    </div>
  );
}
