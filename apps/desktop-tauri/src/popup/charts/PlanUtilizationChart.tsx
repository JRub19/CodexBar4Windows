import { useState } from "react";
import type uPlot from "uplot";
import { MOCK_PLAN_UTILIZATION } from "../mock/chartFixtures";
import { ChartCard } from "./ChartCard";

// Phase 3 D11: plan utilization with a segmented picker when multiple
// series are present. 6 px bar width per spec 15 section 11.5; synthetic
// points (interpolated when source is missing) ride at 0.45 opacity.

interface Props {
  utilization?: typeof MOCK_PLAN_UTILIZATION;
}

export function PlanUtilizationChart({
  utilization = MOCK_PLAN_UTILIZATION,
}: Props) {
  const [active, setActive] = useState(utilization.series[0]?.name ?? "");
  const series = utilization.series.find((s) => s.name === active);
  if (!series) return null;
  const data: uPlot.AlignedData = [utilization.timestamps, series.values];
  return (
    <ChartCard
      title="Plan utilization"
      detailPrimary={`Active: ${active}`}
      data={data}
      series={[
        {},
        {
          stroke: "#6E5AFF",
          fill: "#6E5AFF",
          width: 6,
          points: { show: false },
        },
      ]}
      legend={
        <div className="chart-segmented" role="tablist">
          {utilization.series.map((s) => (
            <button
              key={s.name}
              type="button"
              role="tab"
              aria-selected={s.name === active}
              className={
                s.name === active
                  ? "chart-segmented__tab chart-segmented__tab--active"
                  : "chart-segmented__tab"
              }
              onClick={() => setActive(s.name)}
            >
              {s.name}
            </button>
          ))}
        </div>
      }
    />
  );
}
