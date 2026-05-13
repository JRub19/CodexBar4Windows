import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ActionRow } from "./ActionRow";
import { formatUpdated } from "../format/reset";
import { useUsageStore } from "../state/usageStore";
import { usePopupVisibility } from "../state/usePopupVisibility";
import { useT } from "../../i18n";

// Footer action rows — Refresh / Settings… / About / Quit — plus an
// optional "Update ready" row when the updater has staged an install.
// This mirrors the original mac app's NSMenu meta-section layout:
// stacked full-width rows, each row a hover plate with leading icon,
// label, and optional accelerator hint on the right.

// Segoe Fluent codepoints. See
// https://learn.microsoft.com/en-us/windows/apps/design/style/segoe-fluent-icons-font
const ICON_REFRESH = "";   // Refresh
const ICON_SETTINGS = "";  // Settings
const ICON_INFO = "";      // Info
const ICON_POWER = "";     // PowerButton
const ICON_UPDATE = "";    // SyncFolder / Update

interface Props {
  /** Reserved for in-popup settings panel toggle. Currently unused —
   *  Settings… opens the standalone Preferences window directly. */
  onOpenSettings?: () => void;
}

interface UpdateInfoDto {
  current_version: string;
  available_version: string | null;
  release_notes: string | null;
  release_date: string | null;
}

export function PopupFooter({ onOpenSettings: _onOpenSettings }: Props = {}) {
  const t = useT();
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

  // Probe the updater once when the popup is open. The runtime guard
  // in `updater_commands.rs` short-circuits when the placeholder
  // pubkey is still baked in, so this is always safe to call.
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
      // Installer launch errors fall back to the regular updater
      // path; the runtime guard already logs them.
    } finally {
      setInstalling(false);
    }
  };

  return (
    <footer className="popup-footer">
      {updateAvailable ? (
        <ActionRow
          icon={<span className="action-row__icon-glyph">{ICON_UPDATE}</span>}
          title={
            installing
              ? t("update.title.installing")
              : "Update ready, restart now?"
          }
          subtitle={`v${updateAvailable}`}
          onClick={() => void onInstallUpdate()}
          variant="accent"
        />
      ) : null}
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
        title={refreshError ? "Refresh failed" : "Refresh"}
        subtitle={subtitle}
        accelerator="Ctrl+R"
        onClick={() => {
          void onRefresh();
        }}
        destructive={refreshError != null}
      />
      <ActionRow
        icon={<span className="action-row__icon-glyph">{ICON_SETTINGS}</span>}
        title="Settings…"
        accelerator="Ctrl+,"
        onClick={() => {
          void invoke("open_preferences");
        }}
      />
      <ActionRow
        icon={<span className="action-row__icon-glyph">{ICON_INFO}</span>}
        title="About CodexBar"
        onClick={() => {
          void invoke("open_preferences", { providerId: null });
          // Future: route to the About pane specifically.
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
