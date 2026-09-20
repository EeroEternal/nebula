#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  listLocales,
  listLocaleFiles,
  collectLocaleKeyMap,
  collectReferencedKeys,
  loadLocaleObject,
} from './check-locales.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const localesRoot = join(__dirname, '..', 'src', 'locales');
const CJK = /[\u3400-\u9fff]/;
const KEEP_PREFIXES = [
  'menu.',
  'global.model.type.',
  'global.model.format.',
  'global.model.size.',
  'pages.models.status.',
  'pages.models.form.',
  'monitor.',
  'component.',
  'error.',
];

function jsString(s) {
  return `'${String(s).replace(/\\/g, '\\\\').replace(/'/g, "\\'").replace(/\n/g, '\\n')}'`;
}

function headerOf(src) {
  const idx = src.indexOf('{');
  return idx >= 0 ? src.slice(0, idx + 1) : 'export default {';
}

function main() {
  const byLocale = collectLocaleKeyMap();
  const refs = collectReferencedKeys();
  const en = byLocale.get('en-US')?.values || new Map();
  const keep = new Set(refs.keys());
  for (const p of KEEP_PREFIXES) {
    for (const { keys } of byLocale.values()) {
      for (const k of keys) {
        if (k.startsWith(p) || k === p.slice(0, -1)) {
          keep.add(k);
        }
      }
    }
  }
  // always keep error.* just appended
  for (const k of en.keys()) {
    if (k.startsWith('error.')) {
      keep.add(k);
    }
  }

  let written = 0;
  for (const loc of listLocales()) {
    const locValues = byLocale.get(loc)?.values || new Map();
    for (const file of listLocaleFiles(loc)) {
      const abs = join(localesRoot, loc, file);
      const src = readFileSync(abs, 'utf8');
      const obj = loadLocaleObject(src);
      const entries = [];
      for (const [key, raw] of Object.entries(obj)) {
        if (!keep.has(key)) {
          continue;
        }
        let val = raw;
        if (
          (loc === 'ja-JP' || loc === 'ko-KR') &&
          typeof val === 'string' &&
          CJK.test(val)
        ) {
          const ev = en.get(key);
          if (typeof ev === 'string' && ev && !CJK.test(ev)) {
            val = ev;
          }
        }
        entries.push([key, val]);
      }
      const body = entries
        .map(([k, v]) => `  ${jsString(k)}: ${jsString(v)},`)
        .join('\n');
      writeFileSync(abs, `${headerOf(src)}\n${body}\n};\n`);
      written += 1;
    }
  }
  console.log(JSON.stringify({ files: written, keep: keep.size, refs: refs.size }));
}

main();
