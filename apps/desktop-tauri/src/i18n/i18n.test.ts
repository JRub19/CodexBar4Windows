import { describe, expect, it } from "vitest";
import { resolveSystemLocale, SUPPORTED_LOCALES } from "./index";

// Phase 8 / Polish A2: regression tests for the locale resolver +
// the supported-locale list. The full I18nProvider lifecycle (settings
// fetch + event subscription) is exercised manually since vitest's
// jsdom environment doesn't host Tauri's IPC; here we cover the pure
// `resolveSystemLocale` mapping which is the only piece with branching
// logic worth gating.

describe("resolveSystemLocale", () => {
  it("maps en-US, en-GB, en to en", () => {
    expect(resolveSystemLocale("en-US")).toBe("en");
    expect(resolveSystemLocale("en-GB")).toBe("en");
    expect(resolveSystemLocale("en")).toBe("en");
  });

  it("maps zh-CN, zh-SG, zh-Hans to zh-Hans", () => {
    expect(resolveSystemLocale("zh-CN")).toBe("zh-Hans");
    expect(resolveSystemLocale("zh-SG")).toBe("zh-Hans");
    expect(resolveSystemLocale("zh-Hans")).toBe("zh-Hans");
  });

  it("maps zh-Hant, zh-TW, zh-HK back to en (Traditional not shipped)", () => {
    expect(resolveSystemLocale("zh-Hant")).toBe("en");
    expect(resolveSystemLocale("zh-TW")).toBe("en");
    expect(resolveSystemLocale("zh-HK")).toBe("en");
  });

  it("maps pt-BR and pt-PT to pt-BR (single ptVariant shipped)", () => {
    expect(resolveSystemLocale("pt-BR")).toBe("pt-BR");
    expect(resolveSystemLocale("pt-PT")).toBe("pt-BR");
    expect(resolveSystemLocale("pt")).toBe("pt-BR");
  });

  it("falls back to en for unknown locales", () => {
    expect(resolveSystemLocale("fr-FR")).toBe("en");
    expect(resolveSystemLocale("ja-JP")).toBe("en");
    expect(resolveSystemLocale("")).toBe("en");
    expect(resolveSystemLocale(undefined)).toBe("en");
    expect(resolveSystemLocale(null)).toBe("en");
  });

  it("is case-insensitive", () => {
    expect(resolveSystemLocale("EN-us")).toBe("en");
    expect(resolveSystemLocale("ZH-cn")).toBe("zh-Hans");
  });
});

describe("SUPPORTED_LOCALES", () => {
  it("exposes exactly the three shipped locales", () => {
    expect([...SUPPORTED_LOCALES].sort()).toEqual([
      "en",
      "pt-BR",
      "zh-Hans",
    ]);
  });
});
