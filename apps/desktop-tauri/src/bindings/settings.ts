// Hand authored bindings mirroring `rust/src/settings/model.rs`.
//
// Phase 1 keeps these manual to avoid the ts-rs build pipeline. Phase 8 of
// the plan automates the generation. When you edit `model.rs`, mirror the
// change here in the same commit.

export type RefreshFrequency =
  | "manual"
  | "one_minute"
  | "two_minutes"
  | "five_minutes"
  | "fifteen_minutes"
  | "thirty_minutes";

export interface ProviderToggle {
  id: string;
  enabled: boolean;
  order: number;
}

export interface DisplayPreferences {
  merge_icons: boolean;
  usage_bars_show_used: boolean;
  hide_quota_warning_markers: boolean;
}

export interface DebugFlags {
  debug_menu_enabled: boolean;
  verbose_logging: boolean;
  disable_secret_storage: boolean;
}

export interface Settings {
  schema_version: number;
  refresh_frequency: RefreshFrequency;
  pause_refresh: boolean;
  providers: ProviderToggle[];
  display: DisplayPreferences;
  debug: DebugFlags;
  allow_browser_cookie_import: boolean;
  app_language: string | null;
  provider_kv: Record<string, string>;
  notifications_enabled: boolean;
  popup_toggle_hotkey: string | null;
  telemetry_enabled: boolean;
}

export interface SettingsPatch {
  refresh_frequency?: RefreshFrequency;
  pause_refresh?: boolean;
  providers?: ProviderToggle[];
  display?: DisplayPreferences;
  debug?: DebugFlags;
  allow_browser_cookie_import?: boolean;
  app_language?: string | null;
  provider_kv?: Record<string, string>;
  notifications_enabled?: boolean;
  popup_toggle_hotkey?: string | null;
  telemetry_enabled?: boolean;
}

export interface SettingsChangedPayload {
  settings: Settings;
}
