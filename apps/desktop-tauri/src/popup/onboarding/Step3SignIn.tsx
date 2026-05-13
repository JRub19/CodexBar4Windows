import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderDescriptorDto } from "../../bindings";
import { useT } from "../../i18n";
import { Icon } from "../../components/Icon";

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
  const t = useT();
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

  const openSettingsForProvider = async (id: string) => {
    // Opens the preferences window focused on the picked provider.
    // The Tauri command emits `preferences:focus_provider` so the
    // Settings React side switches to the Providers pane and scrolls
    // the matching row into view.
    try {
      await invoke("open_preferences", { providerId: id });
    } catch {
      // open_preferences is fire-and-forget; ignore errors.
    }
  };

  return (
    <div className="onboarding-step">
      <h2 className="onboarding-step__title" id="onboarding-title">
        {t("onboarding.title.sign_in")}
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        {t("onboarding.body.sign_in")}
      </p>
      {picked.length === 0 ? (
        <p className="onboarding-step__hint">
          {t("onboarding.hint.sign_in.empty")}
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
                style={{ marginLeft: "auto" }}
              >
                {t("onboarding.button.sign_in")}
                <Icon name="externalLink" size={12} />
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="onboarding-step__actions">
        <button type="button" className="btn-ghost" onClick={onBack}>
          <Icon name="chevronLeft" size={14} />
          {t("common.button.back")}
        </button>
        <button type="button" className="btn-primary" onClick={onNext}>
          {t("common.button.next")}
          <Icon name="chevronRight" size={14} />
        </button>
      </div>
    </div>
  );
}
