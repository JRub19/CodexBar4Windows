import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import en from "./en.json";
import zhHans from "./zh-Hans.json";
import ptBR from "./pt-BR.json";

// Phase 8 Task 19: lightweight i18n provider for the popup + settings.
// Mirrors the macOS `Localizable.strings` key conventions so the keys
// can move between platforms 1:1.
//
// Locale resolution order, highest priority first:
//   1. explicit `setLocale()` call (driven by the Preferences
//      "Language" dropdown once it ships).
//   2. `appLanguage` from the persisted settings store.
//   3. `system` → `navigator.language` parsed to one of our codes.
//   4. fallback to `en`.
//
// Missing keys fall back to the English dictionary with a console
// warning — never throw, never blank the UI.

export type LocaleCode = "en" | "zh-Hans" | "pt-BR";
type Dictionary = Record<string, string>;

const DICTS: Record<LocaleCode, Dictionary> = {
  en: en as Dictionary,
  "zh-Hans": zhHans as Dictionary,
  "pt-BR": ptBR as Dictionary,
};

export const SUPPORTED_LOCALES: LocaleCode[] = ["en", "zh-Hans", "pt-BR"];

/** Map `navigator.language` (or any BCP-47 tag) to one of our codes. */
export function resolveSystemLocale(raw: string | undefined | null): LocaleCode {
  if (!raw) return "en";
  const lower = raw.toLowerCase();
  if (lower.startsWith("zh")) {
    // zh-Hans, zh-CN, zh-SG all collapse to Simplified.
    if (lower.includes("hant") || lower.includes("tw") || lower.includes("hk")) {
      // Traditional Chinese — not yet shipped, fall through to en.
      return "en";
    }
    return "zh-Hans";
  }
  if (lower.startsWith("pt")) {
    // pt-BR vs pt-PT; we only ship pt-BR so both map there.
    return "pt-BR";
  }
  if (lower.startsWith("en")) return "en";
  return "en";
}

interface I18nContextValue {
  locale: LocaleCode;
  setLocale: (next: LocaleCode | "system") => void;
  t: (key: string, vars?: Record<string, string | number>) => string;
}

const I18nContext = createContext<I18nContextValue | null>(null);

interface ProviderProps {
  /** Override (e.g. tests). Falls back to the system locale otherwise. */
  initialLocale?: LocaleCode;
  children: ReactNode;
}

export function I18nProvider({ initialLocale, children }: ProviderProps) {
  const [locale, setLocaleState] = useState<LocaleCode>(
    initialLocale ??
      resolveSystemLocale(
        typeof navigator !== "undefined" ? navigator.language : "en",
      ),
  );

  useEffect(() => {
    // React-side reflection of the document `lang` attribute keeps
    // screen readers in sync with our resolved locale.
    if (typeof document !== "undefined") {
      document.documentElement.lang = locale;
    }
  }, [locale]);

  const setLocale = useCallback((next: LocaleCode | "system") => {
    if (next === "system") {
      setLocaleState(
        resolveSystemLocale(
          typeof navigator !== "undefined" ? navigator.language : "en",
        ),
      );
    } else {
      setLocaleState(next);
    }
  }, []);

  // Bridge to the persisted `settings.app_language` value: read it on
  // boot and live-react to `settings:changed` events emitted from any
  // window when the Appearance pane writes a new value. This is what
  // makes the language picker take effect immediately across both the
  // popup and the Preferences window without needing a relaunch.
  useEffect(() => {
    if (initialLocale) return; // tests override system + settings
    let cancelled = false;

    const apply = (raw: string | null | undefined) => {
      if (cancelled) return;
      if (raw && SUPPORTED_LOCALES.includes(raw as LocaleCode)) {
        setLocaleState(raw as LocaleCode);
      } else {
        setLocaleState(
          resolveSystemLocale(
            typeof navigator !== "undefined" ? navigator.language : "en",
          ),
        );
      }
    };

    void invoke<{ app_language: string | null }>("get_settings")
      .then((s) => apply(s.app_language))
      .catch(() => {
        // No backend (vitest/jsdom); keep the navigator-derived locale.
      });

    let unlisten: (() => void) | null = null;
    void listen<{ settings: { app_language: string | null } }>(
      "settings:changed",
      (event) => apply(event.payload?.settings?.app_language ?? null),
    )
      .then((stop) => {
        if (cancelled) {
          stop();
        } else {
          unlisten = stop;
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [initialLocale]);

  const t = useCallback(
    (key: string, vars?: Record<string, string | number>) => {
      const dict = DICTS[locale];
      let value = dict[key];
      if (value === undefined) {
        const fallback = DICTS.en[key];
        if (fallback === undefined) {
          // eslint-disable-next-line no-console
          console.warn(`[i18n] missing key "${key}" in en + ${locale}`);
          return key;
        }
        if (locale !== "en") {
          // eslint-disable-next-line no-console
          console.warn(`[i18n] missing key "${key}" in ${locale}, using en`);
        }
        value = fallback;
      }
      if (vars) {
        for (const [k, v] of Object.entries(vars)) {
          value = value.split(`{${k}}`).join(String(v));
        }
      }
      return value;
    },
    [locale],
  );

  const ctx = useMemo<I18nContextValue>(
    () => ({ locale, setLocale, t }),
    [locale, setLocale, t],
  );

  return <I18nContext.Provider value={ctx}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const v = useContext(I18nContext);
  if (!v) {
    // Without a provider mounted, we fall back to a do-nothing impl
    // so unit tests rendering raw components don't have to mount it.
    return {
      locale: "en",
      setLocale: () => {},
      t: (key) => (DICTS.en[key] ?? key),
    };
  }
  return v;
}

export function useT() {
  return useI18n().t;
}
