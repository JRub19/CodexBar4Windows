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
import { useAutoResize } from "./state/useAutoResize";
import { PopupHeader } from "./header/PopupHeader";
import { CardStack } from "./cards/CardStack";
import { PopupFooter } from "./footer/PopupFooter";
import { FirstRunToast } from "./firstRun/FirstRunToast";
import {
  OnboardingShell,
  type OnboardingStateDto,
} from "./onboarding/OnboardingShell";
import { Icon } from "../components/Icon";
import "../styles/popup.css";
import "../styles/focus.css";
import "../styles/reduced-motion.css";

// Top-level popup. The window is fixed-width (360 px) and the height
// is content-sized; the body region scrolls when content overflows.
//
// Layout (top → bottom):
//   PopupHeader  — switcher only, when ≥2 providers
//   PopupBody    — CardStack OR onboarding wizard OR empty hint
//   PopupFooter  — stacked action rows
//   FirstRunToast — pinned bottom-right when not yet shown
//
// The popup auto-hides on focus loss (handled in the Tauri Rust side).

interface ProviderToggle {
  id: string;
  enabled: boolean;
  order?: number;
}

interface SettingsForPopup {
  providers: ProviderToggle[];
}

export function PopupShell() {
  const descriptors = useUsageStore((s) => s.descriptors);
  const setDescriptors = useUsageStore((s) => s.setDescriptors);
  const setEnabledProviderIds = useUsageStore((s) => s.setEnabledProviderIds);
  const setSnapshots = useUsageStore((s) => s.setSnapshots);
  const applyUsageEvent = useUsageStore((s) => s.applyUsageEvent);
  const applyStatusEvent = useUsageStore((s) => s.applyStatusEvent);
  const [onboardingActive, setOnboardingActive] = useState<boolean | null>(null);
  useKeyboardNav();
  const rootRef = useAutoResize();

  // Fetch settings + listen for live changes so the popup filters to
  // the providers the user has actually enabled. Empty `providers`
  // means "no preference set" — treat every registered descriptor as
  // enabled (matches the refresh-loop's semantics).
  useEffect(() => {
    let cancelled = false;
    const applyToggles = (toggles: ProviderToggle[]) => {
      if (cancelled) return;
      if (toggles.length === 0) {
        setEnabledProviderIds(null);
        return;
      }
      setEnabledProviderIds(
        toggles.filter((t) => t.enabled).map((t) => t.id),
      );
    };
    void invoke<SettingsForPopup>("get_settings").then((s) =>
      applyToggles(s.providers ?? []),
    );
    const unlisten = listen<{ settings: SettingsForPopup }>(
      "settings:changed",
      (event) => {
        applyToggles(event.payload?.settings?.providers ?? []);
      },
    );
    return () => {
      cancelled = true;
      void unlisten.then((f) => f());
    };
  }, [setEnabledProviderIds]);

  // Boot auto-import: kick off the browser-cookie auto-import for the
  // providers that support it (Cursor, Factory). The Tauri command
  // walks Edge/Chrome/Brave/Firefox DPAPI-decoded cookie jars, finds
  // the provider's session cookie, and stores it in the DPAPI-wrapped
  // TokenAccountStore so the next refresh tick can fetch usage data
  // without the user pasting anything. Errors are logged but never
  // shown — auto-import is opportunistic, not required.
  //
  // After auto-import settles we trigger a manual refresh so the
  // cards populate immediately instead of waiting for the 5-minute
  // cadence tick.
  useEffect(() => {
    let cancelled = false;
    const AUTO_IMPORT_PROVIDERS = ["cursor", "factory"];

    async function bootstrap() {
      try {
        const s = await invoke<{ allow_browser_cookie_import: boolean }>(
          "get_settings",
        );
        if (!s.allow_browser_cookie_import || cancelled) return;
      } catch {
        // If get_settings fails we still attempt the refresh — the
        // backend gates auto-import internally per provider anyway.
      }
      // Fire each provider's auto-import in parallel.
      await Promise.allSettled(
        AUTO_IMPORT_PROVIDERS.map((id) =>
          invoke("auto_import_cookies", { providerId: id }),
        ),
      );
      if (cancelled) return;
      // Trigger an immediate refresh so the cards aren't stuck in
      // the loading skeleton waiting for the cadence tick.
      try {
        await invoke("refresh_now");
      } catch {
        /* swallow — refresh errors surface via refresh_error events */
      }
    }

    void bootstrap();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    void invoke<OnboardingStateDto>("first_run_state").then((s) => {
      if (!cancelled) setOnboardingActive(!s.onboarding_completed);
    });
    const unlisten = listen<OnboardingStateDto>(
      "onboarding:state",
      (event) => {
        setOnboardingActive(!event.payload.onboarding_completed);
      },
    );
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
        void getCurrentWindow().hide();
        return;
      }
      if (
        (event.ctrlKey && (event.key === "r" || event.key === "R")) ||
        event.key === "F5"
      ) {
        event.preventDefault();
        void invoke("refresh_now").catch(() => {});
      }
      if (event.ctrlKey && event.key === ",") {
        event.preventDefault();
        void invoke("open_preferences").catch(() => {});
      }
      if (event.ctrlKey && (event.key === "q" || event.key === "Q")) {
        event.preventDefault();
        void invoke("quit_app").catch(() => {});
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div
      className="popup-root"
      role="application"
      aria-label="CodexBar4Windows"
      ref={rootRef}
    >
      <PopupHeader />
      <main className="popup-body">
        {onboardingActive ? (
          <OnboardingShell onFinish={() => setOnboardingActive(false)} />
        ) : descriptors.length === 0 ? (
          <PopupEmpty />
        ) : (
          <CardStack />
        )}
      </main>
      <PopupFooter />
      <FirstRunToast />
    </div>
  );
}

// Empty popup — no providers configured yet. The footer Settings row
// is the primary path forward, but we surface it as a hero CTA here
// so the user doesn't have to hunt for it.
function PopupEmpty() {
  return (
    <div className="empty-state">
      <div className="empty-state__icon">
        <Icon name="sparkles" size={24} />
      </div>
      <div className="empty-state__title">No providers yet</div>
      <p className="empty-state__body">
        Add a provider in Settings to start tracking your AI coding quota.
      </p>
      <div className="empty-state__cta">
        <button
          type="button"
          className="btn-primary"
          onClick={() => void invoke("open_preferences")}
        >
          <Icon name="settings" size={14} />
          Open Settings
        </button>
      </div>
    </div>
  );
}
