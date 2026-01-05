import { NextResponse } from 'next/server'
import type { NextRequest } from 'next/server'
import { locales, defaultLocale, type Locale } from './i18n/config'

function mapBrowserLocale(acceptLanguage: string | null): Locale {
  if (!acceptLanguage) return defaultLocale

  // Get the preferred language from Accept-Language header
  const preferred = acceptLanguage.split(',')[0].split('-')[0].toLowerCase()

  switch (preferred) {
    case 'nb':
    case 'nn':
    case 'no':
      return 'no'
    case 'es':
      return 'es'
    case 'pt':
      return 'pt'
    case 'de':
      return 'de'
    case 'fr':
      return 'fr'
    case 'ja':
      return 'ja'
    default:
      return 'en'
  }
}

export function middleware(request: NextRequest) {
  const localeCookie = request.cookies.get('locale')?.value

  // Already has a valid locale preference
  if (localeCookie && locales.includes(localeCookie as Locale)) {
    return NextResponse.next()
  }

  // Detect from Accept-Language header
  const acceptLanguage = request.headers.get('accept-language')
  const detectedLocale = mapBrowserLocale(acceptLanguage)

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
