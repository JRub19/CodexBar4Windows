import React from "react";
import ReactDOM from "react-dom/client";
import { PopupShell } from "./popup";
import { SettingsApp } from "./settings/SettingsApp";

// Phase 8 task 1: route by URL hash. The Tauri shell opens the
// preferences window with `index.html#/settings`; the tray popup
// continues to load on bare `index.html`. Two-window apps don't
// need react-router.
const isSettingsRoute = window.location.hash.startsWith("#/settings");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isSettingsRoute ? <SettingsApp /> : <PopupShell />}
  </React.StrictMode>,
);
