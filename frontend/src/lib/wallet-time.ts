/**
 * Parse a SQLite UTC timestamp string to Unix timestamp (seconds).
 * Handles "YYYY-MM-DD HH:MM:SS.mmm" format (UTC without timezone indicator).
 */
export function parseWalletTimestampToUnix(dateStr: string): number | undefined {
  const match = dateStr.match(/^(\d{4}-\d{2}-\d{2}) (\d{2}:\d{2}:\d{2})(?:\.\d+)?$/)
  const date = match
    ? new Date(`${match[1]}T${match[2]}Z`)
    : new Date(dateStr)
  const ts = date.getTime()
  return isNaN(ts) ? undefined : Math.floor(ts / 1000)
}
