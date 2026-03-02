import { formatDateTime, loadWalletSvg, getCachedWalletSvg, resetSvgCaches } from '../utils'

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

describe('loadWalletSvg / getCachedWalletSvg', () => {
  const canarySvg = '<svg width="691" height="595"><path fill="#F6C919"/><path fill="#73C2DE"/></svg>'
  const featherSvg = '<svg width="531" height="377"><path fill="#F6C919"/><path fill="#161812"/></svg>'

  beforeEach(() => {
    resetSvgCaches()
    global.fetch = jest.fn()
  })

  afterEach(() => {
    jest.restoreAllMocks()
  })

  it('fetches canary.svg for descriptor wallets', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => canarySvg })

    const result = await loadWalletSvg('#FF0000', 'descriptor')

    expect(global.fetch).toHaveBeenCalledWith('/images/canary.svg')
    expect(result).toContain('#FF0000')
    expect(result).not.toContain('#F6C919')
  })

  it('fetches feather.svg for address wallets', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => featherSvg })

    const result = await loadWalletSvg('#00FF00', 'address')

    expect(global.fetch).toHaveBeenCalledWith('/images/feather.svg')
    expect(result).toContain('#00FF00')
    expect(result).not.toContain('#F6C919')
  })

  it('resizes SVG to 24x24 regardless of original dimensions', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => canarySvg })

    const result = await loadWalletSvg('#FF0000', 'descriptor')

    expect(result).toContain('width="24"')
    expect(result).toContain('height="24"')
    expect(result).not.toContain('width="691"')
    expect(result).not.toContain('height="595"')
  })

  it('only resizes root svg element, not child elements', async () => {
    const svgWithChild = '<svg width="100" height="200"><rect width="50" height="50"/></svg>'
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => svgWithChild })

    const result = await loadWalletSvg('#FF0000', 'descriptor')

    expect(result).toMatch(/^<svg width="24" height="24"/)
    expect(result).toContain('width="50"')
    expect(result).toContain('height="50"')
  })

  it('makes blue transparent in canary SVG', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => canarySvg })

    const result = await loadWalletSvg('#FF0000', 'descriptor')

    expect(result).toContain('transparent')
    expect(result).not.toContain('#73C2DE')
  })

  it('uses separate cache keys for different wallet types with same color', async () => {
    ;(global.fetch as jest.Mock)
      .mockResolvedValueOnce({ ok: true, text: async () => canarySvg })
      .mockResolvedValueOnce({ ok: true, text: async () => featherSvg })

    const descriptorResult = await loadWalletSvg('#FF0000', 'descriptor')
    const addressResult = await loadWalletSvg('#FF0000', 'address')

    expect(descriptorResult).not.toBe(addressResult)
    expect(global.fetch).toHaveBeenCalledTimes(2)
  })

  it('returns cached result on second call', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => canarySvg })

    await loadWalletSvg('#FF0000', 'descriptor')
    const result = await loadWalletSvg('#FF0000', 'descriptor')

    expect(global.fetch).toHaveBeenCalledTimes(1)
    expect(result).toContain('#FF0000')
  })

  it('getCachedWalletSvg returns null before loading', () => {
    expect(getCachedWalletSvg('#FF0000', 'descriptor')).toBeNull()
  })

  it('getCachedWalletSvg returns SVG after loading', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => canarySvg })

    await loadWalletSvg('#FF0000', 'descriptor')

    expect(getCachedWalletSvg('#FF0000', 'descriptor')).toContain('#FF0000')
    expect(getCachedWalletSvg('#FF0000', 'address')).toBeNull()
  })

  it('returns fallback on fetch failure', async () => {
    ;(global.fetch as jest.Mock).mockRejectedValue(new Error('Network error'))

    const result = await loadWalletSvg('#FF0000', 'descriptor')

    expect(result).toContain('⚠️')
  })

  it('defaults to descriptor when walletType is omitted', async () => {
    ;(global.fetch as jest.Mock).mockResolvedValue({ ok: true, text: async () => canarySvg })

    await loadWalletSvg('#FF0000')

    expect(global.fetch).toHaveBeenCalledWith('/images/canary.svg')
  })
})
