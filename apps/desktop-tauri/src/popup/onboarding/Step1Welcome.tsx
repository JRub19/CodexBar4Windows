import { useT } from "../../i18n";

// Phase 8 Task 21 step 1: a static welcome card explaining what the
// app does, with a single "Get started" button that advances to the
// provider picker.

interface Props {
  onNext: () => void;
}

export function Step1Welcome({ onNext }: Props) {
  const t = useT();
  return (
    <div className="onboarding-step">
      <h2 className="onboarding-step__title" id="onboarding-title">
        {t("onboarding.title.welcome")}
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        {t("onboarding.body.welcome")}
      </p>
      <ul className="onboarding-step__bullets">
        <li>{t("onboarding.bullet.welcome.bars")}</li>
        <li>{t("onboarding.bullet.welcome.toasts")}</li>
        <li>{t("onboarding.bullet.welcome.hotkey")}</li>
      </ul>
      <div className="onboarding-step__actions">
        <button type="button" className="btn-primary" onClick={onNext}>
          {t("onboarding.button.get_started")}
        </button>
      </div>
    </div>
  );
}
