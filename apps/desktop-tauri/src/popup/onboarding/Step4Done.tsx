// Phase 8 Task 21 step 4: final confirmation card. The user dismisses
// the wizard and the popup falls back to the regular CardStack.

interface Props {
  onFinish: () => void;
  onBack: () => void;
}

export function Step4Done({ onFinish, onBack }: Props) {
  return (
    <div className="onboarding-step">
      <h2 className="onboarding-step__title" id="onboarding-title">
        You're all set
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        CodexBar will refresh in the background. Click the tray icon
        anytime to open the popup, or press Win+Shift+U.
      </p>
      <ul className="onboarding-step__bullets">
        <li>Re-run this wizard from Preferences → About.</li>
        <li>Quotas, status, and toasts update every minute.</li>
        <li>Pin the tray icon so it always stays visible.</li>
      </ul>
      <div className="onboarding-step__actions">
        <button type="button" className="btn-secondary" onClick={onBack}>
          Back
        </button>
        <button type="button" className="btn-primary" onClick={onFinish}>
          Finish
        </button>
      </div>
    </div>
  );
}
