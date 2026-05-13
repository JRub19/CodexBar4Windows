import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderDescriptorDto } from "../../bindings";

// Phase 8 Task 21 step 2: provider picker. Lists every provider the
// registry exposes with a checkbox; the picked set is forwarded to
// step 3 so the sign-in flow can iterate.
//
// Mirrors the macOS multi-select where the user can tick zero, one,
// or many providers — the wizard validates that at least one is
// chosen before enabling Next.

interface Props {
  picked: string[];
  setPicked: (next: string[]) => void;
  onNext: () => void;
  onBack: () => void;
}

export function Step2Providers({ picked, setPicked, onNext, onBack }: Props) {
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
        Pick your providers
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        Choose the AI services you use. You can change this later in
        Preferences → Providers.
      </p>
      <div className="onboarding-providers" role="group" aria-label="Providers">
        {loading && <p className="onboarding-step__hint">Loading…</p>}
        {!loading && descriptors.length === 0 && (
          <p className="onboarding-step__hint">
            No providers available yet. You can finish onboarding and add
            them later in Preferences.
          </p>
        )}
        {descriptors.map((d) => (
          <label key={d.id} className="onboarding-provider-row">
            <input
              type="checkbox"
              checked={picked.includes(d.id)}
              onChange={() => toggle(d.id)}
            />
            <span
              className="onboarding-provider-row__swatch"
              style={{ backgroundColor: d.branding.accent_hex }}
              aria-hidden="true"
            />
            <span className="onboarding-provider-row__name">
              {d.metadata.display_name}
            </span>
          </label>
        ))}
      </div>
      <div className="onboarding-step__actions">
        <button type="button" className="btn-secondary" onClick={onBack}>
          Back
        </button>
        <button
          type="button"
          className="btn-primary"
          onClick={onNext}
          disabled={!canAdvance && descriptors.length > 0}
        >
          Next
        </button>
      </div>
    </div>
  );
}
