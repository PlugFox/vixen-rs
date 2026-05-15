import { createSignal } from "solid-js";
import type { Locale, MessageDef, TranslationStore } from "./types";

const STORAGE_KEY = "vixen_locale";

const NAMESPACES = ["common", "auth", "chats", "moderation", "settings", "errors"] as const;

const SUPPORTED: readonly Locale[] = ["en", "ru"] as const;

function detectInitialLocale(): Locale {
  if (typeof localStorage !== "undefined") {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === "en" || stored === "ru") return stored;
  }
  if (typeof navigator !== "undefined") {
    const lang = navigator.language?.slice(0, 2).toLowerCase();
    if (lang === "ru") return "ru";
  }
  return "en";
}

const [locale, setLocaleSignal] = createSignal<Locale>(detectInitialLocale());
const [translations, setTranslations] = createSignal<TranslationStore>({});

export const currentLocale = locale;

/**
 * Fetch the JSON blobs for a locale and update the in-memory store.
 * `en` is a no-op fast path: the embedded `def.en` is the source of truth.
 */
async function loadLocale(loc: Locale): Promise<TranslationStore> {
  if (loc === "en") return {};
  const results = await Promise.allSettled(
    NAMESPACES.map(async (ns) => {
      const res = await fetch(`/locales/${loc}/${ns}.json`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const body = (await res.json()) as Record<string, string>;
      return [ns, body] as const;
    }),
  );
  const out: TranslationStore = {};
  for (const r of results) {
    if (r.status === "fulfilled") {
      const [ns, body] = r.value;
      out[ns] = body;
    }
  }
  return out;
}

/**
 * Translate a `MessageDef` with optional `{name}`-style interpolation. The
 * lookup is `translations()[ns][key]` with `def.en` as fallback — missing
 * RU keys do NOT throw, they degrade gracefully to English.
 */
export function t(def: MessageDef, params?: Record<string, string | number>): string {
  const store = translations();
  const raw = store[def.ns]?.[def.key] ?? def.en;
  if (!params) return raw;
  return raw.replace(/\{(\w+)\}/g, (_, k: string) =>
    Object.hasOwn(params, k) ? String(params[k]) : `{${k}}`,
  );
}

export async function setLocale(loc: Locale): Promise<void> {
  if (!SUPPORTED.includes(loc)) return;
  const store = await loadLocale(loc);
  setTranslations(store);
  setLocaleSignal(loc);
  if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, loc);
  if (typeof document !== "undefined") document.documentElement.lang = loc;
}

/**
 * Call once at app startup. Loads the persisted locale (RU translations or
 * an empty EN store) and applies it before the first paint of any
 * translated component.
 */
export async function initI18n(): Promise<void> {
  await setLocale(locale());
}

export { NAMESPACES };
