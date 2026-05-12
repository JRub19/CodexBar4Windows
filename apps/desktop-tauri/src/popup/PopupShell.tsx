import { useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { EVENTS } from "../bindings";
import type {
  ProviderDescriptorDto,
  StatusEventPayload,
  UsageEventPayload,
} from "../bindings";
import { useUsageStore } from "./state/usageStore";
import { useKeyboardNav } from "./a11y/useKeyboardNav";
import { PopupHeader } from "./header/PopupHeader";
import { CardStack } from "./cards/CardStack";
import { PopupFooter } from "./footer/PopupFooter";
import { FirstRunToast } from "./firstRun/FirstRunToast";
import { EmptyState } from "../components/EmptyState";
import "../styles/popup.css";

// Phase 3 D1: The top level popup layout. Holds the three card regions
// (header, body, footer) and is responsible for the popup-wide listeners:
// descriptor fetch, usage event stream, status event stream, and escape
// to dismiss. Sub regions read from the Zustand `useUsageStore`.

export function PopupShell() {
  const descriptors = useUsageStore((s) => s.descriptors);
  const setDescriptors = useUsageStore((s) => s.setDescriptors);
  const applyUsageEvent = useUsageStore((s) => s.applyUsageEvent);
  const applyStatusEvent = useUsageStore((s) => s.applyStatusEvent);
  useKeyboardNav();

  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderDescriptorDto[]>("provider_descriptors").then(
      (next) => {
        if (!cancelled) setDescriptors(next);
      },
    );
    const unlistenUsage = listen<UsageEventPayload>(
      EVENTS.USAGE_UPDATED,
      (event) => applyUsageEvent(event.payload),
    );
    const unlistenStatus = listen<StatusEventPayload>(
      EVENTS.STATUS_UPDATED,
      (event) => applyStatusEvent(event.payload),
    );
    return () => {
      cancelled = true;
      void unlistenUsage.then((f) => f());
      void unlistenStatus.then((f) => f());
    };
  }, [setDescriptors, applyUsageEvent, applyStatusEvent]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void getCurrentWindow().hide();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="popup-root">
      <PopupHeader />
      <main className="popup-body">
        {descriptors.length === 0 ? <EmptyState /> : <CardStack />}
      </main>
      <PopupFooter />
      <FirstRunToast />
    </div>
  );
}
