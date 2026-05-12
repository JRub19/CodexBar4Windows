// Phase 3 D2: 16x16 Segoe Fluent Settings glyph. Phase 4 P4-19 wires
// the click to open the inline `ProviderSettingsPanel` instead of the
// stubbed Preferences window. Phase 8 promotes the panel to a separate
// Tauri window.

interface Props {
  onClick: () => void;
}

export function SettingsCog({ onClick }: Props) {
  return (
    <button
      type="button"
      className="settings-cog"
      aria-label="Preferences"
      title="Preferences"
      onClick={onClick}
    >
      <span aria-hidden="true">&#xE713;</span>
    </button>
  );
}
