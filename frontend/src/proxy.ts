import { NextResponse } from 'next/server'
import type { NextRequest } from 'next/server'
import Negotiator from 'negotiator'
import { match } from '@formatjs/intl-localematcher'
import { locales, defaultLocale, type Locale } from './i18n/config'
import {
  CSP_NONCE_HEADER,
  createContentSecurityPolicy,
  createContentSecurityPolicyNonce,
} from './lib/content-security-policy'

// Extended locales including Norwegian variants for matching
const matcherLocales = ['en', 'en-US', 'nb', 'nn', 'no', 'es', 'es-419', 'pt', 'pt-BR', 'de', 'de-DE', 'fr', 'fr-FR', 'ja', 'da', 'sv']
const selfHostedMode = 'self-hosted'

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

function addLocaleCookie(request: NextRequest, response: NextResponse): NextResponse {
  const localeCookie = request.cookies.get('locale')?.value

  // Already has a valid locale preference
  if (localeCookie && locales.includes(localeCookie as Locale)) {
    return response
  }

  // Detect from Accept-Language header
  const acceptLanguage = request.headers.get('accept-language')
  const detectedLocale = detectLocale(acceptLanguage)

  // Set the locale cookie for future requests
  response.cookies.set('locale', detectedLocale, {
    maxAge: 60 * 60 * 24 * 365, // 1 year
    path: '/',
    sameSite: 'lax',
  })

  return response
}

function isAuthExemptPath(pathname: string): boolean {
  return pathname === '/sign-in' || pathname.startsWith('/api/')
}

function decodeJwtPayload(token: string): unknown {
  const parts = token.split('.')

  if (parts.length !== 3 || !parts[1]) {
    throw new Error('Malformed JWT')
  }

  const base64 = parts[1].replace(/-/g, '+').replace(/_/g, '/')
  const paddedBase64 = base64.padEnd(base64.length + ((4 - (base64.length % 4)) % 4), '=')

  return JSON.parse(atob(paddedBase64))
}

function isExpiredAuthToken(token: string): boolean {
  const payload = decodeJwtPayload(token)

  if (!payload || typeof payload !== 'object' || !('exp' in payload)) {
    throw new Error('Missing JWT exp')
  }

  const exp = (payload as { exp: unknown }).exp

  if (typeof exp !== 'number' || !Number.isFinite(exp)) {
    throw new Error('Invalid JWT exp')
  }

  return exp <= Math.floor(Date.now() / 1000)
}

function redirectToSignIn(request: NextRequest, shouldClearAuthToken = false): NextResponse {
  const response = NextResponse.redirect(new URL('/sign-in', request.url))

  if (shouldClearAuthToken) {
    response.cookies.delete('auth_token')
  }

  return addLocaleCookie(request, response)
}

function addContentSecurityPolicy(response: NextResponse, contentSecurityPolicy: string): NextResponse {
  response.headers.set('Content-Security-Policy', contentSecurityPolicy)
  return response
}

function nextResponseWithContentSecurityPolicy(
  request: NextRequest,
  nonce: string,
  contentSecurityPolicy: string,
): NextResponse {
  const requestHeaders = new Headers(request.headers)
  requestHeaders.set(CSP_NONCE_HEADER, nonce)
  // Next.js parses the request CSP to apply this nonce to its generated scripts and styles.
  requestHeaders.set('Content-Security-Policy', contentSecurityPolicy)

  return NextResponse.next({
    request: {
      headers: requestHeaders,
    },
  })
}

export function proxy(request: NextRequest) {
  const nonce = createContentSecurityPolicyNonce()
  const contentSecurityPolicy = createContentSecurityPolicy(nonce)

  if (process.env.NEXT_PUBLIC_CANARY_MODE === selfHostedMode && !isAuthExemptPath(request.nextUrl.pathname)) {
    const authToken = request.cookies.get('auth_token')?.value

    if (!authToken) {
      return addContentSecurityPolicy(redirectToSignIn(request), contentSecurityPolicy)
    }

    try {
      if (isExpiredAuthToken(authToken)) {
        return addContentSecurityPolicy(redirectToSignIn(request, true), contentSecurityPolicy)
      }
    } catch {
      return addContentSecurityPolicy(redirectToSignIn(request, true), contentSecurityPolicy)
    }
  }

  const response = nextResponseWithContentSecurityPolicy(request, nonce, contentSecurityPolicy)
  return addContentSecurityPolicy(addLocaleCookie(request, response), contentSecurityPolicy)
}

export const config = {
  // Match all routes except static files and API routes
  matcher: ['/((?!api|_next/static|_next/image|favicon.ico|images|.*\\..*).*)'],
}
