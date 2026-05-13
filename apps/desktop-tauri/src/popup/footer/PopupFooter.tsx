import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ActionRow } from "./ActionRow";
import { formatUpdated } from "../format/reset";
import { useUsageStore } from "../state/usageStore";
import { usePopupVisibility } from "../state/usePopupVisibility";
import { Icon } from "../../components/Icon";

// Footer = stacked action rows in the style of a native menu's meta
// section. Each row is 36 px tall, generous gutter, hover plate that
// fades in, press scales to 0.98.
//
// Row order matches the original macOS source's NSMenu layout:
//   Update ready (conditional, accent) → Refresh → Settings… →
//   About CodexBar → Quit.

interface Props {
  onOpenAbout?: () => void;
}

interface UpdateInfoDto {
  current_version: string;
  available_version: string | null;
  release_notes: string | null;
  release_date: string | null;
}

export function PopupFooter({ onOpenAbout: _onOpenAbout }: Props = {}) {
  const lastUsageEvent = useUsageStore((s) => s.lastUsageEvent);
  const visible = usePopupVisibility();
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [lastRefreshAt, setLastRefreshAt] = useState<Date | null>(null);
  const [now, setNow] = useState(() => new Date());
  const [updateAvailable, setUpdateAvailable] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);

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

  // Only probe the updater while the popup is open — saves the
  // round-trip when the user isn't looking.
  useEffect(() => {
    if (!visible) return;
    void invoke<UpdateInfoDto>("check_for_update")
      .then((info) => setUpdateAvailable(info.available_version))
      .catch(() => setUpdateAvailable(null));
  }, [visible]);

  const subtitle = refreshError
    ? refreshError
    : lastRefreshAt
      ? formatUpdated(
          Math.floor((now.getTime() - lastRefreshAt.getTime()) / 1000),
          now,
          lastRefreshAt,
        )
      : null;

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

  const onInstallUpdate = async () => {
    setInstalling(true);
    try {
      await invoke("install_update");
    } catch {
      /* runtime guard handles logging */
    } finally {
      setInstalling(false);
    }
  };

  return (
    <footer className="popup-footer">
      {updateAvailable ? (
        <ActionRow
          icon={<Icon name="download" />}
          title={installing ? "Installing update…" : "Update ready"}
          subtitle={`v${updateAvailable}`}
          onClick={() => void onInstallUpdate()}
          variant="accent"
        />
      ) : null}
      <ActionRow
        icon={
          <span
            className={refreshing ? "action-row__icon-glyph--spinning" : ""}
            style={{ display: "inline-flex" }}
          >
            <Icon name="refresh" />
          </span>
        }
        title={refreshError ? "Refresh failed" : "Refresh"}
        subtitle={subtitle}
        accelerator="Ctrl+R"
        onClick={() => void onRefresh()}
        destructive={refreshError != null}
      />
      <ActionRow
        icon={<Icon name="settings" />}
        title="Settings…"
        accelerator="Ctrl+,"
        onClick={() => void invoke("open_preferences")}
      />
      <ActionRow
        icon={<Icon name="info" />}
        title="About CodexBar"
        onClick={() => void invoke("open_preferences")}
      />
      <ActionRow
        icon={<Icon name="power" />}
        title="Quit"
        accelerator="Ctrl+Q"
        onClick={() => void invoke("quit_app")}
      />
    </footer>
  );
}
