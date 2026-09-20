#!/usr/bin/env node
/**
 * Locale gate: syntax + 5-language key-set equality + static refs exist in zh-CN.
 * Used by Agent delivery gate, `npm run check:locales`, and frontend-gate CI.
 */
import {
  readdirSync,
  readFileSync,
  mkdtempSync,
  writeFileSync,
  rmSync,
} from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import vm from 'node:vm';

const __dirname = dirname(fileURLToPath(import.meta.url));
const frontendRoot = join(__dirname, '..');
const localesRoot = join(frontendRoot, 'src', 'locales');
const srcRoot = join(frontendRoot, 'src');
const DEFAULT_LOCALE = 'zh-CN';
const LIST_CAP = 40;

export function listLocales() {
  return readdirSync(localesRoot, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name)
    .sort();
}

export function listLocaleFiles(locale) {
  return readdirSync(join(localesRoot, locale))
    .filter((f) => f.endsWith('.ts'))
    .sort();
}

function stripTsForCheck(src) {
  return src
    .replace(/^\s*export\s+default\s+/m, 'module.exports = ')
    .replace(/\s+as\s+const\b/g, '')
    .replace(/^\s*import\s+.+?;\s*$/gm, '');
}

export function extractKeysFromSource(src) {
  const keys = new Set();
  const re = /'((?:\\.|[^'\\])*)'\s*:/g;
  let m;
  while ((m = re.exec(src))) {
    keys.add(m[1].replace(/\\'/g, "'"));
  }
  return keys;
}

export function loadLocaleObject(src) {
  const expr = src
    .replace(/^\s*import\s+.+?;\s*$/gm, '')
    .replace(/^\s*export\s+default\s+/m, '')
    .replace(/\s+as\s+const\b/g, '')
    .trim()
    .replace(/;\s*$/, '');
  return vm.runInNewContext(`(${expr})`, Object.create(null), { timeout: 2000 });
}

export function collectLocaleKeyMap() {
  const locales = listLocales();
  const byLocale = new Map();
  for (const locale of locales) {
    const keys = new Set();
    const values = new Map();
    for (const file of listLocaleFiles(locale)) {
      const src = readFileSync(join(localesRoot, locale, file), 'utf8');
      for (const k of extractKeysFromSource(src)) {
        keys.add(k);
      }
      try {
        const obj = loadLocaleObject(src);
        if (obj && typeof obj === 'object') {
          for (const [k, v] of Object.entries(obj)) {
            if (typeof v === 'string') {
              values.set(k, v);
            }
          }
        }
      } catch {
        // syntax check reports parse errors; values stay from other files
      }
    }
    byLocale.set(locale, { keys, values });
  }
  return byLocale;
}

function walkSrcFiles(dir, acc = []) {
  for (const ent of readdirSync(dir, { withFileTypes: true })) {
    if (
      ent.name === 'locales' ||
      ent.name === 'node_modules' ||
      ent.name === 'dist' ||
      ent.name.startsWith('.')
    ) {
      continue;
    }
    const p = join(dir, ent.name);
    if (ent.isDirectory()) {
      walkSrcFiles(p, acc);
    } else if (/\.(ts|tsx|js|jsx)$/.test(ent.name)) {
      acc.push(p);
    }
  }
  return acc;
}

const REF_PATTERNS = [
  /\bl(?:Get|Element)?\(\s*(['"])([^'"`]+)\1/g,
  /<FormattedMessage\b[^>]*\bid=\s*(?:\{\s*)?(['"])([^'"`]+)\1/g,
  /\.formatMessage\(\s*\{\s*id:\s*(['"])([^'"`]+)\1/g,
];

export function collectReferencedKeys() {
  const refs = new Map();
  for (const file of walkSrcFiles(srcRoot)) {
    const src = readFileSync(file, 'utf8');
    const rel = relative(frontendRoot, file).replace(/\\/g, '/');
    for (const re of REF_PATTERNS) {
      re.lastIndex = 0;
      let m;
      while ((m = re.exec(src))) {
        const id = m[2];
        if (!id || id.includes('${')) {
          continue;
        }
        if (!refs.has(id)) {
          refs.set(id, []);
        }
        const hits = refs.get(id);
        if (hits.length < 3) {
          hits.push(rel);
        }
      }
    }
  }
  return refs;
}

function printList(items) {
  const arr = [...items];
  for (const item of arr.slice(0, LIST_CAP)) {
    console.error(`  - ${item}`);
  }
  if (arr.length > LIST_CAP) {
    console.error(`  … ${arr.length - LIST_CAP} more`);
  }
}

function checkSyntax(locales) {
  let failed = 0;
  for (const locale of locales) {
    for (const file of listLocaleFiles(locale)) {
      const abs = join(localesRoot, locale, file);
      const rel = `src/locales/${locale}/${file}`;
      const src = stripTsForCheck(readFileSync(abs, 'utf8'));
      const tmp = mkdtempSync(join(tmpdir(), 'powerllm-locale-'));
      const out = join(tmp, file.replace(/\.ts$/, '.js'));
      try {
        writeFileSync(out, src);
        const r = spawnSync(process.execPath, ['--check', out], {
          encoding: 'utf8',
        });
        if (r.status !== 0) {
          failed += 1;
          console.error(`FAIL ${rel}`);
          console.error((r.stderr || r.stdout || '').trim());
        } else {
          console.log(`OK   ${rel}`);
        }
      } finally {
        rmSync(tmp, { recursive: true, force: true });
      }
    }
  }
  return failed;
}

function main() {
  const locales = listLocales();
  if (!locales.length) {
    console.error(`no locale dirs under ${localesRoot}`);
    process.exit(1);
  }
  if (!locales.includes(DEFAULT_LOCALE)) {
    console.error(`default locale ${DEFAULT_LOCALE} not found`);
    process.exit(1);
  }

  let failed = checkSyntax(locales);
  if (failed) {
    console.error(`\nlocale syntax check failed: ${failed} file(s)`);
    process.exit(1);
  }
  console.log(`\nlocale syntax check passed (${locales.length} locales)`);

  const byLocale = collectLocaleKeyMap();
  const union = new Set();
  for (const { keys } of byLocale.values()) {
    for (const k of keys) {
      union.add(k);
    }
  }

  let keySetFailed = 0;
  for (const locale of locales) {
    const { keys } = byLocale.get(locale);
    const missing = [...union].filter((k) => !keys.has(k)).sort();
    const extra = [...keys].filter((k) => !union.has(k)).sort();
    if (missing.length) {
      keySetFailed += 1;
      console.error(
        `\nFAIL key-set: ${locale} missing ${missing.length} vs union`,
      );
      printList(missing);
    }
    if (extra.length) {
      keySetFailed += 1;
      console.error(`\nFAIL key-set: ${locale} extra ${extra.length}`);
      printList(extra);
    }
  }
  if (!keySetFailed) {
    console.log(`locale key-set check passed (${union.size} keys, ${locales.length} locales)`);
  }

  const refs = collectReferencedKeys();
  const defaultKeys = byLocale.get(DEFAULT_LOCALE).keys;
  const missingRefs = [...refs.keys()]
    .filter((k) => !defaultKeys.has(k))
    .sort();
  if (missingRefs.length) {
    console.error(
      `\nFAIL refs: ${missingRefs.length} static key(s) missing from ${DEFAULT_LOCALE}`,
    );
    printList(
      missingRefs.map((k) => `${k}  (${(refs.get(k) || []).join(', ')})`),
    );
  } else {
    console.log(
      `locale ref check passed (${refs.size} static ids, default ${DEFAULT_LOCALE})`,
    );
  }

  if (keySetFailed || missingRefs.length) {
    process.exit(1);
  }
  console.log('\nlocale check passed');
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main();
}
