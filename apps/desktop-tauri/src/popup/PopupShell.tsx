import { useEffect, useRef, useState } from "react";
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
import { useAutoResize } from "./state/useAutoResize";
import { useKeyboardNav } from "./a11y/useKeyboardNav";
import { PopupHeader } from "./header/PopupHeader";
import { CardStack } from "./cards/CardStack";
import { PopupFooter } from "./footer/PopupFooter";
import { FirstRunToast } from "./firstRun/FirstRunToast";
import {
  OnboardingShell,
  type OnboardingStateDto,
} from "./onboarding/OnboardingShell";
import { Icon } from "../components/Icon";
import { debugLog } from "./debug/logger";
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
  const selectedProviderId = useUsageStore((s) => s.selectedProviderId);
  const [onboardingActive, setOnboardingActive] = useState<boolean | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  useKeyboardNav();
  useAutoResize(rootRef);

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
    void invoke<SettingsForPopup>("get_settings")
      .then((s) => {
        debugLog.info(
          "PopupShell",
          `get_settings ok: providers=${(s.providers ?? []).length}`,
        );
        applyToggles(s.providers ?? []);
      })
      .catch((err) => {
        debugLog.error("PopupShell", `get_settings failed: ${String(err)}`);
      });
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

  useEffect(() => {
    let cancelled = false;
    void invoke<OnboardingStateDto>("first_run_state")
      .then((s) => {
        debugLog.info(
          "PopupShell",
          `first_run_state ok: onboarding_completed=${String(s.onboarding_completed)}`,
        );
        if (!cancelled) setOnboardingActive(!s.onboarding_completed);
      })
      .catch((err) => {
        debugLog.error("PopupShell", `first_run_state failed: ${String(err)}`);
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
    void invoke<ProviderDescriptorDto[]>("provider_descriptors")
      .then((next) => {
        debugLog.info(
          "PopupShell",
          `provider_descriptors ok: count=${next.length} ids=[${next.map((d) => d.id).join(",")}]`,
        );
        if (!cancelled) setDescriptors(next);
      })
      .catch((err) => {
        debugLog.error(
          "PopupShell",
          `provider_descriptors failed: ${String(err)}`,
        );
      });
    const refetchSnapshots = () =>
      invoke<Record<string, import("./state/usageStore").ProviderSlot>>(
        "provider_snapshots",
      )
        .then((next) => {
          debugLog.info(
            "PopupShell",
            `provider_snapshots ok: keys=${Object.keys(next).length}`,
          );
          if (!cancelled) setSnapshots(next);
        })
        .catch((err) => {
          debugLog.error(
            "PopupShell",
            `provider_snapshots failed: ${String(err)}`,
          );
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
    if (!selectedProviderId) return;
    void invoke("set_active_tray_provider", {
      providerId: selectedProviderId,
    }).catch((err) => {
      debugLog.warn(
        "PopupShell",
        `set_active_tray_provider failed: ${String(err)}`,
      );
    });
  }, [selectedProviderId]);

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
      ref={rootRef}
      className="popup-root"
      role="application"
      aria-label="CodexBar4Windows"
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
