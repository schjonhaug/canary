import { formatRelativeTime, parseWalletTimestampToUnix } from '../wallet-time'

describe('parseWalletTimestampToUnix', () => {
  it('parses SQLite UTC timestamps with milliseconds', () => {
    expect(parseWalletTimestampToUnix('2025-09-03 13:26:11.944')).toBe(
      Math.floor(Date.UTC(2025, 8, 3, 13, 26, 11, 944) / 1000)
    )
  })

  it('parses SQLite UTC timestamps without milliseconds', () => {
    expect(parseWalletTimestampToUnix('2026-02-02 11:26:43')).toBe(
      Math.floor(Date.UTC(2026, 1, 2, 11, 26, 43) / 1000)
    )
  })

  it('parses ISO timestamps with explicit timezone offsets', () => {
    expect(parseWalletTimestampToUnix('2025-09-03T13:26:11+02:00')).toBe(
      Math.floor(Date.UTC(2025, 8, 3, 11, 26, 11) / 1000)
    )
  })

  it('parses ISO timestamps with Z suffixes', () => {
    expect(parseWalletTimestampToUnix('2025-09-03T13:26:11Z')).toBe(
      Math.floor(Date.UTC(2025, 8, 3, 13, 26, 11) / 1000)
    )
  })

  it('parses ISO timestamps with milliseconds and explicit timezone offsets', () => {
    expect(parseWalletTimestampToUnix('2025-09-03T13:26:11.944+02:00')).toBe(
      Math.floor(Date.UTC(2025, 8, 3, 11, 26, 11, 944) / 1000)
    )
  })

  it('parses SQLite-like timestamps with explicit timezone offsets', () => {
    expect(parseWalletTimestampToUnix('2025-09-03 13:26:11+02:00')).toBe(
      Math.floor(Date.UTC(2025, 8, 3, 11, 26, 11) / 1000)
    )
  })

  it('parses SQLite-like timestamps with milliseconds and explicit timezone offsets', () => {
    expect(parseWalletTimestampToUnix('2025-09-03 13:26:11.944+02:00')).toBe(
      Math.floor(Date.UTC(2025, 8, 3, 11, 26, 11, 944) / 1000)
    )
  })

  it('parses timezone-less ISO timestamps as UTC', () => {
    expect(parseWalletTimestampToUnix('2025-09-03T13:26:11')).toBe(
      Math.floor(Date.UTC(2025, 8, 3, 13, 26, 11) / 1000)
    )
  })

  it('returns undefined for unsupported browser-dependent formats', () => {
    expect(parseWalletTimestampToUnix('2025/09/03 13:26:11')).toBeUndefined()
    expect(parseWalletTimestampToUnix('not-a-date')).toBeUndefined()
  })
})

describe('formatRelativeTime', () => {
  it('uses the provided now value for deterministic relative labels', () => {
    const timestamp = Math.floor(Date.UTC(2026, 0, 1, 11, 59, 0) / 1000)
    const now = Date.UTC(2026, 0, 1, 12, 0, 0)

    expect(formatRelativeTime(timestamp, 'en-US', now)).toBe('1 minute ago')
  })

  it('falls back to English for unsupported locale strings', () => {
    const timestamp = Math.floor(Date.UTC(2026, 0, 1, 11, 59, 0) / 1000)
    const now = Date.UTC(2026, 0, 1, 12, 0, 0)

    expect(formatRelativeTime(timestamp, 'unsupported', now)).toBe('1 minute ago')
  })
})
