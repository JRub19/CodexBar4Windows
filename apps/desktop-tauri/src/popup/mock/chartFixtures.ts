// Phase 3 D11 fixtures. Phase 4 onward replaces these with real
// per-provider history fetched from the Rust core. The shape mirrors
// uPlot's data layout: a [timestamps[], series[]] tuple, with epoch
// seconds on the x axis.

export interface CostHistory {
  timestamps: number[];
  totals: number[];
  // Top 4 services per spec 15 section 11.3.
  breakdownByService: Array<{ name: string; values: number[] }>;
}

export interface CreditsHistory {
  timestamps: number[];
  values: number[];
}

export interface UsageBreakdown {
  timestamps: number[];
  bySession: Array<{ name: string; values: number[] }>;
}

export interface PlanUtilization {
  timestamps: number[];
  // Each series is one plan window; values are 0..100 utilization.
  series: Array<{ name: string; values: number[] }>;
}

function lastNDays(n: number): number[] {
  const out: number[] = [];
  const today = Math.floor(Date.now() / 1000);
  for (let i = n - 1; i >= 0; i--) out.push(today - i * 86_400);
  return out;
}

export const MOCK_COST_HISTORY: CostHistory = {
  timestamps: lastNDays(30),
  totals: Array.from({ length: 30 }, (_, i) => Math.round(Math.sin(i / 4) * 6 + 10) * 100 / 100),
  breakdownByService: [
    { name: "Sonnet", values: Array.from({ length: 30 }, () => Math.random() * 5) },
    { name: "Opus", values: Array.from({ length: 30 }, () => Math.random() * 3) },
    { name: "Haiku", values: Array.from({ length: 30 }, () => Math.random() * 2) },
    { name: "Other", values: Array.from({ length: 30 }, () => Math.random()) },
  ],
};

export const MOCK_CREDITS_HISTORY: CreditsHistory = {
  timestamps: lastNDays(30),
  values: Array.from({ length: 30 }, (_, i) => 120 + Math.round(Math.cos(i / 5) * 20)),
};

export const MOCK_USAGE_BREAKDOWN: UsageBreakdown = {
  timestamps: lastNDays(14),
  bySession: [
    { name: "Editor", values: Array.from({ length: 14 }, () => Math.random() * 40) },
    { name: "CLI", values: Array.from({ length: 14 }, () => Math.random() * 25) },
    { name: "Web", values: Array.from({ length: 14 }, () => Math.random() * 18) },
  ],
};

export const MOCK_PLAN_UTILIZATION: PlanUtilization = {
  timestamps: lastNDays(7),
  series: [
    { name: "Weekly", values: Array.from({ length: 7 }, (_, i) => Math.min(100, 30 + i * 10)) },
    { name: "Session", values: Array.from({ length: 7 }, () => 45 + Math.random() * 30) },
  ],
};
