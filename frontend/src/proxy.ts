import { NextResponse } from 'next/server'
import type { NextRequest } from 'next/server'
import Negotiator from 'negotiator'
import { match } from '@formatjs/intl-localematcher'
import { locales, defaultLocale, type Locale } from './i18n/config'

// Extended locales including Norwegian variants for matching
const matcherLocales = ['en', 'en-US', 'nb', 'nn', 'no', 'es', 'es-419', 'pt', 'pt-BR', 'de', 'de-DE', 'fr', 'fr-FR', 'ja', 'da', 'sv']

function detectLocale(acceptLanguage: string | null): Locale {
  if (!acceptLanguage) return defaultLocale

  try {
    // Use Negotiator to parse Accept-Language header
    const negotiator = new Negotiator({ headers: { 'accept-language': acceptLanguage } })
    const languages = negotiator.languages()

    // Use intl-localematcher to find best match
    const matched = match(languages, matcherLocales, 'en')

    // Map browser locales to our supported locales
    if (matched === 'nb' || matched === 'nn' || matched === 'no') return 'nb'
    if (matched === 'es' || matched === 'es-419') return 'es-419'
    if (matched === 'pt' || matched === 'pt-BR') return 'pt-BR'
    if (matched === 'de' || matched === 'de-DE') return 'de-DE'
    if (matched === 'fr' || matched === 'fr-FR') return 'fr-FR'
    if (matched === 'en' || matched === 'en-US') return 'en-US'

    // Verify it's one of our supported locales
    if (locales.includes(matched as Locale)) return matched as Locale

    return defaultLocale
  } catch {
    return defaultLocale
  }
}

export function proxy(request: NextRequest) {
  const localeCookie = request.cookies.get('locale')?.value

  // Already has a valid locale preference
  if (localeCookie && locales.includes(localeCookie as Locale)) {
    return NextResponse.next()
  }

  // Detect from Accept-Language header
  const acceptLanguage = request.headers.get('accept-language')
  const detectedLocale = detectLocale(acceptLanguage)

  // Set the locale cookie for future requests
  const response = NextResponse.next()
  response.cookies.set('locale', detectedLocale, {
    maxAge: 60 * 60 * 24 * 365, // 1 year
    path: '/',
    sameSite: 'lax',
  })

  return response
}

export const config = {
  // Match all routes except static files and API routes
  matcher: ['/((?!api|_next/static|_next/image|favicon.ico|images|.*\\..*).*)'],
}
