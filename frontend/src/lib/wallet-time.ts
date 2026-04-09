/**
 * Parse a SQLite UTC timestamp string to Unix timestamp (seconds).
 * Handles SQLite "YYYY-MM-DD HH:MM:SS.mmm" and ISO timestamps.
 * Timezone-less SQLite/ISO values are treated as UTC, matching backend storage.
 */
export function parseWalletTimestampToUnix(dateStr: string): number | undefined {
  const sqliteMatch = dateStr.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const isoUtcMatch = dateStr.match(/^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const isoZonedMatch = dateStr.match(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/)

  if (!sqliteMatch && !isoUtcMatch && !isoZonedMatch) {
    return undefined
  }

  const date = sqliteMatch
    ? new Date(`${sqliteMatch[1]}T${sqliteMatch[2]}Z`)
    : isoUtcMatch
      ? new Date(`${isoUtcMatch[1]}T${isoUtcMatch[2]}Z`)
    : new Date(dateStr)
  const ts = date.getTime()
  return isNaN(ts) ? undefined : Math.floor(ts / 1000)
}
