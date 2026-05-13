import { useT } from "../../i18n";
import { Icon } from "../../components/Icon";

// Step 1: Welcome. Hero icon + headline + body + a single primary
// CTA. No back button (this is the first step).

interface Props {
  onNext: () => void;
}

export function Step1Welcome({ onNext }: Props) {
  const t = useT();
  return (
    <div className="onboarding-step">
      <div className="onboarding-step__hero">
        <Icon name="sparkles" size={28} />
      </div>
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
      <div className="onboarding-step__actions onboarding-step__actions--end">
        <button type="button" className="btn-primary" onClick={onNext}>
          {t("onboarding.button.get_started")}
          <Icon name="chevronRight" size={14} />
        </button>
      </div>
    </div>
  );
}
