import { useUsageStore } from "../state/usageStore";
import { useSettings } from "../../hooks/useSettings";
import { useFirstPaint } from "../hooks/useFirstPaint";

// Phase 3 D6: 6 px tall progress bar with optional quota warning markers,
// pace tip stripe, and a 200 ms ease-out tween on data update. Per
// spec 15 section 5.1 the bar lives inside MetricRow but is independently
// useful so we expose it as its own component.

interface Props {
  // 0..100 fill percent.
  percent: number;
  // 0..100, the "ideal" pace location. When provided the pace tip
  // overlays the bar at this position.
  pacePercent?: number | null;
  // Tinted card highlight inverts the bar colors per spec 15 section 5.1.
  highlighted?: boolean;
  brandColor: string;
}

const MARKER_AT_50_REMAINING = 50;
const MARKER_AT_20_REMAINING = 80;

export function UsageProgressBar({
  percent,
  pacePercent,
  highlighted = false,
  brandColor,
}: Props) {
  const settings = useSettings();
  // Phase 3 D6 toggle: settings already exposes
  // display.hide_quota_warning_markers via the SettingsHandle.
  const hideMarkers =
    settings?.display?.hide_quota_warning_markers ?? false;
  const firstPaint = useFirstPaint();
  const filled = Math.max(0, Math.min(100, percent));
  const usageStoreEvent = useUsageStore((s) => s.lastUsageEvent);
  // Reading the usage event subscribes us to re-render on update; we
  // still rely on parent props for the actual value.
  void usageStoreEvent;

  return (
    <div
      className={
        highlighted ? "usage-bar usage-bar--highlighted" : "usage-bar"
      }
      style={
        {
          "--usage-fill": brandColor,
          "--usage-percent": `${filled}%`,
          "--usage-transition": firstPaint ? "0ms" : "200ms",
        } as React.CSSProperties
      }
    >
      <div className="usage-bar__track" />
      <div className="usage-bar__fill" />
      {!hideMarkers ? (
        <>
          <div
            className="usage-bar__marker"
            style={{ left: `${MARKER_AT_50_REMAINING}%` }}
            aria-hidden="true"
          />
          <div
            className="usage-bar__marker"
            style={{ left: `${MARKER_AT_20_REMAINING}%` }}
            aria-hidden="true"
          />
        </>
      ) : null}
      {pacePercent != null ? (
        <div
          className="usage-bar__pace-tip"
          style={{ left: `${Math.max(0, Math.min(100, pacePercent))}%` }}
          aria-hidden="true"
        />
      ) : null}
    </div>
  );
}
