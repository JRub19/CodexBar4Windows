import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../../i18n";

// Phase 3 D12: 3 s after the popup mounts on a fresh install, we show
// an inline toast that hints the user to drag the tray icon out of the
// overflow flyout. Native `Shell_NotifyIcon` `NIF_INFO` balloons aren't
// reachable from the tray-icon crate today, so the toast is rendered
// inside the popup itself; the persisted flag still gates whether we
// show it ever again, so the experience is one-shot per install.

interface FirstRunStateDto {
  tray_pinned_hint_shown: boolean;
}

export function FirstRunToast() {
  const t = useT();
  const [show, setShow] = useState(false);

  useEffect(() => {
    let timeoutId: ReturnType<typeof setTimeout> | null = null;
    void invoke<FirstRunStateDto>("first_run_state").then((state) => {
      if (state.tray_pinned_hint_shown) return;
      timeoutId = setTimeout(() => setShow(true), 3000);
    });
    return () => {
      if (timeoutId) clearTimeout(timeoutId);
    };
  }, []);

  if (!show) return null;

  return (
    <div className="first-run-toast" role="dialog" aria-labelledby="first-run-toast-title">
      <div className="first-run-toast__title" id="first-run-toast-title">
        {t("first_run.title")}
      </div>
      <p className="first-run-toast__text">{t("first_run.body")}</p>
      <button
        type="button"
        className="first-run-toast__dismiss"
        onClick={() => {
          setShow(false);
          void invoke("first_run_mark_tray_hint_shown");
        }}
      >
        {t("first_run.button.got_it")}
      </button>
    </div>
  );
}
