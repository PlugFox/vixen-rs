/**
 * Types consumed by both the codegen output (`generated/<ns>.ts`) and the
 * runtime fetcher (`i18n.ts`).
 *
 * A `MessageDef` carries the namespace + key + EN fallback. At runtime the
 * fetcher loads `/locales/<locale>/<ns>.json` blobs and looks up by key,
 * falling back to `def.en` if a translation is missing.
 */

export type Locale = "en" | "ru";

export interface MessageDef {
  readonly ns: string;
  readonly key: string;
  readonly en: string;
}

/** Mapping `{ [ns]: { [key]: translated-string } }` for a single locale. */
export type TranslationStore = Record<string, Record<string, string>>;
