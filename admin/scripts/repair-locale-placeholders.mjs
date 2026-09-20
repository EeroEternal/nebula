#!/usr/bin/env node
/**
 * Replace `'key': 'key'` placeholders left by a failed value parse.
 * Source order: en-US, then zh-CN, then any locale with a real string.
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

function jsString(s) {
  return `'${String(s).replace(/\\/g, '\\\\').replace(/'/g, "\\'")}'`;
}

function pickValue(key, locale, byLocale) {
  const own = byLocale.get(locale)?.values.get(key);
  if (typeof own === 'string' && own !== key) {
    return own;
  }
  for (const loc of VALUE_ORDER) {
    if (loc === locale) continue;
    const v = byLocale.get(loc)?.values.get(key);
    if (typeof v === 'string' && v !== key) {
      return v;
    }
  }
  for (const [loc, { values }] of byLocale) {
    if (loc === locale) continue;
    const v = values.get(key);
    if (typeof v === 'string' && v !== key) {
      return v;
    }
  }
  return null;
}

function main() {
  const byLocale = collectLocaleKeyMap();
  let replaced = 0;
  let leftover = 0;
  for (const locale of listLocales()) {
    for (const file of listLocaleFiles(locale)) {
      const abs = join(localesRoot, locale, file);
      let src = readFileSync(abs, 'utf8');
      const next = src.replace(
        /^(\s*)'((?:\\.|[^'\\])*)':\s*'\2',?\s*$/gm,
        (full, indent, key) => {
          const val = pickValue(key, locale, byLocale);
          if (val == null) {
            leftover += 1;
            return full;
          }
          replaced += 1;
          return `${indent}${jsString(key)}: ${jsString(val)},`;
        },
      );
      if (next !== src) {
        writeFileSync(abs, next);
        console.log(`~ ${locale}/${file}`);
      }
    }
  }
  console.log(`repair-locale-placeholders: replaced ${replaced}, leftover ${leftover}`);
}

main();
