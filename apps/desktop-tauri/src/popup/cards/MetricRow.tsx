import { useEffect, useState } from "react";
import type { Metric } from "./snapshot";

// Compact secondary metric row. Used for weekly / credits / cost
// after the hero metric block. Two-line layout:
//
//   WEEKLY                          ← uppercase tertiary caption
//   48% remaining                   ← title + percent inline
//   ━━━━━━━━━━━━━━                  ← compact bar (4 px)
//   Resets Fri 12 PM     12.4M/20M  ← reset / detail row
//
// First-paint animation grows the bar from 0 → value; subsequent
// updates transition the width smoothly.

interface Props {
  metric: Metric;
}

function colorClassFor(remainingPercent: number | null): string {
  if (remainingPercent == null) return "";
  if (remainingPercent < 10) return "usage-bar__fill--critical";
  if (remainingPercent < 25) return "usage-bar__fill--warning";
  return "";
}

export function MetricRow({ metric }: Props) {
  const [isFirstPaint, setIsFirstPaint] = useState(true);
  useEffect(() => {
    const t = window.requestAnimationFrame(() => setIsFirstPaint(false));
    return () => window.cancelAnimationFrame(t);
  }, []);

  const usedPercent = metric.percent;
  const remainingPercent =
    usedPercent != null ? Math.max(0, 100 - usedPercent) : null;

  return (
    <div className="metric-row">
      <div className="metric-row__caption">{metric.title}</div>
      <div className="metric-row__head">
        <span className="metric-row__title">
          {remainingPercent != null
            ? `${Math.round(remainingPercent)}% remaining`
            : metric.detailLeft ?? "—"}
        </span>
      </div>
      {usedPercent != null ? (
        <div
          className="usage-bar usage-bar--compact"
          role="progressbar"
          aria-label={`${metric.title} usage`}
          aria-valuenow={usedPercent}
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
                "--usage-percent": `${usedPercent}%`,
              } as React.CSSProperties
            }
          />
        </div>
      ) : null}
      {metric.resetText || metric.detailRight ? (
        <div className="metric-row__details">
          {metric.resetText ? (
            <span className="metric-row__detail-left">
              Resets {metric.resetText}
            </span>
          ) : (
            <span />
          )}
          {metric.detailRight ? (
            <span className="metric-row__detail-right">
              {metric.detailRight}
            </span>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
