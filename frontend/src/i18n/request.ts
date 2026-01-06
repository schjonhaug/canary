import { getRequestConfig } from 'next-intl/server'
import { cookies, headers } from 'next/headers'
import Negotiator from 'negotiator'
import { match } from '@formatjs/intl-localematcher'
import { defaultLocale, locales, type Locale } from './config'

// Extended locales including Norwegian variants for matching
const matcherLocales = ['en', 'no', 'nb', 'nn', 'es', 'pt', 'de', 'fr', 'ja', 'da', 'sv']

function detectLocale(acceptLanguage: string | null): Locale {
  if (!acceptLanguage) return defaultLocale

  try {
    const negotiator = new Negotiator({ headers: { 'accept-language': acceptLanguage } })
    const languages = negotiator.languages()
    const matched = match(languages, matcherLocales, defaultLocale)

    // Map Norwegian variants to 'no'
    if (matched === 'nb' || matched === 'nn') return 'no'

    if (locales.includes(matched as Locale)) return matched as Locale
    return defaultLocale
  } catch {
    return defaultLocale
  }
}

export default getRequestConfig(async () => {
  const cookieStore = await cookies()
  const cookieLocale = cookieStore.get('locale')?.value as Locale | undefined

  let locale: Locale
  if (cookieLocale && locales.includes(cookieLocale)) {
    locale = cookieLocale
  } else {
    // No cookie yet - detect from Accept-Language header (first visit)
    const headerStore = await headers()
    const acceptLanguage = headerStore.get('accept-language')
    locale = detectLocale(acceptLanguage)
  }

  return {
    locale,
    messages: (await import(`../../messages/${locale}.json`)).default,
  }
})
