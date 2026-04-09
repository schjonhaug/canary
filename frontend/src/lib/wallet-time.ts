import { formatDistance, Locale } from 'date-fns'
import { enUS, nb, es, ptBR, de, fr, ja, da, sv } from 'date-fns/locale'

const localeMap: Record<string, Locale> = {
  'en-US': enUS,
  nb: nb,
  'es-419': es,
  'pt-BR': ptBR,
  'de-DE': de,
  'fr-FR': fr,
  ja: ja,
  da: da,
  sv: sv,
}

/**
 * Parse a wallet timestamp string to Unix timestamp (seconds).
 * Handles SQLite "YYYY-MM-DD HH:MM:SS.mmm" and ISO timestamps.
 * Timezone-less SQLite/ISO values are treated as UTC, matching backend storage.
 * Keep SQLite handling aligned with formatDateTime in utils.ts.
 */
export function parseWalletTimestampToUnix(dateStr: string): number | undefined {
  const sqliteMatch = dateStr.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const sqliteZonedMatch = dateStr.match(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/)
  const isoNoTzMatch = dateStr.match(/^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const isoZonedMatch = dateStr.match(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/)

  if (!sqliteMatch && !sqliteZonedMatch && !isoNoTzMatch && !isoZonedMatch) {
    return undefined
  }

  let date: Date
  if (sqliteMatch) {
    date = new Date(`${sqliteMatch[1]}T${sqliteMatch[2]}Z`)
  } else if (sqliteZonedMatch) {
    date = new Date(dateStr.replace(' ', 'T'))
  } else if (isoNoTzMatch) {
    date = new Date(`${isoNoTzMatch[1]}T${isoNoTzMatch[2]}Z`)
  } else {
    date = new Date(dateStr)
  }
  const ts = date.getTime()
  return isNaN(ts) ? undefined : Math.floor(ts / 1000)
}

export function formatRelativeTime(timestamp: number, locale: string, now = Date.now()) {
  const dateFnsLocale = localeMap[locale] || enUS
  return formatDistance(new Date(timestamp * 1000), new Date(now), {
    addSuffix: true,
    locale: dateFnsLocale
  })
}
