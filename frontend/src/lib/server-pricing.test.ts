import { fetchPricingOnServer } from './server-pricing'

const originalFetch = global.fetch
const originalApiUrl = process.env.API_URL

describe('fetchPricingOnServer', () => {
  afterEach(() => {
    global.fetch = originalFetch
    process.env.API_URL = originalApiUrl
  })

  it('fetches the pricing from the backend', async () => {
    process.env.API_URL = 'http://backend:3000/'
    const pricing = { tiers: [{ tier: 'personal', name: 'Personal', features: {} }] }
    const fetchMock = jest.fn(async () => ({ ok: true, json: async () => pricing }))
    global.fetch = fetchMock as unknown as typeof fetch

    await expect(fetchPricingOnServer()).resolves.toEqual(pricing)
    expect(fetchMock).toHaveBeenCalledWith('http://backend:3000/api/billing/pricing', expect.any(Object))
  })

  it('returns null when the backend fails or answers without tiers', async () => {
    global.fetch = jest.fn(async () => ({ ok: false, json: async () => ({}) })) as unknown as typeof fetch
    await expect(fetchPricingOnServer()).resolves.toBeNull()

    global.fetch = jest.fn(async () => ({ ok: true, json: async () => ({ error: 'x' }) })) as unknown as typeof fetch
    await expect(fetchPricingOnServer()).resolves.toBeNull()

    global.fetch = jest.fn(async () => { throw new Error('down') }) as unknown as typeof fetch
    await expect(fetchPricingOnServer()).resolves.toBeNull()
  })
})
