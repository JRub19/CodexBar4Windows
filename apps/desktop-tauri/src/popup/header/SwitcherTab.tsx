import type { ProviderDescriptorDto } from "../../bindings";

// Pill-segmented tab used inside the popup switcher. Selected tab
// shows an accent underline indicator; unselected tabs use a hover
// plate. The brand color is exposed as a CSS custom property so the
// underline picks up the provider's own color rather than the
// generic system accent — that subtle cue helps users glance-orient
// when several providers are stacked.

interface Props {
  descriptor: ProviderDescriptorDto;
  selected: boolean;
  onSelect: () => void;
}

export function SwitcherTab({ descriptor, selected, onSelect }: Props) {
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
        { "--accent": descriptor.branding.accent_hex } as React.CSSProperties
      }
      onClick={onSelect}
    >
      <span className="switcher-tab__label">
        {descriptor.metadata.display_name}
      </span>
    </button>
  );
}
