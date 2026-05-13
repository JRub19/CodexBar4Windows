import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderDescriptorDto } from "../../bindings";

// Phase 8 Task 21 step 3: per-provider sign-in. For each picked
// provider we surface a "Sign in" button that triggers the same
// strategy the regular settings pane uses. The button is informational
// only — opening Preferences for the OAuth dance keeps this wizard
// thin and avoids duplicating the strategy machinery. The user can
// always come back to Preferences later.

interface Props {
  pickedProviders: string[];
  onNext: () => void;
  onBack: () => void;
}

export function Step3SignIn({ pickedProviders, onNext, onBack }: Props) {
  const [descriptors, setDescriptors] = useState<ProviderDescriptorDto[]>([]);

  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderDescriptorDto[]>("provider_descriptors").then((d) => {
      if (!cancelled) setDescriptors(d);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const picked = descriptors.filter((d) => pickedProviders.includes(d.id));

  const openSettingsForProvider = async (_id: string) => {
    // Opens the preferences window — the provider pane in there
    // shows the per-strategy sign-in widgets. Per-provider focus
    // routing is a future preferences-pane enhancement.
    try {
      await invoke("open_preferences");
    } catch {
      // open_preferences is fire-and-forget; ignore errors.
    }
  };

  return (
    <div className="onboarding-step">
      <h2 className="onboarding-step__title" id="onboarding-title">
        Sign in
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        Sign in to each provider so CodexBar can read your usage. The
        sign-in window opens in Preferences; come back here when done.
      </p>
      {picked.length === 0 ? (
        <p className="onboarding-step__hint">
          No providers selected. You can skip this step and add accounts
          later from Preferences.
        </p>
      ) : (
        <ul className="onboarding-signin-list">
          {picked.map((d) => (
            <li key={d.id} className="onboarding-signin-row">
              <span
                className="onboarding-provider-row__swatch"
                style={{ backgroundColor: d.branding.accent_hex }}
                aria-hidden="true"
              />
              <span className="onboarding-signin-row__name">
                {d.metadata.display_name}
              </span>
              <button
                type="button"
                className="btn-link"
                onClick={() => void openSettingsForProvider(d.id)}
              >
                Sign in →
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="onboarding-step__actions">
        <button type="button" className="btn-secondary" onClick={onBack}>
          Back
        </button>
        <button type="button" className="btn-primary" onClick={onNext}>
          Next
        </button>
      </div>
    </div>
  );
}
