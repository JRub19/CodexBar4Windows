import type uPlot from "uplot";
import { MOCK_COST_HISTORY } from "../mock/chartFixtures";
import { ChartCard } from "./ChartCard";

// Phase 3 D11: 30 day total spend chart with optional service breakdown
// rendered as stacked bars on top of the brand-color total bar. Phase 4
// hands in real history via props.

interface Props {
  brandAccent: string;
  history?: typeof MOCK_COST_HISTORY;
}

export function CostHistoryChart({ brandAccent, history = MOCK_COST_HISTORY }: Props) {
  const data: uPlot.AlignedData = [history.timestamps, history.totals];
  const peak = Math.max(...history.totals);
  const peakThreshold = peak * 0.95;
  const series: uPlot.Series[] = [
    {},
    {
      stroke: brandAccent,
      fill: (_u, idx) => {
        // Peak overlay: light yellow on the top 5 percent of values.
        const value = history.totals[idx as unknown as number];
        return value >= peakThreshold ? "#FFD60A" : brandAccent;
      },
      width: 1,
      paths: () => null,
      points: { show: false },
    },
  ];
  return (
    <ChartCard
      title="Cost (30d)"
      detailPrimary={`Total $${history.totals.reduce((a, b) => a + b, 0).toFixed(2)}`}
      detailSecondary={`Peak $${peak.toFixed(2)}`}
      data={data}
      series={series}
    />
  );
}
