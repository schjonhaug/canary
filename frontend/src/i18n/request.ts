import { getRequestConfig } from 'next-intl/server'
import { cookies, headers } from 'next/headers'
import Negotiator from 'negotiator'
import { match } from '@formatjs/intl-localematcher'
import { defaultLocale, locales, type Locale } from './config'

// Extended locales including Norwegian variants for matching
const matcherLocales = ['en', 'en-US', 'nb', 'nn', 'no', 'es', 'es-419', 'pt', 'pt-BR', 'de', 'de-DE', 'fr', 'fr-FR', 'ja', 'da', 'sv']

function detectLocale(acceptLanguage: string | null): Locale {
  if (!acceptLanguage) return defaultLocale

  try {
    const negotiator = new Negotiator({ headers: { 'accept-language': acceptLanguage } })
    const languages = negotiator.languages()
    const matched = match(languages, matcherLocales, 'en')

    // Map browser locales to our supported locales
    if (matched === 'nb' || matched === 'nn' || matched === 'no') return 'nb'
    if (matched === 'es' || matched === 'es-419') return 'es-419'
    if (matched === 'pt' || matched === 'pt-BR') return 'pt-BR'
    if (matched === 'de' || matched === 'de-DE') return 'de-DE'
    if (matched === 'fr' || matched === 'fr-FR') return 'fr-FR'
    if (matched === 'en' || matched === 'en-US') return 'en-US'

    if (locales.includes(matched as Locale)) return matched as Locale
    return defaultLocale
  } catch {
    return defaultLocale
  }
}

export default getRequestConfig(async () => {
  const headerStore = await headers()
  const cookieStore = await cookies()

  // Check for locale override from query param (set by middleware)
  const localeOverride = headerStore.get('x-locale-override') as Locale | null

  let locale: Locale
  if (localeOverride && locales.includes(localeOverride)) {
    locale = localeOverride
  } else {
    const cookieLocale = cookieStore.get('locale')?.value as Locale | undefined
    if (cookieLocale && locales.includes(cookieLocale)) {
      locale = cookieLocale
    } else {
      // No cookie yet - detect from Accept-Language header (first visit)
      const acceptLanguage = headerStore.get('accept-language')
      locale = detectLocale(acceptLanguage)
    }
  }

  return {
    locale,
    messages: (await import(`../../messages/${locale}.json`)).default,
  }
})
