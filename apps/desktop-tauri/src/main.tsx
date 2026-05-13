import React from "react";
import ReactDOM from "react-dom/client";
import { PopupShell } from "./popup";
import { SettingsApp } from "./settings/SettingsApp";
import { I18nProvider } from "./i18n";
import { ErrorBoundary } from "./popup/debug/ErrorBoundary";
import { debugLog } from "./popup/debug/logger";

// Phase 8 task 1: route by URL hash. The Tauri shell opens the
// preferences window with `index.html#/settings`; the tray popup
// continues to load on bare `index.html`. Two-window apps don't
// need react-router.
const isSettingsRoute = window.location.hash.startsWith("#/settings");

debugLog.info(
  "main.tsx",
  `boot route=${isSettingsRoute ? "settings" : "popup"} href=${window.location.href}`,
);

const rootEl = document.getElementById("root");
if (!rootEl) {
  debugLog.error("main.tsx", "no #root element in document");
} else {
  debugLog.info("main.tsx", "creating React root");
  try {
    ReactDOM.createRoot(rootEl).render(
      <React.StrictMode>
        <ErrorBoundary>
          <I18nProvider>
            {isSettingsRoute ? <SettingsApp /> : <PopupShell />}
          </I18nProvider>
        </ErrorBoundary>
      </React.StrictMode>,
    );
    debugLog.info("main.tsx", "React render() returned");
  } catch (err) {
    debugLog.error(
      "main.tsx",
      `createRoot threw: ${err instanceof Error ? `${err.message}\n${err.stack}` : String(err)}`,
    );
  }
}
