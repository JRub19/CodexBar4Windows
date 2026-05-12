import { describe, expect, it } from "vitest";
import { formatAbsolute, formatCountdown, formatUpdated } from "./reset";

describe("formatCountdown", () => {
  it("returns Resets now at 0", () => {
    expect(formatCountdown(0)).toBe("Resets now");
    expect(formatCountdown(-5)).toBe("Resets now");
  });

  it("rounds sub-minute up to one minute", () => {
    expect(formatCountdown(10)).toBe("Resets in 1m");
  });

  it("formats minutes only under an hour", () => {
    expect(formatCountdown(60 * 17)).toBe("Resets in 17m");
  });

  it("formats h m within a day", () => {
    expect(formatCountdown(60 * 60 * 4 + 60 * 22)).toBe("Resets in 4h 22m");
  });

  it("drops minutes when zero", () => {
    expect(formatCountdown(60 * 60 * 4)).toBe("Resets in 4h");
  });

  it("formats d h over a day", () => {
    expect(formatCountdown(60 * 60 * 24 * 2 + 60 * 60 * 3)).toBe(
      "Resets in 2d 3h",
    );
  });

  it("drops hours when zero on multi day", () => {
    expect(formatCountdown(60 * 60 * 24 * 3)).toBe("Resets in 3d");
  });
});

describe("formatAbsolute", () => {
  const now = new Date(2026, 4, 12, 9, 30); // 2026-05-12 09:30 local.

  it("uses Resets HH:MM for same day", () => {
    const target = new Date(2026, 4, 12, 17, 0);
    expect(formatAbsolute(target, now)).toMatch(/^Resets [0-9]+[:.][0-9]+/);
  });

  it("uses Resets tomorrow, HH:MM for next day", () => {
    const target = new Date(2026, 4, 13, 8, 15);
    expect(formatAbsolute(target, now)).toMatch(/^Resets tomorrow, /);
  });

  it("uses Resets MMM d, HH:MM for later dates", () => {
    const target = new Date(2026, 4, 20, 14, 0);
    const out = formatAbsolute(target, now);
    expect(out.startsWith("Resets ")).toBe(true);
    expect(out).toMatch(/,/);
  });
});

describe("formatUpdated", () => {
  it("says just now under 60 seconds", () => {
    expect(formatUpdated(15)).toBe("Updated just now");
  });

  it("abbreviates minutes under an hour", () => {
    expect(formatUpdated(60 * 7)).toBe("Updated 7m ago");
  });

  it("abbreviates hours under a day", () => {
    expect(formatUpdated(60 * 60 * 3 + 60 * 12)).toBe("Updated 3h ago");
  });

  it("uses absolute time after 24 hours", () => {
    const lastUpdate = new Date(2026, 4, 10, 8, 15);
    const now = new Date(2026, 4, 12, 9, 30);
    const out = formatUpdated(
      Math.floor((now.getTime() - lastUpdate.getTime()) / 1000),
      now,
      lastUpdate,
    );
    expect(out.startsWith("Updated ")).toBe(true);
    expect(out).not.toMatch(/ago/);
  });
});
