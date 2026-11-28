import { fetchServerPricing } from '../server-pricing'

const originalFetch = global.fetch
const originalApiUrl = process.env.API_URL

describe('fetchServerPricing', () => {
  afterEach(() => {
    global.fetch = originalFetch
    process.env.API_URL = originalApiUrl
  })

  it('fetches pricing from the internal API URL', async () => {
    process.env.API_URL = 'http://backend:3000'
    const pricing = { tiers: [{ tier: 'personal' }] }
    const fetchMock = jest.fn().mockResolvedValue({ ok: true, json: async () => pricing })
    global.fetch = fetchMock as unknown as typeof fetch

    await expect(fetchServerPricing()).resolves.toEqual(pricing)
    expect(fetchMock).toHaveBeenCalledWith('http://backend:3000/api/billing/pricing', expect.any(Object))
  })

  it('returns null when the backend fails, so the page still renders', async () => {
    global.fetch = jest.fn().mockResolvedValue({ ok: false }) as unknown as typeof fetch
    await expect(fetchServerPricing()).resolves.toBeNull()

    global.fetch = jest.fn().mockRejectedValue(new Error('down')) as unknown as typeof fetch
    await expect(fetchServerPricing()).resolves.toBeNull()

    global.fetch = jest.fn().mockResolvedValue({ ok: true, json: async () => ({}) }) as unknown as typeof fetch
    await expect(fetchServerPricing()).resolves.toBeNull()
  })
})
