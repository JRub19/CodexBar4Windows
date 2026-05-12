import { invoke } from "@tauri-apps/api/core";

export function EmptyState() {
  return (
    <section className="empty-state" aria-live="polite">
      <h2>No providers configured</h2>
      <p>
        Open Preferences to enable the AI coding providers you want to track in
        your tray.
      </p>
      <button
        type="button"
        onClick={() => {
          void invoke("open_preferences");
        }}
      >
        Open Preferences
      </button>
    </section>
  );
}
