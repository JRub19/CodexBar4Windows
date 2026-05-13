import { useT } from "../../i18n";

// Phase 8 Task 21 step 4: final confirmation card. The user dismisses
// the wizard and the popup falls back to the regular CardStack.

interface Props {
  onFinish: () => void;
  onBack: () => void;
}

export function Step4Done({ onFinish, onBack }: Props) {
  const t = useT();
  return (
    <div className="onboarding-step">
      <h2 className="onboarding-step__title" id="onboarding-title">
        {t("onboarding.title.done")}
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        {t("onboarding.body.done")}
      </p>
      <ul className="onboarding-step__bullets">
        <li>{t("onboarding.bullet.done.rerun")}</li>
        <li>{t("onboarding.bullet.done.cadence")}</li>
        <li>{t("onboarding.bullet.done.pin")}</li>
      </ul>
      <div className="onboarding-step__actions">
        <button type="button" className="btn-secondary" onClick={onBack}>
          {t("common.button.back")}
        </button>
        <button type="button" className="btn-primary" onClick={onFinish}>
          {t("onboarding.button.finish")}
        </button>
      </div>
    </div>
  );
}
