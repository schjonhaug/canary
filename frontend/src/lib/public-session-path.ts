const PUBLIC_SESSION_PATHS = new Set([
  '/',
  '/cloud',
  '/private',
  '/sign-in',
  '/sign-up',
  '/sign-up/success',
  '/forgot-password',
  '/contact',
  '/donations',
  '/demo',
])

const PUBLIC_SESSION_PREFIXES = [
  '/reset-password/',
  '/verify-email/',
  '/donations/',
]

export function isPublicSessionPath(pathname: string): boolean {
  const path = pathname.split('?')[0] || '/'

  return PUBLIC_SESSION_PATHS.has(path) || PUBLIC_SESSION_PREFIXES.some((prefix) => path.startsWith(prefix))
}
