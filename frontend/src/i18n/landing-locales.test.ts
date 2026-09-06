import fs from 'node:fs'
import path from 'node:path'
import { locales } from './config'

function leafPaths(value: unknown, prefix = ''): string[] {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    return [prefix]
  }

  return Object.entries(value).flatMap(([key, child]) => leafPaths(child, prefix ? `${prefix}.${key}` : key))
}

function readMessages(locale: string) {
  return JSON.parse(fs.readFileSync(path.join(process.cwd(), 'messages', `${locale}.json`), 'utf8'))
}

describe('public page locale parity', () => {
  const english = readMessages('en-US')

  it.each(['landing', 'cloudPage'] as const)('keeps the complete %s key shape in every locale', (namespace) => {
    const expectedPaths = leafPaths(english[namespace]).sort()

    for (const locale of locales) {
      const messages = readMessages(locale)
      expect(leafPaths(messages[namespace]).sort()).toEqual(expectedPaths)
    }
  })

  it('uses localized hero copy instead of English placeholders', () => {
    for (const locale of locales.filter((locale) => locale !== 'en-US')) {
      const messages = readMessages(locale)
      expect(messages.landing.hero.title).not.toBe(english.landing.hero.title)
      expect(messages.cloudPage.faq.title).not.toBe(english.cloudPage.faq.title)
    }
  })
})
