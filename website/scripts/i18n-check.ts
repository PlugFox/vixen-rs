#!/usr/bin/env bun
/**
 * i18n locale parity check.
 *
 * For each EN namespace the script verifies that the matching RU namespace
 * contains:
 *  - the same set of top-level keys, and
 *  - a non-empty `ru:` value for every key.
 *
 * Fails with exit code 1 (grouped report) on any divergence. The CI
 * `website-ci.yml::i18n-parity` job invokes this directly.
 */

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parse } from "yaml";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPT_DIR, "..");
const SOURCE_DIR = join(ROOT, "i18n", "messages");
const LOCALES = ["en", "ru"] as const;

interface Entry {
  en?: string;
  ru?: string;
}

function readKeys(locale: string, ns: string): Map<string, string | undefined> {
  const file = join(SOURCE_DIR, locale, `${ns}.yaml`);
  if (!existsSync(file)) return new Map();
  const parsed = (parse(readFileSync(file, "utf8")) ?? {}) as Record<string, Entry>;
  const out = new Map<string, string | undefined>();
  for (const [k, v] of Object.entries(parsed)) {
    out.set(k, (v as Entry)?.[locale as keyof Entry]);
  }
  return out;
}

function listNamespaces(): string[] {
  const enDir = join(SOURCE_DIR, "en");
  if (!existsSync(enDir)) return [];
  return readdirSync(enDir)
    .filter((f) => f.endsWith(".yaml"))
    .map((f) => basename(f, ".yaml"))
    .sort();
}

interface NamespaceReport {
  ns: string;
  missingInRu: string[];
  extraInRu: string[];
  emptyRuValues: string[];
}

const namespaces = listNamespaces();
const reports: NamespaceReport[] = [];

for (const ns of namespaces) {
  const en = readKeys("en", ns);
  const ru = readKeys("ru", ns);
  const enKeys = new Set(en.keys());
  const ruKeys = new Set(ru.keys());
  const missingInRu: string[] = [];
  const extraInRu: string[] = [];
  const emptyRuValues: string[] = [];
  for (const k of enKeys) {
    if (!ruKeys.has(k)) missingInRu.push(k);
  }
  for (const k of ruKeys) {
    if (!enKeys.has(k)) extraInRu.push(k);
  }
  for (const k of ruKeys) {
    const v = ru.get(k);
    if (typeof v !== "string" || v.trim() === "") emptyRuValues.push(k);
  }
  if (missingInRu.length || extraInRu.length || emptyRuValues.length) {
    reports.push({ ns, missingInRu, extraInRu, emptyRuValues });
  }
}

const _localesUnused: readonly string[] = LOCALES;
void _localesUnused;

if (reports.length === 0) {
  console.log(`i18n-check: ✓ ${namespaces.length} namespaces in parity`);
  process.exit(0);
}

console.error(`i18n-check: ✗ ${reports.length} namespace(s) out of parity`);
for (const r of reports) {
  console.error(`\n[${r.ns}]`);
  if (r.missingInRu.length) {
    console.error(`  missing in ru/: ${r.missingInRu.join(", ")}`);
  }
  if (r.extraInRu.length) {
    console.error(`  extra in ru/ (no en/ counterpart): ${r.extraInRu.join(", ")}`);
  }
  if (r.emptyRuValues.length) {
    console.error(`  empty ru values: ${r.emptyRuValues.join(", ")}`);
  }
}
process.exit(1);
