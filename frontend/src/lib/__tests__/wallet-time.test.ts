import { parseWalletTimestampToUnix } from '../wallet-time'

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

  it('returns undefined for unsupported browser-dependent formats', () => {
    expect(parseWalletTimestampToUnix('2025/09/03 13:26:11')).toBeUndefined()
    expect(parseWalletTimestampToUnix('not-a-date')).toBeUndefined()
  })
})
