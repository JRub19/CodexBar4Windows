import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhHans from "./zh-Hans.json";
import ptBR from "./pt-BR.json";

// Phase 9 §D-1 + §B-9 coverage tests:
//
//   1. Every key present in `en.json` must also exist in `zh-Hans.json`
//      and `pt-BR.json`. Drift here is the most common i18n bug:
//      shipping a new English string and forgetting to translate it,
//      so the user sees `t("some.new.key")` instead of "Cancel".
//
//   2. The English bundle must contain no exclamation marks. Spec 80
//      §rules: CodexBar's voice is calm and informative, not chirpy.
//      The `!` rule is enforced here so any PR adding "Welcome!" fails
//      CI before shipping. (Other locales may need `!` for grammar.)

const EN_KEYS = Object.keys(en).sort();
const ZH_KEYS = new Set(Object.keys(zhHans));
const PT_KEYS = new Set(Object.keys(ptBR));

describe("locale coverage", () => {
  it("zh-Hans has every key present in en", () => {
    const missing = EN_KEYS.filter((k) => !ZH_KEYS.has(k));
    expect(missing, `missing keys in zh-Hans: ${missing.join(", ")}`).toEqual(
      [],
    );
  });

  it("pt-BR has every key present in en", () => {
    const missing = EN_KEYS.filter((k) => !PT_KEYS.has(k));
    expect(missing, `missing keys in pt-BR: ${missing.join(", ")}`).toEqual(
      [],
    );
  });

  it("zh-Hans does not contain keys absent from en (catch typos)", () => {
    const enSet = new Set(EN_KEYS);
    const extra = Object.keys(zhHans).filter((k) => !enSet.has(k));
    expect(extra, `unknown keys in zh-Hans: ${extra.join(", ")}`).toEqual([]);
  });

  it("pt-BR does not contain keys absent from en (catch typos)", () => {
    const enSet = new Set(EN_KEYS);
    const extra = Object.keys(ptBR).filter((k) => !enSet.has(k));
    expect(extra, `unknown keys in pt-BR: ${extra.join(", ")}`).toEqual([]);
  });
});

describe("english voice lint (Phase 9 §B-9)", () => {
  it("contains no exclamation marks", () => {
    const offenders: string[] = [];
    for (const [key, value] of Object.entries(en)) {
      if (typeof value === "string" && value.includes("!")) {
        offenders.push(`${key}: ${value}`);
      }
    }
    expect(
      offenders,
      `exclamation marks found in en.json — CodexBar's voice is calm, not chirpy:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  it("does not start or end with whitespace (catch copy-paste errors)", () => {
    const offenders: string[] = [];
    for (const [key, value] of Object.entries(en)) {
      if (typeof value === "string" && value !== value.trim()) {
        offenders.push(`${key}: <<${value}>>`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
