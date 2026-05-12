import { invoke } from "@tauri-apps/api/core";
import { EmptyState } from "./components/EmptyState";
import { useSettings } from "./hooks/useSettings";
import { useUsageEvents } from "./hooks/useUsageEvents";
import "./styles/popup.css";

function App() {
  const settings = useSettings();
  const { descriptors, lastUpdate } = useUsageEvents();

  return (
    <div className="popup-root">
      <header className="popup-header">CodexBar</header>
      <main className="popup-body">
        {descriptors.length === 0 ? (
          <EmptyState />
        ) : (
          <p>
            {descriptors.length} provider
            {descriptors.length === 1 ? "" : "s"} configured.
            {lastUpdate ? ` Last update: ${lastUpdate.provider}` : ""}
          </p>
        )}
      </main>
      <footer className="popup-footer">
        <button
          type="button"
          onClick={() => {
            void invoke("refresh_now");
          }}
        >
          Refresh now
        </button>
        <span>
          {settings
            ? settings.pause_refresh
              ? "Refresh paused"
              : `Cadence: ${settings.refresh_frequency}`
            : "Loading..."}
        </span>
      </footer>
    </div>
  );
}

export default App;
