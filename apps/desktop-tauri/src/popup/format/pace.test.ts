import { describe, expect, it } from "vitest";
import { buildPaceText } from "./pace";

const base = {
  elapsedPercent: 50,
  deltaPercent: 0,
  remainingPercent: 50,
  secondsUntilReset: 60 * 60 * 24,
  secondsUntilRunOut: 60 * 60 * 24,
  runOutRiskPercent: null,
};

describe("buildPaceText", () => {
  it("hides both captions below 3% elapsed", () => {
    const t = buildPaceText({ ...base, elapsedPercent: 2.9 });
    expect(t.left).toBeNull();
    expect(t.right).toBeNull();
  });

  it("shows On pace when delta rounds to zero", () => {
    const t = buildPaceText({ ...base, deltaPercent: 0.3 });
    expect(t.left).toBe("On pace");
  });

  it("shows N% in deficit when ahead of pace", () => {
    const t = buildPaceText({ ...base, deltaPercent: 12.4 });
    expect(t.left).toBe("12% in deficit");
  });

  it("shows N% in reserve when behind pace", () => {
    const t = buildPaceText({ ...base, deltaPercent: -7.6 });
    expect(t.left).toBe("8% in reserve");
  });

  it("collapses to Runs out now when remaining is zero", () => {
    const t = buildPaceText({ ...base, remainingPercent: 0 });
    expect(t.right).toBe("Runs out now");
  });

  it("says Lasts until reset when run out is past reset", () => {
    const t = buildPaceText({
      ...base,
      secondsUntilRunOut: base.secondsUntilReset! + 60,
    });
    expect(t.right).toBe("Lasts until reset");
  });

  it("says Lasts until reset when run out is unknown", () => {
    const t = buildPaceText({ ...base, secondsUntilRunOut: null });
    expect(t.right).toBe("Lasts until reset");
  });

  it("formats duration as minutes when under an hour", () => {
    const t = buildPaceText({ ...base, secondsUntilRunOut: 60 * 42 });
    expect(t.right).toBe("Runs out in 42m");
  });

  it("formats duration as h m when under a day", () => {
    const t = buildPaceText({
      ...base,
      secondsUntilRunOut: 60 * 60 * 5 + 60 * 17,
    });
    expect(t.right).toBe("Runs out in 5h 17m");
  });

  it("formats duration as d h when over a day", () => {
    const t = buildPaceText({
      ...base,
      secondsUntilRunOut: 60 * 60 * 24 * 2 + 60 * 60 * 3,
      secondsUntilReset: 60 * 60 * 24 * 5,
    });
    expect(t.right).toBe("Runs out in 2d 3h");
  });

  it("appends rounded risk suffix when set", () => {
    const t = buildPaceText({
      ...base,
      secondsUntilRunOut: 60 * 60 * 3,
      runOutRiskPercent: 23,
    });
    expect(t.right).toBe("Runs out in 3h • ~25% run out risk");
  });

  it("ignores zero risk", () => {
    const t = buildPaceText({
      ...base,
      secondsUntilRunOut: 60 * 60 * 3,
      runOutRiskPercent: 0,
    });
    expect(t.right).toBe("Runs out in 3h");
  });
});
