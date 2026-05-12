import { useUsageStore } from "../state/usageStore";
import { SettingsCog } from "./SettingsCog";

// Phase 3 D2: top region of the popup. Switcher tabs slot in here in D3.
// The header sits directly on the Mica backdrop with no blur of its own,
// which is why it has no background-color of its own.

export function PopupHeader() {
  const descriptors = useUsageStore((s) => s.descriptors);
  const title =
    descriptors.length === 0
      ? "CodexBar"
      : descriptors.length === 1
        ? descriptors[0].metadata.display_name
        : "Overview";

  return (
    <header className="popup-header">
      <span className="popup-header__title">{title}</span>
      <SettingsCog />
    </header>
  );
}
