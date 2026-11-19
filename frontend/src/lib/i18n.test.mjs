import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const i18nPath = path.resolve(__dirname, 'i18n.tsx')

function extractLocaleMessages(filePath) {
  const sourceText = fs.readFileSync(filePath, 'utf8')
  const sf = ts.createSourceFile(filePath, sourceText, ts.ScriptTarget.Latest, true)

  let messagesObj = null
  function visit(node) {
    if (
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.name.text === 'messages' &&
      node.initializer &&
      ts.isObjectLiteralExpression(node.initializer)
    ) {
      messagesObj = node.initializer
      return
    }
    ts.forEachChild(node, visit)
  }
  visit(sf)
  assert.ok(messagesObj, 'Could not locate `messages` object in i18n.tsx')

  const result = {}
  const duplicates = {}

  for (const prop of messagesObj.properties) {
    if (!ts.isPropertyAssignment(prop)) continue
    const locale = ts.isIdentifier(prop.name)
      ? prop.name.text
      : ts.isStringLiteral(prop.name)
        ? prop.name.text
        : null
    if (!locale) continue
    assert.ok(
      ts.isObjectLiteralExpression(prop.initializer),
      `Locale "${locale}" must map to an object literal`,
    )
    const map = new Map()
    const dups = []
    for (const entry of prop.initializer.properties) {
      if (!ts.isPropertyAssignment(entry)) continue
      const key = ts.isStringLiteral(entry.name)
        ? entry.name.text
        : ts.isIdentifier(entry.name)
          ? entry.name.text
          : null
      assert.ok(key, `Non-literal property key in locale "${locale}"`)
      if (map.has(key)) dups.push(key)
      assert.ok(
        ts.isStringLiteral(entry.initializer) || ts.isNoSubstitutionTemplateLiteral(entry.initializer),
        `Value for "${locale}.${key}" must be a string literal`,
      )
      map.set(key, entry.initializer.text)
    }
    result[locale] = map
    duplicates[locale] = dups
  }

  return { result, duplicates }
}

function extractPlaceholders(str) {
  const matches = str.match(/\{\{[a-zA-Z0-9_]+\}\}|\{[a-zA-Z0-9_]+\}/g) || []
  return [...new Set(matches)].sort()
}

test('i18n zh and en locales have identical keys, no duplicates, and matching placeholders', () => {
  const { result, duplicates } = extractLocaleMessages(i18nPath)
  assert.ok(result.zh instanceof Map && result.en instanceof Map, 'zh and en locales must exist')

  assert.deepEqual(duplicates.zh, [], `Duplicate zh keys: ${duplicates.zh.join(', ')}`)
  assert.deepEqual(duplicates.en, [], `Duplicate en keys: ${duplicates.en.join(', ')}`)

  const zhKeys = new Set(result.zh.keys())
  const enKeys = new Set(result.en.keys())

  const missingInEn = [...zhKeys].filter((k) => !enKeys.has(k))
  const missingInZh = [...enKeys].filter((k) => !zhKeys.has(k))

  assert.deepEqual(missingInEn, [], `Keys in zh missing from en: ${missingInEn.join(', ')}`)
  assert.deepEqual(missingInZh, [], `Keys in en missing from zh: ${missingInZh.join(', ')}`)

  for (const key of zhKeys) {
    const zhVal = result.zh.get(key)
    const enVal = result.en.get(key)
    assert.ok(zhVal.trim().length > 0, `Empty zh translation for "${key}"`)
    assert.ok(enVal.trim().length > 0, `Empty en translation for "${key}"`)
    assert.deepEqual(
      extractPlaceholders(zhVal),
      extractPlaceholders(enVal),
      `Placeholder mismatch for "${key}": zh=${zhVal} vs en=${enVal}`,
    )
  }
})
