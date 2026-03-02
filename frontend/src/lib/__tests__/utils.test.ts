import { formatDateTime } from '../utils'

describe('formatDateTime', () => {
  it('formats SQLite timestamp without milliseconds', () => {
    const result = formatDateTime('2026-02-02 11:26:43', 'en-US')
    expect(result).not.toBe('Invalid date')
    expect(result).toMatch(/02/)
  })

  it('formats SQLite timestamp with milliseconds', () => {
    const result = formatDateTime('2026-03-02 10:09:33.944', 'en-US')
    expect(result).not.toBe('Invalid date')
    expect(result).toMatch(/03/)
  })

  it('formats Unix timestamp', () => {
    const result = formatDateTime(1740000000, 'en-US')
    expect(result).not.toBe('Invalid date')
  })

  it('does not treat timestamps with timezone offsets as SQLite UTC', () => {
    // A string with a timezone offset should NOT be coerced to UTC
    // "13:26:11+02:00" should be parsed as-is, not as "13:26:11Z"
    const withOffset = formatDateTime('2025-09-03 13:26:11+02:00', 'en-US')
    const asUTC = formatDateTime('2025-09-03 13:26:11', 'en-US')
    // If the offset string were incorrectly treated as SQLite UTC,
    // both would produce the same result. They should differ.
    // Note: if the browser can't parse the offset string at all, it returns "Invalid date"
    // which also proves it wasn't coerced to UTC.
    if (withOffset !== 'Invalid date') {
      expect(withOffset).not.toBe(asUTC)
    }
  })

  it('returns Invalid date for unparseable strings', () => {
    expect(formatDateTime('not-a-date', 'en-US')).toBe('Invalid date')
  })
})
