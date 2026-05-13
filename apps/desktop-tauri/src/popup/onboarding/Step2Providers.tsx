import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderDescriptorDto } from "../../bindings";
import { useT } from "../../i18n";
import { Icon } from "../../components/Icon";

// Step 2: pick providers. Multi-select with brand swatches; Next is
// disabled until at least one provider is picked.

interface Props {
  picked: string[];
  setPicked: (next: string[]) => void;
  onNext: () => void;
  onBack: () => void;
}

export function Step2Providers({ picked, setPicked, onNext, onBack }: Props) {
  const t = useT();
  const [descriptors, setDescriptors] = useState<ProviderDescriptorDto[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderDescriptorDto[]>("provider_descriptors")
      .then((next) => {
        if (!cancelled) {
          setDescriptors(next);
          setLoading(false);
        }
      })
      .catch(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const toggle = (id: string) => {
    if (picked.includes(id)) {
      setPicked(picked.filter((p) => p !== id));
    } else {
      setPicked([...picked, id]);
    }
  };

  const canAdvance = picked.length > 0;

  return (
    <div className="onboarding-step">
      <h2 className="onboarding-step__title" id="onboarding-title">
        {t("onboarding.title.providers")}
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        {t("onboarding.body.providers")}
      </p>
      <div className="onboarding-providers" role="group" aria-label="Providers">
        {loading && (
          <p className="onboarding-step__hint">
            {t("onboarding.hint.providers.loading")}
          </p>
        )}
        {!loading && descriptors.length === 0 && (
          <p className="onboarding-step__hint">
            {t("onboarding.hint.providers.empty")}
          </p>
        )}
        {descriptors.map((d) => {
          const isPicked = picked.includes(d.id);
          return (
            <label
              key={d.id}
              className="onboarding-provider-row"
              style={
                isPicked
                  ? { background: "var(--accent-soft)" }
                  : undefined
              }
            >
              <input
                type="checkbox"
                checked={isPicked}
                onChange={() => toggle(d.id)}
                style={{ display: "none" }}
              />
              <span
                className="onboarding-provider-row__swatch"
                style={{ background: d.branding.accent_hex }}
                aria-hidden="true"
              />
              <span className="onboarding-provider-row__name">
                {d.metadata.display_name}
              </span>
              {isPicked ? (
                <Icon
                  name="check"
                  size={16}
                  strokeWidth={2}
                  style={{ color: "var(--accent)", marginLeft: "auto" }}
                />
              ) : null}
            </label>
          );
        })}
      </div>
      <div className="onboarding-step__actions">
        <button type="button" className="btn-ghost" onClick={onBack}>
          <Icon name="chevronLeft" size={14} />
          {t("common.button.back")}
        </button>
        <button
          type="button"
          className="btn-primary"
          onClick={onNext}
          disabled={!canAdvance}
        >
          {t("common.button.next")}
          <Icon name="chevronRight" size={14} />
        </button>
      </div>
    </div>
  );
}
