import { useEffect, useState } from 'react'
import { getExampleNumber, CountryCode, isSupportedCountry } from 'libphonenumber-js'
import examples from 'libphonenumber-js/mobile/examples'

const DEFAULT_PLACEHOLDER = '+1 234 567 8900'

// Map language codes to country codes where they differ
// For most single-country languages (is, fi, pl, etc.), the language code
// matches the country code and is handled automatically via isSupportedCountry()
const LANGUAGE_TO_COUNTRY: Record<string, string> = {
  // Nordic
  'nb': 'NO', 'nn': 'NO', 'no': 'NO', // Norwegian variants
  'da': 'DK', // Danish
  'sv': 'SE', // Swedish
  // Asian
  'ja': 'JP', // Japanese
  'ko': 'KR', // Korean
  'zh': 'CN', // Chinese (default to mainland)
  'fil': 'PH', 'tl': 'PH', // Filipino/Tagalog
  'ms': 'MY', // Malay
  'vi': 'VN', // Vietnamese
  'th': 'TH', // Thai
  // European
  'cs': 'CZ', // Czech
  'el': 'GR', // Greek
  'et': 'EE', // Estonian
  'sl': 'SI', // Slovenian
  'uk': 'UA', // Ukrainian
  'be': 'BY', // Belarusian
  'sq': 'AL', // Albanian
  'sr': 'RS', // Serbian
  'bs': 'BA', // Bosnian
  'mk': 'MK', // Macedonian
  'ca': 'ES', // Catalan
  'eu': 'ES', // Basque
  'gl': 'ES', // Galician
  // Middle East
  'he': 'IL', // Hebrew
  'ar': 'SA', // Arabic (default to Saudi)
  'fa': 'IR', // Persian
  // South Asian
  'hi': 'IN', // Hindi
  'bn': 'BD', // Bengali
  'ta': 'IN', // Tamil
  'te': 'IN', // Telugu
  'ur': 'PK', // Urdu
  'ne': 'NP', // Nepali
  'si': 'LK', // Sinhala
  // African
  'sw': 'KE', // Swahili
  'am': 'ET', // Amharic
  'zu': 'ZA', // Zulu
  'af': 'ZA', // Afrikaans
}

/**
 * Try to get an example phone number for a country code.
 * Returns the formatted international number or null if not found.
 */
function tryGetExample(countryCode: string): string | null {
  const upper = countryCode.toUpperCase()
  if (!isSupportedCountry(upper)) return null

  try {
    const example = getExampleNumber(upper as CountryCode, examples)
    return example?.formatInternational() ?? null
  } catch {
    return null
  }
}

/**
 * Hook that returns a localized phone number placeholder based on the user's browser locale.
 * Uses libphonenumber-js to generate realistic example numbers for each country.
 */
export function usePhonePlaceholder(): string {
  const [placeholder, setPlaceholder] = useState(DEFAULT_PLACEHOLDER)

  useEffect(() => {
    const browserLocale = navigator.language
    const parts = browserLocale.split('-')
    const languageCode = parts[0]?.toLowerCase()
    const regionCode = parts[1]

    // Priority 1: Use explicit region code if present (e.g., "es-MX" -> "MX")
    if (regionCode) {
      const example = tryGetExample(regionCode)
      if (example) {
        setPlaceholder(example)
        return
      }
    }

    // Priority 2: Check manual mapping for languages where code != country
    const mappedCountry = LANGUAGE_TO_COUNTRY[languageCode]
    if (mappedCountry) {
      const example = tryGetExample(mappedCountry)
      if (example) {
        setPlaceholder(example)
        return
      }
    }

    // Priority 3: Try language code as country code (works for is/IS, fi/FI, pl/PL, etc.)
    const example = tryGetExample(languageCode)
    if (example) {
      setPlaceholder(example)
      return
    }

    // Keep default placeholder
  }, [])

  return placeholder
}
