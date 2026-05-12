import type uPlot from "uplot";
import { MOCK_USAGE_BREAKDOWN } from "../mock/chartFixtures";
import { ChartCard } from "./ChartCard";

// Phase 3 D11: stacked bars per service, palette per spec 15 section 11.3.
// Legend is a min 110 px grid rendered as a child of ChartCard.

const SERVICE_PALETTE = ["#6E5AFF", "#49A3B0", "#FFA94D", "#FF6961"];

interface Props {
  breakdown?: typeof MOCK_USAGE_BREAKDOWN;
}

export function UsageBreakdownChart({ breakdown = MOCK_USAGE_BREAKDOWN }: Props) {
  const data: uPlot.AlignedData = [
    breakdown.timestamps,
    ...breakdown.bySession.map((s) => s.values),
  ];
  const series: uPlot.Series[] = [
    {},
    ...breakdown.bySession.map((s, i) => ({
      label: s.name,
      stroke: SERVICE_PALETTE[i % SERVICE_PALETTE.length],
      fill: SERVICE_PALETTE[i % SERVICE_PALETTE.length],
      width: 1,
      points: { show: false },
    })),
  ];
  const legend = (
    <ul className="chart-legend">
      {breakdown.bySession.map((s, i) => (
        <li key={s.name} className="chart-legend__item">
          <span
            className="chart-legend__swatch"
            style={{
              background: SERVICE_PALETTE[i % SERVICE_PALETTE.length],
            }}
          />
          {s.name}
        </li>
      ))}
    </ul>
  );
  return (
    <ChartCard
      title="Usage by service"
      data={data}
      series={series}
      legend={legend}
    />
  );
}
