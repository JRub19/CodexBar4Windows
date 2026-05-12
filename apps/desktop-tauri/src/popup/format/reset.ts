// Phase 3 D8: reset countdown formatters per spec 15 sections 7.1, 7.2,
// 7.4. Pure functions so they stay testable without DOM or timers.

export type CountdownPart = "d" | "h" | "m";

function ceilMinutes(seconds: number): number {
  return Math.max(1, Math.ceil(seconds / 60));
}

function pieces(totalMinutes: number): { d: number; h: number; m: number } {
  const d = Math.floor(totalMinutes / (60 * 24));
  const afterDays = totalMinutes - d * 60 * 24;
  const h = Math.floor(afterDays / 60);
  const m = afterDays - h * 60;
  return { d, h, m };
}

// Section 7.1: countdown style. Caller passes seconds until reset; we
// emit one of "Resets now", "Resets in {d}d {h}h", "Resets in {d}d",
// "Resets in {h}h {m}m", "Resets in {h}h", or "Resets in {m}m".
export function formatCountdown(secondsUntilReset: number): string {
  if (secondsUntilReset <= 0) return "Resets now";
  const totalMin = ceilMinutes(secondsUntilReset);
  const { d, h, m } = pieces(totalMin);
  if (d > 0) {
    return h > 0 ? `Resets in ${d}d ${h}h` : `Resets in ${d}d`;
  }
  if (h > 0) {
    return m > 0 ? `Resets in ${h}h ${m}m` : `Resets in ${h}h`;
  }
  return `Resets in ${m}m`;
}

// Section 7.2: absolute style. Uses the user's locale via Intl.
// - same day: "Resets HH:MM"
// - next calendar day: "Resets tomorrow, HH:MM"
// - later: "Resets MMM d, HH:MM"
export function formatAbsolute(target: Date, now: Date = new Date()): string {
  const time = new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(target);
  const startOfDay = (d: Date) =>
    new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const dayDiff = Math.round(
    (startOfDay(target) - startOfDay(now)) / (24 * 60 * 60 * 1000),
  );
  if (dayDiff <= 0) return `Resets ${time}`;
  if (dayDiff === 1) return `Resets tomorrow, ${time}`;
  const date = new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(target);
  return `Resets ${date}, ${time}`;
}

// Section 7.4: "Updated" caption next to the refresh icon. Caller passes
// seconds elapsed since the last successful refresh.
export function formatUpdated(
  secondsSinceUpdate: number,
  now: Date = new Date(),
  lastUpdate: Date = new Date(now.getTime() - secondsSinceUpdate * 1000),
): string {
  if (secondsSinceUpdate < 60) return "Updated just now";
  if (secondsSinceUpdate < 24 * 60 * 60) {
    const totalMin = Math.max(1, Math.floor(secondsSinceUpdate / 60));
    if (totalMin < 60) return `Updated ${totalMin}m ago`;
    const h = Math.floor(totalMin / 60);
    return `Updated ${h}h ago`;
  }
  const time = new Intl.DateTimeFormat(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  }).format(lastUpdate);
  return `Updated ${time}`;
}
