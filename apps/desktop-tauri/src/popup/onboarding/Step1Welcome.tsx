// Phase 8 Task 21 step 1: a static welcome card explaining what the
// app does, with a single "Get started" button that advances to the
// provider picker.

interface Props {
  onNext: () => void;
}

export function Step1Welcome({ onNext }: Props) {
  return (
    <div className="onboarding-step">
      <h2 className="onboarding-step__title" id="onboarding-title">
        Welcome to CodexBar4Windows
      </h2>
      <p className="onboarding-step__body" id="onboarding-body">
        Track your AI coding usage at a glance. CodexBar lives in the
        system tray and shows your quota for Claude, Codex, GitHub
        Copilot, and more — all without leaving your editor.
      </p>
      <ul className="onboarding-step__bullets">
        <li>One bar per provider, primary plus secondary windows.</li>
        <li>Smart toasts when you near a quota threshold.</li>
        <li>Open the popup with Win+Shift+U (rebindable).</li>
      </ul>
      <div className="onboarding-step__actions">
        <button type="button" className="btn-primary" onClick={onNext}>
          Get started
        </button>
      </div>
    </div>
  );
}
