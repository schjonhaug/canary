export const locales = ['en-US', 'nb', 'es-419', 'pt-BR', 'de-DE', 'fr-FR', 'ja', 'da', 'sv'] as const
export type Locale = (typeof locales)[number]
export const defaultLocale: Locale = 'en-US'

// Native language names for the language selector dropdown
// Each language is written in its own language so users can always find it
export const localeNames: Record<Locale, string> = {
  'en-US': 'English (US)',
  nb: 'Norsk (Bokmål)',
  'es-419': 'Español (Latinoamérica)',
  'pt-BR': 'Português (Brasil)',
  'de-DE': 'Deutsch',
  'fr-FR': 'Français',
  ja: '日本語',
  da: 'Dansk',
  sv: 'Svenska',
}

// Map browser locale codes to our supported locales
export function mapBrowserLocale(browserLocale: string): Locale {
  const locale = browserLocale.toLowerCase().split('-')[0]

  switch (locale) {
    case 'nb':
    case 'nn':
    case 'no':
      return 'nb'
    case 'es':
      return 'es-419'
    case 'pt':
      return 'pt-BR'
    case 'de':
      return 'de-DE'
    case 'fr':
      return 'fr-FR'
    case 'ja':
      return 'ja'
    case 'da':
      return 'da'
    case 'sv':
      return 'sv'
    default:
      return 'en-US'
  }
}
