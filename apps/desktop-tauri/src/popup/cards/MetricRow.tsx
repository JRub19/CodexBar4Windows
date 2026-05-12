import type { Metric } from "./snapshot";
import { UsageProgressBar } from "../components/UsageProgressBar";

// Phase 3 D5: a single metric row inside a card. Per spec 15 section 4.1
// the row is title, then bar, then two captions: percent + reset on top,
// detail left + detail right below. The real `UsageProgressBar` replaces
// the temporary fallback in phase 3 D6.

interface Props {
  metric: Metric;
  brandAccent: string;
}

export function MetricRow({ metric, brandAccent }: Props) {
  const pct = metric.percent;
  return (
    <div className="metric-row">
      <span className="metric-row__title">{metric.title}</span>
      <UsageProgressBar percent={pct ?? 0} brandColor={brandAccent} />
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
