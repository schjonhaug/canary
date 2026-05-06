/**
 * @jest-environment node
 */
import { NextRequest } from 'next/server'
import { proxy } from './proxy'

jest.mock('@formatjs/intl-localematcher', () => ({
  match: jest.fn((languages: string[]) => (languages.some((language) => language.startsWith('nb')) ? 'nb' : 'en-US')),
}))

const originalCanaryMode = process.env.NEXT_PUBLIC_CANARY_MODE

function base64UrlEncode(value: unknown): string {
  return Buffer.from(JSON.stringify(value))
    .toString('base64')
    .replace(/=/g, '')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
}

function makeToken(payload: unknown): string {
  return `${base64UrlEncode({ alg: 'none', typ: 'JWT' })}.${base64UrlEncode(payload)}.signature`
}

function makeRequest(path: string, cookie?: string, headers: Record<string, string> = {}) {
  return new NextRequest(`http://localhost:3001${path}`, {
    headers: {
      ...headers,
      ...(cookie ? { cookie } : {}),
    },
  })
}

function expectRedirectToSignIn(response: Response) {
  expect(response.status).toBe(307)
  expect(response.headers.get('location')).toBe('http://localhost:3001/sign-in')
}

describe('proxy self-hosted auth recovery', () => {
  beforeEach(() => {
    process.env.NEXT_PUBLIC_CANARY_MODE = 'self-hosted'
    jest.spyOn(Date, 'now').mockReturnValue(1_700_000_000_000)
  })

  afterEach(() => {
    process.env.NEXT_PUBLIC_CANARY_MODE = originalCanaryMode
    jest.restoreAllMocks()
  })

  it('redirects page requests without an auth token to sign-in', () => {
    const response = proxy(makeRequest('/wallets'))

    expectRedirectToSignIn(response)
    expect(response.headers.get('set-cookie')).toContain('locale=en-US')
  })

  it('redirects page requests with an expired auth token and clears it', () => {
    const response = proxy(makeRequest('/wallets', `auth_token=${makeToken({ exp: 1_699_999_999 })}`))

    expectRedirectToSignIn(response)
    expect(response.headers.get('set-cookie')).toContain('auth_token=')
  })

  it('redirects page requests with a malformed auth token and clears it', () => {
    const response = proxy(makeRequest('/wallets', 'auth_token=not-a-jwt'))

    expectRedirectToSignIn(response)
    expect(response.headers.get('set-cookie')).toContain('auth_token=')
  })

  it('allows page requests with an unexpired well-formed auth token', () => {
    const response = proxy(makeRequest('/wallets', `auth_token=${makeToken({ exp: 1_700_000_001 })}; locale=nb`))

    expect(response.status).toBe(200)
    expect(response.headers.get('location')).toBeNull()
    expect(response.headers.get('set-cookie')).toBeNull()
  })

  it('exempts sign-in and api paths from auth checks', () => {
    const signInResponse = proxy(makeRequest('/sign-in', 'locale=nb'))
    const apiResponse = proxy(makeRequest('/api/wallets', 'locale=nb'))

    expect(signInResponse.status).toBe(200)
    expect(signInResponse.headers.get('location')).toBeNull()
    expect(apiResponse.status).toBe(200)
    expect(apiResponse.headers.get('location')).toBeNull()
  })
})

describe('proxy cloud mode locale behavior', () => {
  beforeEach(() => {
    process.env.NEXT_PUBLIC_CANARY_MODE = 'cloud'
  })

  afterEach(() => {
    process.env.NEXT_PUBLIC_CANARY_MODE = originalCanaryMode
  })

  it('does not require auth and keeps locale-only behavior', () => {
    const response = proxy(makeRequest('/wallets', undefined, { 'accept-language': 'nb-NO,nb;q=0.9' }))

    expect(response.status).toBe(200)
    expect(response.headers.get('location')).toBeNull()
    expect(response.headers.get('set-cookie')).toContain('locale=nb')
  })
})
