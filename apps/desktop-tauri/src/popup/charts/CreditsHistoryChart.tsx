import type uPlot from "uplot";
import { MOCK_CREDITS_HISTORY } from "../mock/chartFixtures";
import { ChartCard } from "./ChartCard";

// Phase 3 D11: 30 day credit balance chart, single bar color #49A3B0
// per spec 15 section 11.2.

interface Props {
  history?: typeof MOCK_CREDITS_HISTORY;
}

export function CreditsHistoryChart({ history = MOCK_CREDITS_HISTORY }: Props) {
  const data: uPlot.AlignedData = [history.timestamps, history.values];
  const total = history.values.reduce((a, b) => a + b, 0);
  const series: uPlot.Series[] = [
    {},
    {
      stroke: "#49A3B0",
      fill: "#49A3B0",
      width: 1,
      points: { show: false },
    },
  ];
  return (
    <ChartCard
      title="Credits (30d)"
      detailPrimary={`Total ${total} credits`}
      data={data}
      series={series}
      footer={`Total (30d): ${total} credits`}
    />
  );
}
