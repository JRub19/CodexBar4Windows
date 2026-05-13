import { useEffect, useState } from "react";
import type { Metric } from "./snapshot";
import { computeWindowPace } from "../format/pace";

// Hero metric — the single most important number on the card.
//
//   81%               ← 36px tabular, the visual anchor
//   remaining         ← 11px caption
//
//   ━━━━━━━━━━━━━━     ← 6px bar; FILL represents REMAINING. The
//                       fill width matches the displayed number so
//                       "81% remaining" shows a bar 81% full.
//                       A vertical pace tick overlays the bar at the
//                       *expected remaining* position (i.e. where
//                       the user "should" be at this moment). When
//                       the actual remaining is greater than expected
//                       the user is "in reserve" (tick is green); when
//                       less, they are "in deficit" (tick is red).
//
//   Resets in 3h 12m  ← reset hint
//   On pace · ahead 4% (when pace data is present)
//
// Critical state thresholds bias the bar fill color:
//   - remaining ≥ 25%: accent (the default)
//   - 10-25%: warning amber
//   - <  10%: error red

interface Props {
  metric: Metric;
}

function colorClassFor(remainingPercent: number | null): string {
  if (remainingPercent == null) return "";
  if (remainingPercent < 10) return "usage-bar__fill--critical";
  if (remainingPercent < 25) return "usage-bar__fill--warning";
  return "";
}

export function HeroMetric({ metric }: Props) {
  const [isFirstPaint, setIsFirstPaint] = useState(true);

  // Bar grows from 0 to its value on first paint, then transitions
  // smoothly to new values on subsequent updates.
  useEffect(() => {
    const t = window.requestAnimationFrame(() => setIsFirstPaint(false));
    return () => window.cancelAnimationFrame(t);
  }, []);

  const usedPercent = metric.percent;
  const remainingPercent =
    usedPercent != null ? Math.max(0, 100 - usedPercent) : null;

  // Pace overlay — only when we have enough data to compute it
  // (a known window duration, a future reset time, and a used%).
  const pace =
    usedPercent != null
      ? computeWindowPace({
          usedPercent,
          label: metric.windowLabel,
          resetAtUnixSecs: metric.resetAtUnixSecs,
        })
      : null;

  return (
    <div className="hero-metric">
      <div>
        <div className="hero-metric__value">
          {remainingPercent != null ? `${Math.round(remainingPercent)}%` : "—"}
        </div>
        <div className="hero-metric__label">remaining</div>
      </div>
      <div
        className="usage-bar"
        role="progressbar"
        aria-label={`${metric.title} remaining`}
        aria-valuenow={remainingPercent ?? undefined}
        aria-valuemin={0}
        aria-valuemax={100}
      >
        <div
          className={
            "usage-bar__fill" +
            (isFirstPaint ? " usage-bar__fill--first-paint" : "") +
            " " + colorClassFor(remainingPercent)
          }
          style={
            {
              "--usage-percent": `${remainingPercent ?? 0}%`,
            } as React.CSSProperties
          }
        />
        {pace && pace.sentiment !== "neutral" ? (
          <div
            className={`usage-bar__pace-tip usage-bar__pace-tip--${pace.sentiment}`}
            style={{ left: `${pace.expectedRemainingPercent}%` }}
            aria-hidden="true"
            title={pace.text.left ?? undefined}
          />
        ) : null}
      </div>
      <div className="hero-metric__footer">
        {metric.resetText ? (
          <div className="hero-metric__reset">Resets {metric.resetText}</div>
        ) : null}
        {pace?.text.left ? (
          <div
            className={`hero-metric__pace hero-metric__pace--${pace.sentiment}`}
          >
            <span className="hero-metric__pace-dot" aria-hidden="true" />
            <span>
              {pace.text.left}
              {pace.text.right ? ` · ${pace.text.right}` : ""}
            </span>
          </div>
        ) : null}
      </div>
    </div>
  );
}
