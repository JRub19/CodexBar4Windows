import { useEffect, useState } from "react";
import { formatAbsolute, formatCountdown } from "../format/reset";

// Phase 3 D8: rotates between countdown and absolute styles every 60 s
// so the label stays fresh without burning render cycles. Per spec 15
// section 7.3 the user toggles the global preference; we honor it here
// by switching on the `style` prop.

interface Props {
  resetAt: Date;
  style: "countdown" | "absolute";
}

function tickInterval(resetAt: Date) {
  const now = Date.now();
  const remaining = resetAt.getTime() - now;
  // Tick every minute when far away, every 5 s when under a minute so
  // the seconds round up smoothly.
  return remaining > 90_000 ? 30_000 : 5_000;
}

export function ResetCountdown({ resetAt, style }: Props) {
  const [now, setNow] = useState(() => new Date());

  useEffect(() => {
    const interval = setInterval(() => setNow(new Date()), tickInterval(resetAt));
    return () => clearInterval(interval);
  }, [resetAt]);

  const text =
    style === "countdown"
      ? formatCountdown(Math.floor((resetAt.getTime() - now.getTime()) / 1000))
      : formatAbsolute(resetAt, now);

  return <span className="reset-countdown">{text}</span>;
}
