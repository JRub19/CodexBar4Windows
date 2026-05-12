// Phase 3 D9: tiny floating chip that confirms a copy. Renders only
// while `visible` is true; the parent toggles it via state. CSS handles
// the animation per spec 15: 120 ms opacity fade plus a 4 px slide up,
// holds 1000 ms, fades 200 ms.

interface Props {
  visible: boolean;
  label?: string;
}

export function CopiedChip({ visible, label = "Copied" }: Props) {
  if (!visible) return null;
  return (
    <span className="copied-chip" role="status" aria-live="polite">
      {label}
    </span>
  );
}
