import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

// Phase 9 §A-2: hook that exposes the popup's current visibility.
// The backend emits `popup:visibility` events on focus gain/loss; this
// hook subscribes and exposes a boolean for React components that
// want to pause their work (animations, polling, network calls) when
// the popup is hidden.
//
// Full WebView2 process suspension (the ideal end state) requires
// `ICoreWebView2Controller3::TrySuspend`, which Tauri 2 doesn't yet
// expose. Until then, pausing JS work is the cheapest available win:
// every component that subscribes can return early from its render
// or skip its setInterval tick when hidden.

interface VisibilityPayload {
  visible: boolean;
}

export function usePopupVisibility(): boolean {
  // Default: visible. The popup is initially shown when the user
  // clicks the tray; the first hide event flips this.
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    const unlisten = listen<VisibilityPayload>(
      "popup:visibility",
      (event) => {
        setVisible(event.payload.visible);
      },
    );
    return () => {
      void unlisten.then((f) => f());
    };
  }, []);

  return visible;
}
