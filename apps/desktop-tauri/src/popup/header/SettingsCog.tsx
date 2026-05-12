import { invoke } from "@tauri-apps/api/core";

// Phase 3 D2: 16x16 Segoe Fluent Settings glyph. The Fluent UI codepoint
// for the cog is U+E713 in the Windows Symbol font. Click invokes the
// Tauri `open_preferences` command which is currently a stub; Phase 8
// replaces the stub with the real preferences window.

export function SettingsCog() {
  return (
    <button
      type="button"
      className="settings-cog"
      aria-label="Preferences"
      title="Preferences"
      onClick={() => {
        void invoke("open_preferences");
      }}
    >
      <span aria-hidden="true">&#xE713;</span>
    </button>
  );
}
