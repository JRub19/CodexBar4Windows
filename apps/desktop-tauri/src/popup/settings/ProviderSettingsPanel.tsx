import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  ProviderSettingsSnapshot,
  SettingsDescriptor,
} from "../../bindings";
import { TokenAccountsRow } from "./TokenAccountsRow";
import { CopilotLoginButton } from "./CopilotLoginButton";

// Phase 4 P4-19: renders the per-provider settings rows produced by the
// `provider_settings_descriptors` Tauri command. Each descriptor variant
// has a tiny dedicated renderer; the panel itself stays generic so a
// new provider's settings show up without any React work.

interface Props {
  onClose: () => void;
}

export function ProviderSettingsPanel({ onClose }: Props) {
  const [snapshot, setSnapshot] = useState<ProviderSettingsSnapshot | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderSettingsSnapshot>("provider_settings_descriptors")
      .then((next) => {
        if (!cancelled) setSnapshot(next);
      })
      .catch((e) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="settings-pane" role="dialog" aria-label="Provider settings">
      <header className="settings-pane__header">
        <span className="settings-pane__title">Preferences</span>
        <button
          type="button"
          className="settings-pane__close"
          onClick={onClose}
          aria-label="Close"
        >
          ×
        </button>
      </header>
      <div className="settings-pane__body">
        {error ? <p className="settings-pane__error">{error}</p> : null}
        {snapshot ? (
          snapshot.sections.length === 0 ? (
            <p className="settings-pane__empty">
              No provider settings to show yet.
            </p>
          ) : (
            snapshot.sections.map((section) => (
              <section key={section.provider_id} className="settings-pane__section">
                <h2 className="settings-pane__section-title">
                  {section.section_title}
                </h2>
                {section.rows.map((row, idx) => (
                  <DescriptorRow key={idx} descriptor={row} />
                ))}
                {section.provider_id === "copilot" ? (
                  <div className="settings-row settings-row--login">
                    <CopilotLoginButton />
                  </div>
                ) : null}
              </section>
            ))
          )
        ) : (
          <p className="settings-pane__loading">Loading…</p>
        )}
      </div>
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
