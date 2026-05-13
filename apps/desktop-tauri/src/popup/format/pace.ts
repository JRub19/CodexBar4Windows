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

// ============================================================
// Window-level pace computation (ports UsagePace.weekly from
// the macOS source).
//
// We infer the window's nominal duration from its label, project a
// linear ideal `expected = elapsed / duration`, and surface the
// signed delta plus a run-out ETA. Returns null when pace cannot be
// computed (no reset time, unknown label, window already over).
// ============================================================

const DURATION_TABLE_SECS: Array<[RegExp, number]> = [
  // Session-style 5-hour windows (Claude session, Codex 5h).
  [/^\s*session\s*$/i, 5 * 3600],
  [/\b5\s*h(?:our)?s?\b/i, 5 * 3600],
  // Weekly windows. Match common labels: "Week", "WEEK", "Weekly",
  // "WEEK (SONNET)", etc.
  [/\bweek(?:ly)?\b/i, 7 * 24 * 3600],
  // Monthly.
  [/\bmonth(?:ly)?\b/i, 30 * 24 * 3600],
  // Daily / 24-hour.
  [/\bday(?:ly)?\b/i, 24 * 3600],
  [/\b24\s*h(?:our)?s?\b/i, 24 * 3600],
  // Hourly.
  [/\b1\s*h(?:our)?\b/i, 3600],
  [/^\s*hour(?:ly)?\b/i, 3600],
];

export function inferWindowDurationSecs(label: string): number | null {
  for (const [re, secs] of DURATION_TABLE_SECS) {
    if (re.test(label)) return secs;
  }
  return null;
}

export type PaceSentiment = "ahead" | "behind" | "neutral";

export interface WindowPace {
  /** `+` = ahead of the spend curve (deficit, bad). `-` = in reserve. */
  deltaPercent: number;
  /** 0..100, what % SHOULD be used by now on the linear curve. */
  expectedUsedPercent: number;
  /** 0..100, what % SHOULD remain — `100 - expectedUsedPercent`. */
  expectedRemainingPercent: number;
  /** Sentiment grouping driven by sign + threshold. */
  sentiment: PaceSentiment;
  /** Formatted left + right captions. */
  text: PaceText;
}

export interface ComputeWindowPaceInput {
  /** 0..100, used percentage at the moment of evaluation. */
  usedPercent: number;
  /** Window label — used to infer duration. */
  label: string;
  /** Absolute reset time in unix seconds, or null. */
  resetAtUnixSecs: number | null;
  /** Optional duration override in seconds. */
  windowDurationSecs?: number | null;
  /** Optional now, defaults to wall-clock. */
  nowUnixSecs?: number;
}

export function computeWindowPace(
  input: ComputeWindowPaceInput,
): WindowPace | null {
  const now = input.nowUnixSecs ?? Math.floor(Date.now() / 1000);
  const duration =
    input.windowDurationSecs ?? inferWindowDurationSecs(input.label);
  if (!duration || duration <= 0) return null;
  if (input.resetAtUnixSecs == null) return null;
  const secondsUntilReset = input.resetAtUnixSecs - now;
  if (secondsUntilReset <= 0) return null;
  const elapsed = Math.max(0, Math.min(duration, duration - secondsUntilReset));
  if (elapsed <= 0) return null;
  const expectedUsedPercent = (elapsed / duration) * 100;
  const elapsedPercent = expectedUsedPercent;
  const used = Math.max(0, Math.min(100, input.usedPercent));
  const remainingPercent = Math.max(0, 100 - used);
  const deltaPercent = used - expectedUsedPercent;

  // Run-out ETA: linear projection from the rate so far.
  let secondsUntilRunOut: number | null = null;
  if (used > 0 && used < 100 && elapsed > 0) {
    const ratePerSec = used / elapsed;
    if (ratePerSec > 0) {
      secondsUntilRunOut = remainingPercent / ratePerSec;
    }
  }

  const text = buildPaceText({
    elapsedPercent,
    deltaPercent,
    remainingPercent,
    secondsUntilReset,
    secondsUntilRunOut,
    runOutRiskPercent: null,
  });

  // Sentiment threshold mirrors UsagePace.swift §classify (±2 = onTrack).
  let sentiment: PaceSentiment;
  if (Math.abs(deltaPercent) <= 2) sentiment = "neutral";
  else sentiment = deltaPercent > 0 ? "ahead" : "behind";

  return {
    deltaPercent,
    expectedUsedPercent,
    expectedRemainingPercent: 100 - expectedUsedPercent,
    sentiment,
    text,
  };
}
