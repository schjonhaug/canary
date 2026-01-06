import { type Locale, locales, defaultLocale } from '@/i18n/config'

const LOCALE_COOKIE_NAME = 'locale'

export function getStoredLocale(): Locale {
  if (typeof document === 'undefined') return defaultLocale

  const cookies = document.cookie.split(';')
  for (const cookie of cookies) {
    const [name, value] = cookie.trim().split('=')
    if (name === LOCALE_COOKIE_NAME) {
      const locale = value as Locale
      if (locales.includes(locale)) {
        return locale
      }
    }
  }
  return defaultLocale
}

export function setStoredLocale(locale: Locale): void {
  if (typeof document === 'undefined') return

  // Set cookie with 1 year expiry
  const maxAge = 60 * 60 * 24 * 365
  document.cookie = `${LOCALE_COOKIE_NAME}=${locale}; path=/; max-age=${maxAge}; samesite=lax`
}

export function clearStoredLocale(): void {
  if (typeof document === 'undefined') return

  // Clear cookie by setting max-age to 0
  document.cookie = `${LOCALE_COOKIE_NAME}=; path=/; max-age=0; samesite=lax`
}
