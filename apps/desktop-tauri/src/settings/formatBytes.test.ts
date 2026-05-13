import { describe, expect, it } from "vitest";
import { formatBytes } from "./SettingsApp";

// Polish A3: lock the storage-footprint size formatting. The Cost
// pane renders these strings inline, so any drift would show up
// in the UI immediately.

describe("formatBytes", () => {
  it("renders 0 specially", () => {
    expect(formatBytes(0)).toBe("0 B");
  });

  it("renders raw bytes without decimals", () => {
    expect(formatBytes(7)).toBe("7 B");
    expect(formatBytes(512)).toBe("512 B");
  });

  it("renders KB with two decimals under 10, one above", () => {
    expect(formatBytes(2048)).toBe("2.00 KB");
    expect(formatBytes(12 * 1024)).toBe("12.0 KB");
  });

  it("renders MB / GB with the same precision rule", () => {
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.00 MB");
    // 500 >= 10 so the formatter switches to one decimal.
    expect(formatBytes(500 * 1024 * 1024)).toBe("500.0 MB");
    expect(formatBytes(2.5 * 1024 * 1024 * 1024)).toBe("2.50 GB");
  });

  it("caps the unit at TB for absurd values", () => {
    const huge = 5 * 1024 * 1024 * 1024 * 1024;
    expect(formatBytes(huge)).toBe("5.00 TB");
  });

  it("crosses unit boundaries at exactly the right input", () => {
    // Below the boundary stays in the previous unit; at or above it
    // jumps to the next unit. Within-bucket rounding collisions are
    // intentional (1024 and 1025 both round to "1.00 KB").
    expect(formatBytes(1023)).toBe("1023 B");
    expect(formatBytes(1024)).toBe("1.00 KB");
    // 1024-1 KB still rounds within KB; >=10 so one-decimal precision.
    expect(formatBytes(1024 * 1024 - 1)).toBe("1024.0 KB");
    expect(formatBytes(1024 * 1024)).toBe("1.00 MB");
  });
});
