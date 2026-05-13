import { useT } from "../../i18n";
import { Icon } from "../../components/Icon";

// Step 4: Done. Confirms onboarding completed. The check-mark hero
// signals success without being chirpy.

interface Props {
  onFinish: () => void;
  onBack: () => void;
}

export function Step4Done({ onFinish, onBack }: Props) {
  const t = useT();
  return (
    <div className="onboarding-step">
      <div
        className="onboarding-step__hero"
        style={{
          background: "color-mix(in srgb, var(--text-success) 14%, transparent)",
          color: "var(--text-success)",
        }}
      >
        <Icon name="check" size={28} strokeWidth={2} />
      </div>
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
        <button type="button" className="btn-ghost" onClick={onBack}>
          <Icon name="chevronLeft" size={14} />
          {t("common.button.back")}
        </button>
        <button type="button" className="btn-primary" onClick={onFinish}>
          <Icon name="check" size={14} />
          {t("onboarding.button.finish")}
        </button>
      </div>
    </div>
  );
}
