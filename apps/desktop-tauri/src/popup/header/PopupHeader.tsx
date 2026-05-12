import { useUsageStore } from "../state/usageStore";
import { ProviderSwitcherButtons } from "./ProviderSwitcherButtons";
import { SettingsCog } from "./SettingsCog";

// Phase 3 D2: top region of the popup. Sits directly on the Mica
// backdrop with no blur of its own. Phase 3 D3 adds the switcher
// tab row underneath the title when more than one provider is
// configured.

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
      <div className="popup-header__row">
        <span className="popup-header__title">{title}</span>
        <SettingsCog />
      </div>
      <ProviderSwitcherButtons />
    </header>
  );
}
