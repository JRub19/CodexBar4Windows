import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

// Phase 3 D11: common skeleton for cost, credits, breakdown, and plan
// utilization charts. 130 px canvas height per spec 15 section 11.6,
// two optional detail lines above the canvas, optional legend below.
// Charts are mounted with stripped-down uPlot options; the parent
// passes ready-to-render `series` and `data`.

interface Props {
  title: string;
  detailPrimary?: string | null;
  detailSecondary?: string | null;
  footer?: string | null;
  data: uPlot.AlignedData;
  series: uPlot.Series[];
  axes?: uPlot.Axis[];
  scales?: uPlot.Scales;
  height?: number;
  legend?: React.ReactNode;
}

export function ChartCard({
  title,
  detailPrimary,
  detailSecondary,
  footer,
  data,
  series,
  axes,
  scales,
  height = 130,
  legend,
}: Props) {
  const ref = useRef<HTMLDivElement | null>(null);
  const plotRef = useRef<uPlot | null>(null);

  useEffect(() => {
    if (!ref.current) return;
    const opts: uPlot.Options = {
      width: ref.current.clientWidth,
      height,
      legend: { show: false },
      cursor: { drag: { x: false, y: false } },
      series,
      axes: axes ?? [
        { stroke: "rgba(255,255,255,0.32)" },
        { stroke: "rgba(255,255,255,0.32)" },
      ],
      scales,
    };
    plotRef.current = new uPlot(opts, data, ref.current);
    const onResize = () => {
      if (plotRef.current && ref.current) {
        plotRef.current.setSize({ width: ref.current.clientWidth, height });
      }
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      plotRef.current?.destroy();
      plotRef.current = null;
    };
  }, [data, series, axes, scales, height]);

  return (
    <section className="chart-card">
      <header className="chart-card__header">
        <span className="chart-card__title">{title}</span>
        {detailPrimary ? (
          <span className="chart-card__detail">{detailPrimary}</span>
        ) : null}
        {detailSecondary ? (
          <span className="chart-card__detail chart-card__detail--secondary">
            {detailSecondary}
          </span>
        ) : null}
      </header>
      <div className="chart-card__canvas" ref={ref} />
      {legend ? <div className="chart-card__legend">{legend}</div> : null}
      {footer ? <footer className="chart-card__footer">{footer}</footer> : null}
    </section>
  );
}
