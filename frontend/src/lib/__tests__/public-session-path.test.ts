import { isPublicSessionPath } from '../public-session-path'

describe('isPublicSessionPath', () => {
  it.each([
    '/',
    '/cloud',
    '/private',
    '/sign-in',
    '/sign-up',
    '/sign-up/success',
    '/forgot-password',
    '/contact',
    '/donations',
    '/donations/thank-you',
    '/demo',
    '/reset-password/abc',
    '/verify-email/token',
  ])('treats %s as a public session path', (pathname) => {
    expect(isPublicSessionPath(pathname)).toBe(true)
  })

  it.each(['/wallets', '/settings', '/subscription', '/support', '/wallets/add'])(
    'requires a sign-in redirect from %s after session expiry',
    (pathname) => {
      expect(isPublicSessionPath(pathname)).toBe(false)
    },
  )
})
