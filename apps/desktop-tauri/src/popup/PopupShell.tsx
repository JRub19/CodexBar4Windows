import { useEffect, useState } from "react";
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
import { UpdateBanner } from "./header/UpdateBanner";
import { CardStack } from "./cards/CardStack";
import { PopupFooter } from "./footer/PopupFooter";
import { FirstRunToast } from "./firstRun/FirstRunToast";
import { OnboardingShell, type OnboardingStateDto } from "./onboarding/OnboardingShell";
import { ProviderSettingsPanel } from "./settings/ProviderSettingsPanel";
import { EmptyState } from "../components/EmptyState";
import "../styles/popup.css";
import "../styles/focus.css";
import "../styles/reduced-motion.css";

// Phase 3 D1: The top level popup layout. Holds the three card regions
// (header, body, footer) and is responsible for the popup-wide listeners:
// descriptor fetch, usage event stream, status event stream, and escape
// to dismiss. Phase 4 P4-19 swaps the body for the settings pane when
// the user clicks the cog.

export function PopupShell() {
  const descriptors = useUsageStore((s) => s.descriptors);
  const setDescriptors = useUsageStore((s) => s.setDescriptors);
  const setSnapshots = useUsageStore((s) => s.setSnapshots);
  const applyUsageEvent = useUsageStore((s) => s.applyUsageEvent);
  const applyStatusEvent = useUsageStore((s) => s.applyStatusEvent);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [onboardingActive, setOnboardingActive] = useState<boolean | null>(null);
  useKeyboardNav();

  useEffect(() => {
    let cancelled = false;
    void invoke<OnboardingStateDto>("first_run_state").then((s) => {
      if (!cancelled) setOnboardingActive(!s.onboarding_completed);
    });
    const unlisten = listen<OnboardingStateDto>("onboarding:state", (event) => {
      // Re-show the wizard whenever the back-end resets the flag,
      // e.g. from the About-pane "Run onboarding again" button.
      setOnboardingActive(!event.payload.onboarding_completed);
    });
    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void invoke<ProviderDescriptorDto[]>("provider_descriptors").then(
      (next) => {
        if (!cancelled) setDescriptors(next);
      },
    );
    const refetchSnapshots = () =>
      invoke<Record<string, import("./state/usageStore").ProviderSlot>>(
        "provider_snapshots",
      ).then((next) => {
        if (!cancelled) setSnapshots(next);
      });
    void refetchSnapshots();
    const unlistenUsage = listen<UsageEventPayload>(
      EVENTS.USAGE_UPDATED,
      (event) => {
        applyUsageEvent(event.payload);
        void refetchSnapshots();
      },
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
  }, [setDescriptors, setSnapshots, applyUsageEvent, applyStatusEvent]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        if (settingsOpen) {
          setSettingsOpen(false);
        } else {
          void getCurrentWindow().hide();
        }
        return;
      }
      // Phase 9 §B-6: Ctrl+R refreshes the popup without closing it.
      // The default browser refresh would reload the WebView and
      // remount everything, dropping in-flight state; explicit
      // refresh_now invoke keeps the popup mounted and just kicks the
      // backend refresh loop. Browser F5 / Ctrl+F5 are also caught
      // (WebView2 honours them otherwise).
      if (
        (event.ctrlKey && (event.key === "r" || event.key === "R")) ||
        event.key === "F5"
      ) {
        event.preventDefault();
        void invoke("refresh_now").catch(() => {
          // Best-effort; the refresh loop swallows errors internally.
        });
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [settingsOpen]);

  return (
    <div
      className="popup-root"
      role="application"
      aria-label="CodexBar4Windows popup"
    >
      <UpdateBanner />
      <PopupHeader onOpenSettings={() => setSettingsOpen(true)} />
      <main className="popup-body">
        {onboardingActive ? (
          <OnboardingShell onFinish={() => setOnboardingActive(false)} />
        ) : settingsOpen ? (
          <ProviderSettingsPanel onClose={() => setSettingsOpen(false)} />
        ) : descriptors.length === 0 ? (
          <EmptyState />
        ) : (
          <CardStack />
        )}
      </main>
      <PopupFooter />
      <FirstRunToast />
    </div>
  );
}
