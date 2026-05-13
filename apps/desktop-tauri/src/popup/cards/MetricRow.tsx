import { useEffect, useState } from "react";
import type { Metric } from "./snapshot";
import { computeWindowPace } from "../format/pace";

// Compact secondary metric row. Used for weekly / credits / cost
// after the hero metric block. Two-line layout:
//
//   WEEKLY                          ← uppercase tertiary caption
//   55% remaining                   ← title + percent inline
//   ━━━━━━━━━━━━━━ │                ← compact bar (4 px) with pace tick
//   Resets Fri 12 PM     12.4M/20M  ← reset / detail row
//
// Like HeroMetric, the fill represents REMAINING so the bar's
// visual fullness matches the "X% remaining" caption.

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
  const pace =
    usedPercent != null
      ? computeWindowPace({
          usedPercent,
          label: metric.windowLabel,
          resetAtUnixSecs: metric.resetAtUnixSecs,
        })
      : null;

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
      {pace?.text.left ? (
        <div className={`metric-row__pace metric-row__pace--${pace.sentiment}`}>
          {pace.text.left}
          {pace.text.right ? ` · ${pace.text.right}` : ""}
        </div>
      ) : null}
    </div>
  );
}
