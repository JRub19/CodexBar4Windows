import type { Metric } from "./snapshot";

// Phase 3 D5: a single metric row inside a card. Per spec 15 section 4.1
// the row is title, then bar, then two captions: percent + reset on top,
// detail left + detail right below. D6 will replace the temporary `<div
// className="metric-row__bar-fallback">` with the real UsageProgressBar.

interface Props {
  metric: Metric;
  brandAccent: string;
}

export function MetricRow({ metric, brandAccent }: Props) {
  const pct = metric.percent;
  return (
    <div className="metric-row">
      <span className="metric-row__title">{metric.title}</span>
      <div
        className="metric-row__bar-fallback"
        style={
          {
            "--metric-fill": brandAccent,
            "--metric-percent": pct == null ? "0%" : `${Math.max(0, Math.min(100, pct))}%`,
          } as React.CSSProperties
        }
      />
      <div className="metric-row__captions">
        <span className="metric-row__percent">
          {pct == null ? "—" : `${Math.round(pct)}%`}
        </span>
        {metric.resetText ? (
          <span className="metric-row__reset">{metric.resetText}</span>
        ) : null}
      </div>
      {metric.detailLeft || metric.detailRight ? (
        <div className="metric-row__details">
          {metric.detailLeft ? (
            <span className="metric-row__detail-left">{metric.detailLeft}</span>
          ) : null}
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
