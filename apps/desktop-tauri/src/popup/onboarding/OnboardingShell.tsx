import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Step1Welcome } from "./Step1Welcome";
import { Step2Providers } from "./Step2Providers";
import { Step3SignIn } from "./Step3SignIn";
import { Step4Done } from "./Step4Done";

// Phase 8 Task 21: the four-step onboarding wizard rendered inline in
// the popup body until `onboarding_completed = true`. Mirrors the
// macOS `OnboardingFlow` so a fresh install lands on Step 1 and walks
// the user through provider picking + sign-in before the regular
// usage cards appear.

export type OnboardingStep = "welcome" | "providers" | "sign_in" | "done";

export interface OnboardingStateDto {
  tray_pinned_hint_shown: boolean;
  onboarding_completed: boolean;
  onboarding_step: OnboardingStep;
}

interface Props {
  /** Called after the user reaches Step 4 and dismisses the wizard. */
  onFinish: () => void;
}

export function OnboardingShell({ onFinish }: Props) {
  const [step, setStep] = useState<OnboardingStep>("welcome");
  const [pickedProviders, setPickedProviders] = useState<string[]>([]);

  useEffect(() => {
    let cancelled = false;
    void invoke<OnboardingStateDto>("first_run_state").then((s) => {
      if (!cancelled) setStep(s.onboarding_step);
    });
    const unlisten = listen<OnboardingStateDto>("onboarding:state", (event) => {
      setStep(event.payload.onboarding_step);
    });
    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, []);

  const advance = useCallback(async () => {
    const s = await invoke<OnboardingStateDto>("onboarding_advance");
    setStep(s.onboarding_step);
  }, []);

  const rewind = useCallback(async () => {
    const s = await invoke<OnboardingStateDto>("onboarding_rewind");
    setStep(s.onboarding_step);
  }, []);

  const complete = useCallback(async () => {
    await invoke("onboarding_complete");
    onFinish();
  }, [onFinish]);

  return (
    <div
      className="onboarding-shell"
      role="dialog"
      aria-labelledby="onboarding-title"
      aria-describedby="onboarding-body"
    >
      <ProgressDots current={step} />
      {step === "welcome" && <Step1Welcome onNext={advance} />}
      {step === "providers" && (
        <Step2Providers
          picked={pickedProviders}
          setPicked={setPickedProviders}
          onNext={advance}
          onBack={rewind}
        />
      )}
      {step === "sign_in" && (
        <Step3SignIn
          pickedProviders={pickedProviders}
          onNext={advance}
          onBack={rewind}
        />
      )}
      {step === "done" && <Step4Done onFinish={complete} onBack={rewind} />}
    </div>
  );
}

const STEPS: OnboardingStep[] = ["welcome", "providers", "sign_in", "done"];

function ProgressDots({ current }: { current: OnboardingStep }) {
  const currentIdx = STEPS.indexOf(current);
  return (
    <div className="onboarding-progress" aria-hidden="true">
      {STEPS.map((s, i) => (
        <span
          key={s}
          className={
            "onboarding-progress__dot" +
            (i <= currentIdx ? " onboarding-progress__dot--filled" : "")
          }
        />
      ))}
    </div>
  );
}
