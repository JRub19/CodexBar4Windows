import { useMemo, useState } from "react";
import type { DailyCostEntry, ProviderCostSnapshot } from "./useCostHistory";

// 30-day spend bar chart ported from the macOS `CostHistoryChartMenuView`.
//
// Layout (top → bottom):
//   "Last 30 days · $4.21 total"                   (caption, secondary)
//   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━     30 bars at ~10px each
//   May 14  ·····················  Today          axis labels (first/last)
//   Hover a bar for details OR
//   May 13: $0.42 · 18.4K tokens                  detail caption
//   ┃ claude-sonnet-4-5   $0.32 · 12.1K           model breakdown rows
//   ┃ claude-haiku-4-5    $0.10 · 6.3K
//
// Visual rules (ported from the macOS source):
//   - bar color = provider brand accent
//   - peak day gets a 5%-height yellow cap on top
//   - hovered bar opacity 1.0, others fade to 0.4
//   - detail block reserves max-height (4 rows) so layout doesn't jump
//   - "On track" / "in deficit" — owned by the wider pace machinery,
//     not this chart. We only show the spend.

interface Props {
  snapshot: ProviderCostSnapshot | null;
  /** Brand accent hex used for bars + per-model row accents. */
  brandColor: string;
  /** Tells the chart it is currently visible — drives stagger animation. */
  visible: boolean;
}

const CHART_HEIGHT = 84;
const BAR_GAP = 2;
const PEAK_CAP_FRACTION = 0.05;
const MAX_BREAKDOWN_ROWS = 4;
const ROW_OPACITY_LADDER = [0.85, 0.72, 0.58, 0.44];

function formatUsd(n: number): string {
  if (n === 0) return "$0";
  if (n < 0.01) return "<$0.01";
  if (n < 1) return `$${n.toFixed(2)}`;
  if (n < 10) return `$${n.toFixed(2)}`;
  if (n < 1000) return `$${n.toFixed(2)}`;
  return `$${n.toFixed(0)}`;
}

function formatTokens(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(1)}K`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

function formatDateShort(ymd: string): string {
  const [y, m, d] = ymd.split("-").map(Number);
  if (!y || !m || !d) return ymd;
  const months = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
  return `${months[m - 1]} ${d}`;
}

function prettifyModelId(raw: string): string {
  // claude-sonnet-4-5 → Sonnet 4.5
  // gpt-5.1-codex → GPT-5.1 Codex
  // Falls back to the raw id when no transform fires.
  let s = raw.toLowerCase();
  s = s.replace(/^claude-/, "");
  s = s.replace(/^gpt-/, "GPT-");
  s = s.replace(/-(\d{8})$/, "");
  s = s.replace(/^(sonnet|opus|haiku)-(\d)-(\d)/, (_, fam, a, b) => {
    const cap = fam.charAt(0).toUpperCase() + fam.slice(1);
    return `${cap} ${a}.${b}`;
  });
  s = s.replace(/-codex\b/, " Codex");
  s = s.replace(/-mini\b/, " Mini");
  s = s.replace(/-nano\b/, " Nano");
  s = s.replace(/-pro\b/, " Pro");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export function CostHistoryChart({ snapshot, brandColor, visible }: Props) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  const days: DailyCostEntry[] = snapshot?.daily ?? [];
  const totals = snapshot?.last_30_days_usd ?? [];
  const total = snapshot?.total_window_usd ?? 0;

  const peakValue = useMemo(
    () => totals.reduce((m, v) => (v > m ? v : m), 0),
    [totals],
  );

  // Bars are only meaningful when there's data — show an empty state
  // instead of a row of 30 grey bars at 0 height.
  const hasData = total > 0;

  if (!hasData) {
    return (
      <div className="cost-chart">
        <div className="cost-chart__caption">Last 30 days</div>
        <div className="cost-chart__empty">
          No cost data yet. Start a session in Claude Code or Codex and your
          spend will appear here.
        </div>
      </div>
    );
  }

  const peakIndex = totals.findIndex((v) => v === peakValue);

  return (
    <div className="cost-chart">
      <div className="cost-chart__caption">
        <span className="cost-chart__caption-label">Last 30 days</span>
        <span className="cost-chart__caption-value">{formatUsd(total)}</span>
      </div>
      <div
        className="cost-chart__bars"
        style={{ height: CHART_HEIGHT }}
        onMouseLeave={() => setHoveredIndex(null)}
      >
        {totals.map((cost, i) => {
          const heightFraction = peakValue > 0 ? cost / peakValue : 0;
          const barHeight = Math.max(
            cost > 0 ? 2 : 0,
            Math.round(heightFraction * (CHART_HEIGHT - 2)),
          );
          const isPeak = i === peakIndex && peakValue > 0;
          const isHovered = hoveredIndex === i;
          const isAnyHovered = hoveredIndex != null;
          const opacity = !isAnyHovered ? 1 : isHovered ? 1 : 0.35;
          const capHeight = isPeak
            ? Math.max(1, Math.round(barHeight * PEAK_CAP_FRACTION))
            : 0;
          return (
            <button
              key={i}
              type="button"
              className={
                "cost-chart__bar" +
                (visible ? " cost-chart__bar--enter" : "") +
                (isHovered ? " cost-chart__bar--hovered" : "")
              }
              style={
                {
                  "--cost-stagger": `${i * 14}ms`,
                  marginRight: i === totals.length - 1 ? 0 : BAR_GAP,
                  opacity,
                } as React.CSSProperties
              }
              onMouseEnter={() => setHoveredIndex(i)}
              onFocus={() => setHoveredIndex(i)}
              onBlur={() => setHoveredIndex(null)}
              aria-label={`${formatDateShort(days[i]?.date ?? "")}: ${formatUsd(cost)}, ${formatTokens(days[i]?.total_tokens ?? 0)} tokens`}
            >
              <div
                className="cost-chart__bar-fill"
                style={{
                  height: barHeight,
                  background: brandColor,
                }}
              />
              {capHeight > 0 ? (
                <div
                  className="cost-chart__bar-cap"
                  style={{ height: capHeight, bottom: barHeight - capHeight }}
                />
              ) : null}
            </button>
          );
        })}
      </div>
      <div className="cost-chart__axis">
        <span>{days[0]?.date ? formatDateShort(days[0].date) : ""}</span>
        <span>
          {days[days.length - 1]?.date
            ? formatDateShort(days[days.length - 1].date)
            : ""}
        </span>
      </div>
      <CostHistoryDetail
        hoverEntry={hoveredIndex != null ? days[hoveredIndex] ?? null : null}
        brandColor={brandColor}
      />
    </div>
  );
}

function CostHistoryDetail({
  hoverEntry,
  brandColor,
}: {
  hoverEntry: DailyCostEntry | null;
  brandColor: string;
}) {
  // Reserve max-height regardless of how many breakdown rows we
  // have — this mirrors the macOS layout-stability trick.
  const filler = Array.from(
    {
      length: Math.max(
        0,
        MAX_BREAKDOWN_ROWS - (hoverEntry?.models.slice(0, MAX_BREAKDOWN_ROWS).length ?? 0),
      ),
    },
    (_, i) => i,
  );

  return (
    <div className="cost-chart__detail">
      <div className="cost-chart__detail-primary">
        {hoverEntry == null
          ? "Hover a bar for details"
          : `${formatDateShort(hoverEntry.date)}: ${formatUsd(hoverEntry.cost_usd)} · ${formatTokens(hoverEntry.total_tokens)} tokens`}
      </div>
      <div className="cost-chart__detail-rows">
        {hoverEntry?.models
          .slice(0, MAX_BREAKDOWN_ROWS)
          .map((m, i) => (
            <div className="cost-chart__detail-row" key={`${m.model_id}-${i}`}>
              <span
                className="cost-chart__detail-accent"
                style={{
                  background: brandColor,
                  opacity: ROW_OPACITY_LADDER[i] ?? 0.3,
                }}
              />
              <span className="cost-chart__detail-name">
                {prettifyModelId(m.model_id)}
              </span>
              <span className="cost-chart__detail-cost">
                {formatUsd(m.cost_usd)} · {formatTokens(m.total_tokens)}
              </span>
            </div>
          ))}
        {filler.map((i) => (
          <div className="cost-chart__detail-row cost-chart__detail-row--filler" key={`f-${i}`} />
        ))}
      </div>
    </div>
  );
}
