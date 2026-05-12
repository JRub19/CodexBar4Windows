// Phase 3 D7: builds the two pace text strings for a metric row per
// spec 15 section 6.
//
// Inputs:
// - elapsedPercent: 0..100 fraction of the current quota window elapsed.
//   Below 3 we hide pace altogether because the signal is unreliable.
// - deltaPercent: positive when the user is ahead of pace (running out
//   faster than the window allows), negative when behind, zero when on
//   pace. Expressed in percentage points relative to ideal.
// - remainingPercent: 0..100 quota remaining. When 0 the message
//   becomes "Runs out now".
// - lastUntilResetSec / runOutInSec: seconds until reset, and seconds
//   until projected exhaustion. The latter drives "Runs out in {dur}".
// - runOutRiskPercent: 0..100 risk of running out before reset. Rounded
//   to the nearest 5 and appended as " * ~{R}% run out risk" when set.

export interface PaceTextInput {
  elapsedPercent: number;
  deltaPercent: number;
  remainingPercent: number;
  secondsUntilReset: number | null;
  secondsUntilRunOut: number | null;
  runOutRiskPercent: number | null;
}

export interface PaceText {
  // Left caption. May be null when below the 3% threshold.
  left: string | null;
  // Right caption with optional risk suffix.
  right: string | null;
}

const ELAPSED_FLOOR = 3;

function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds / 60));
  if (total < 60) return `${total}m`;
  const hours = Math.floor(total / 60);
  const mins = total % 60;
  if (hours < 24) return mins === 0 ? `${hours}h` : `${hours}h ${mins}m`;
  const days = Math.floor(hours / 24);
  const remHours = hours % 24;
  return remHours === 0 ? `${days}d` : `${days}d ${remHours}h`;
}

function buildLeft({ elapsedPercent, deltaPercent }: PaceTextInput): string | null {
  if (elapsedPercent < ELAPSED_FLOOR) return null;
  const n = Math.round(Math.abs(deltaPercent));
  if (n === 0) return "On pace";
  return deltaPercent > 0 ? `${n}% in deficit` : `${n}% in reserve`;
}

function buildRight(input: PaceTextInput): string | null {
  if (input.elapsedPercent < ELAPSED_FLOOR) return null;
  let base: string;
  if (input.remainingPercent <= 0) {
    base = "Runs out now";
  } else if (input.secondsUntilRunOut == null) {
    base = "Lasts until reset";
  } else if (
    input.secondsUntilReset != null &&
    input.secondsUntilRunOut >= input.secondsUntilReset
  ) {
    base = "Lasts until reset";
  } else {
    base = `Runs out in ${formatDuration(input.secondsUntilRunOut)}`;
  }
  if (input.runOutRiskPercent != null && input.runOutRiskPercent > 0) {
    const r = Math.round(input.runOutRiskPercent / 5) * 5;
    return `${base} • ~${r}% run out risk`;
  }
  return base;
}

export function buildPaceText(input: PaceTextInput): PaceText {
  return { left: buildLeft(input), right: buildRight(input) };
}
