import { useEffect, useState } from "react";
import type { Metric } from "./snapshot";

// Hero metric — the single most important number on the card.
//
//   62%               ← 36px tabular, the visual anchor
//   remaining         ← 11px caption
//
//   ━━━━━━━━━━━━━━     ← 6px bar
//
//   Resets in 3h 12m  ← compact reset text
//   On pace · ahead 4% (when pace data is present)
//
// Critical state thresholds bias the bar fill color:
//   - >= 25%: accent (the default)
//   - 10-25%: warning amber
//   - <  10%: error red

interface Props {
  metric: Metric;
  /** Pace text for "ahead/behind/on pace" hint. */
  pace?: {
    text: string;
    sentiment: "ahead" | "behind" | "neutral";
  } | null;
}

function colorClassFor(remainingPercent: number | null): string {
  if (remainingPercent == null) return "";
  if (remainingPercent < 10) return "usage-bar__fill--critical";
  if (remainingPercent < 25) return "usage-bar__fill--warning";
  return "";
}

export function HeroMetric({ metric, pace }: Props) {
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

  // The hero number shows REMAINING (what users care about). When the
  // setting `usage_bars_show_used` is on in future, this can flip.
  const displayPercent = remainingPercent;

  return (
    <div className="hero-metric">
      <div>
        <div className="hero-metric__value">
          {displayPercent != null ? `${Math.round(displayPercent)}%` : "—"}
        </div>
        <div className="hero-metric__label">remaining</div>
      </div>
      <div
        className="usage-bar"
        role="progressbar"
        aria-label={`${metric.title} usage`}
        aria-valuenow={usedPercent ?? undefined}
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
              "--usage-percent": `${usedPercent ?? 0}%`,
            } as React.CSSProperties
          }
        />
      </div>
      <div className="hero-metric__footer">
        {metric.resetText ? (
          <div className="hero-metric__reset">Resets {metric.resetText}</div>
        ) : null}
        {pace ? (
          <div className={`hero-metric__pace hero-metric__pace--${pace.sentiment}`}>
            <span className="hero-metric__pace-dot" aria-hidden="true" />
            <span>{pace.text}</span>
          </div>
        ) : null}
      </div>
    </div>
  );
}
