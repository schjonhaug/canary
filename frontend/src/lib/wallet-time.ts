/**
 * Parse a wallet timestamp string to Unix timestamp (seconds).
 * Handles SQLite "YYYY-MM-DD HH:MM:SS.mmm" and ISO timestamps.
 * Timezone-less SQLite/ISO values are treated as UTC, matching backend storage.
 * Keep SQLite handling aligned with formatDateTime in utils.ts.
 */
export function parseWalletTimestampToUnix(dateStr: string): number | undefined {
  const sqliteMatch = dateStr.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const isoUtcMatch = dateStr.match(/^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const isoZonedMatch = dateStr.match(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/)

  if (!sqliteMatch && !isoUtcMatch && !isoZonedMatch) {
    return undefined
  }

  let date: Date
  if (sqliteMatch) {
    date = new Date(`${sqliteMatch[1]}T${sqliteMatch[2]}Z`)
  } else if (isoUtcMatch) {
    date = new Date(`${isoUtcMatch[1]}T${isoUtcMatch[2]}Z`)
  } else {
    date = new Date(dateStr)
  }
  const ts = date.getTime()
  return isNaN(ts) ? undefined : Math.floor(ts / 1000)
}
