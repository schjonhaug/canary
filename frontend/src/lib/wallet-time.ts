/**
 * Parse a SQLite UTC timestamp string to Unix timestamp (seconds).
 * Handles "YYYY-MM-DD HH:MM:SS.mmm" format (UTC without timezone indicator).
 */
export function parseWalletTimestampToUnix(dateStr: string): number | undefined {
  const sqliteMatch = dateStr.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const isoMatch = dateStr.match(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$/)

  if (!sqliteMatch && !isoMatch) {
    return undefined
  }

  const date = sqliteMatch
    ? new Date(`${sqliteMatch[1]}T${sqliteMatch[2]}Z`)
    : new Date(dateStr)
  const ts = date.getTime()
  return isNaN(ts) ? undefined : Math.floor(ts / 1000)
}
