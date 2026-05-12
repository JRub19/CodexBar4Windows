import { useUsageStore } from "../state/usageStore";
import { ProviderSwitcherButtons } from "./ProviderSwitcherButtons";
import { SettingsCog } from "./SettingsCog";

// Phase 3 D2: top region of the popup. Sits directly on the Mica
// backdrop with no blur of its own. Phase 4 P4-19 routes the cog click
// through a parent-supplied callback so the popup can swap the body
// for the settings pane without route changes.

interface Props {
  onOpenSettings: () => void;
}

export function PopupHeader({ onOpenSettings }: Props) {
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
        <SettingsCog onClick={onOpenSettings} />
      </div>
      <ProviderSwitcherButtons />
    </header>
  );
}
