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
}

export interface Settings {
  schema_version: number;
  refresh_frequency: RefreshFrequency;
  pause_refresh: boolean;
  providers: ProviderToggle[];
  display: DisplayPreferences;
  debug: DebugFlags;
  app_language: string | null;
}

export interface SettingsPatch {
  refresh_frequency?: RefreshFrequency;
  pause_refresh?: boolean;
  providers?: ProviderToggle[];
  display?: DisplayPreferences;
  debug?: DebugFlags;
  app_language?: string | null;
}

export interface SettingsChangedPayload {
  settings: Settings;
}
