import type { ProviderDescriptorDto } from "../../bindings";
import { WeeklyIndicator } from "./WeeklyIndicator";

interface Props {
  descriptor: ProviderDescriptorDto;
  selected: boolean;
  // 0..100 remaining in the current weekly window.
  weeklyRemainingPercent: number;
  onSelect: () => void;
}

// Phase 3 D3: a single tab. Selected paints the brand color through the
// CSS custom property `--tab-accent`, so the unselected hover state can
// fade in a 12% wash of the same hue without re-rendering inline style.

export function SwitcherTab({
  descriptor,
  selected,
  weeklyRemainingPercent,
  onSelect,
}: Props) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={selected}
      tabIndex={selected ? 0 : -1}
      className={
        selected ? "switcher-tab switcher-tab--selected" : "switcher-tab"
      }
      style={
        { "--tab-accent": descriptor.branding.accent_hex } as React.CSSProperties
      }
      onClick={onSelect}
    >
      <span className="switcher-tab__label">
        {descriptor.metadata.display_name}
      </span>
      <WeeklyIndicator
        brandColor={descriptor.branding.accent_hex}
        remainingPercent={weeklyRemainingPercent}
        hidden={selected}
      />
    </button>
  );
}
