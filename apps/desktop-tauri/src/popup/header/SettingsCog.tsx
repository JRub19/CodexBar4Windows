// 16x16 Segoe Fluent Settings glyph. The click opens the inline provider
// settings panel.

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
