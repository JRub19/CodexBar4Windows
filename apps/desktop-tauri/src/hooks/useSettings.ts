import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Settings, SettingsChangedPayload } from "../bindings";
import { EVENTS } from "../bindings";

export function useSettings(): Settings | null {
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    let cancelled = false;
    void invoke<Settings>("get_settings").then((value) => {
      if (!cancelled) {
        setSettings(value);
      }
    });

    const unlisten = listen<SettingsChangedPayload>(
      EVENTS.SETTINGS_CHANGED,
      (event) => {
        setSettings(event.payload.settings);
      },
    );

    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, []);

  return settings;
}
