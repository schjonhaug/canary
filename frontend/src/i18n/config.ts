export const locales = ['en', 'no', 'es', 'pt', 'de', 'fr', 'ja'] as const
export type Locale = (typeof locales)[number]
export const defaultLocale: Locale = 'en'

export const localeNames: Record<Locale, string> = {
  en: 'English',
  no: 'Norsk',
  es: 'Español',
  pt: 'Português',
  de: 'Deutsch',
  fr: 'Français',
  ja: '日本語',
}

// Map browser locale codes to our supported locales
export function mapBrowserLocale(browserLocale: string): Locale {
  const locale = browserLocale.toLowerCase().split('-')[0]

  switch (locale) {
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
