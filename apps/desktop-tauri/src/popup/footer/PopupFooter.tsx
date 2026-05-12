import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ActionRow } from "./ActionRow";
import { formatUpdated } from "../format/reset";
import { useUsageStore } from "../state/usageStore";

// Phase 3 D10: footer with Refresh, Preferences, and Quit rows. The
// refresh icon spins via the `action-row__icon--spinning` class while a
// refresh is in flight. Last refresh caption ticks every 30 s through
// the shared `formatUpdated` helper.

// Segoe Fluent codepoints, see https://learn.microsoft.com/en-us/windows/apps/design/style/segoe-fluent-icons-font
const ICON_REFRESH = "";
const ICON_SETTINGS = "";
const ICON_POWER = "";

export function PopupFooter() {
  const lastUsageEvent = useUsageStore((s) => s.lastUsageEvent);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [lastRefreshAt, setLastRefreshAt] = useState<Date | null>(null);
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    if (lastUsageEvent != null) {
      setLastRefreshAt(new Date());
      setRefreshError(null);
    }
  }, [lastUsageEvent]);

  useEffect(() => {
    const id = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(id);
  }, []);

  const subtitle = refreshError
    ? refreshError
    : lastRefreshAt
      ? formatUpdated(
          Math.floor((now.getTime() - lastRefreshAt.getTime()) / 1000),
          now,
          lastRefreshAt,
        )
      : "Awaiting first refresh";

  const onRefresh = async () => {
    setRefreshing(true);
    setRefreshError(null);
    try {
      await invoke("refresh_now");
    } catch (e) {
      setRefreshError(e instanceof Error ? e.message : "Refresh failed");
    } finally {
      setRefreshing(false);
    }
  };

  return (
    <footer className="popup-footer">
      <ActionRow
        icon={
          <span
            className={
              refreshing
                ? "action-row__icon-glyph action-row__icon-glyph--spinning"
                : "action-row__icon-glyph"
            }
          >
            {ICON_REFRESH}
          </span>
        }
        title={refreshError ? "Refresh failed" : "Refresh now"}
        subtitle={subtitle}
        onClick={() => {
          void onRefresh();
        }}
        destructive={refreshError != null}
      />
      <ActionRow
        icon={<span className="action-row__icon-glyph">{ICON_SETTINGS}</span>}
        title="Preferences"
        accelerator="Ctrl+,"
        onClick={() => {
          void invoke("open_preferences");
        }}
      />
      <ActionRow
        icon={<span className="action-row__icon-glyph">{ICON_POWER}</span>}
        title="Quit"
        accelerator="Ctrl+Q"
        onClick={() => {
          void invoke("quit_app");
        }}
      />
    </footer>
  );
}
