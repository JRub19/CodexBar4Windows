import React from "react";
import ReactDOM from "react-dom/client";
import { PopupShell } from "./popup";
import { SettingsApp } from "./settings/SettingsApp";
import { CostPopoverApp } from "./popup/cost/CostPopoverApp";
import { I18nProvider } from "./i18n";
import { ErrorBoundary } from "./popup/debug/ErrorBoundary";
import { debugLog } from "./popup/debug/logger";

// Route by URL hash. Three top-level entry points share index.html:
//   #/settings        → preferences window
//   #/cost-popover    → floating cost-history side panel
//   <no hash>         → main tray popup
const isSettingsRoute = window.location.hash.startsWith("#/settings");
const isCostPopoverRoute = window.location.hash.startsWith("#/cost-popover");

debugLog.info(
  "main.tsx",
  `route detection href=${window.location.href} hash=${window.location.hash} settings=${isSettingsRoute} costPopover=${isCostPopoverRoute}`,
);

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
            {isSettingsRoute ? (
              <SettingsApp />
            ) : isCostPopoverRoute ? (
              <CostPopoverApp />
            ) : (
              <PopupShell />
            )}
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
