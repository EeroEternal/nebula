#!/usr/bin/env node
/**
 * Fill missing locale keys from the 5-language union.
 * Value order: en-US, then zh-CN, then first locale that has it.
 * Writes only; `check-locales.mjs` is the fail-closed gate.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  listLocales,
  listLocaleFiles,
  collectLocaleKeyMap,
} from './check-locales.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const localesRoot = join(__dirname, '..', 'src', 'locales');
const VALUE_ORDER = ['en-US', 'zh-CN'];
const MODULES = [
  'models',
  'model',
  'monitoring',
  'management',
  'dashboard',
  'global',
  'menu',
  'pages',
  'admin',
  'tasks',
  'app',
];

function moduleForKey(key) {
  if (key === 'monitor' || key.startsWith('monitor.')) {
    return 'monitoring';
  }
  const hit = MODULES.find((m) => key === m || key.startsWith(`${m}.`));
  return hit || 'global';
}

function jsString(s) {
  return `'${String(s).replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
}

function pickValue(key, byLocale) {
  for (const loc of VALUE_ORDER) {
    const v = byLocale.get(loc)?.values.get(key);
    if (typeof v === 'string') {
      return v;
    }
  }
  for (const { values } of byLocale.values()) {
    const v = values.get(key);
    if (typeof v === 'string') {
      return v;
    }
  }
  return key;
}

function appendKeys(filePath, entries) {
  let src = readFileSync(filePath, 'utf8');
  const idx = src.lastIndexOf('};');
  if (idx < 0) {
    throw new Error(`no closing }; in ${filePath}`);
  }
  const block = entries
    .map(([k, v]) => `  ${jsString(k)}: ${jsString(v)},\n`)
    .join('');
  writeFileSync(filePath, `${src.slice(0, idx)}\n${block}${src.slice(idx)}`);
}

function main() {
  const locales = listLocales();
  const byLocale = collectLocaleKeyMap();
  const union = new Set();
  for (const { keys } of byLocale.values()) {
    for (const k of keys) {
      union.add(k);
    }
  }

  let added = 0;
  for (const locale of locales) {
    const { keys } = byLocale.get(locale);
    const missing = [...union].filter((k) => !keys.has(k)).sort();
    if (!missing.length) {
      continue;
    }
    const byFile = new Map();
    for (const key of missing) {
      const mod = moduleForKey(key);
      if (!byFile.has(mod)) {
        byFile.set(mod, []);
      }
      byFile.get(mod).push([key, pickValue(key, byLocale)]);
    }
    const existing = new Set(listLocaleFiles(locale).map((f) => f.replace(/\.ts$/, '')));
    for (const [mod, entries] of byFile) {
      if (!existing.has(mod)) {
        throw new Error(`${locale} has no ${mod}.ts for ${entries.length} keys`);
      }
      appendKeys(join(localesRoot, locale, `${mod}.ts`), entries);
      added += entries.length;
      console.log(`+ ${locale}/${mod}.ts  ${entries.length}`);
    }
  }
  console.log(`sync-locale-keys: added ${added} entries`);
}

main();
