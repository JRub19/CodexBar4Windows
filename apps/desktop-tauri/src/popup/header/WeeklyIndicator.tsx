// Phase 3 D3: 4 px tall bar that hugs the bottom of each unselected
// switcher tab. The width represents `remaining / 100` of the available
// tab width, drawn in the provider's brand color. When the tab is
// selected we hide the indicator (the selected fill conveys the same
// info).

interface Props {
  brandColor: string;
  remainingPercent: number; // 0..100
  hidden: boolean;
}

export function WeeklyIndicator({ brandColor, remainingPercent, hidden }: Props) {
  if (hidden) return null;
  const clamped = Math.max(0, Math.min(100, remainingPercent));
  return (
    <div
      className="weekly-indicator"
      style={{
        width: `calc(${clamped}% - 12px)`,
        background: brandColor,
      }}
      aria-hidden="true"
    />
  );
}
