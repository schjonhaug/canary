export const CSP_NONCE_HEADER = "x-nonce"

export function createContentSecurityPolicyNonce(): string {
  const randomBytes = crypto.getRandomValues(new Uint8Array(16))
  return btoa(String.fromCharCode(...randomBytes))
}

function configuredApiOrigin(): string | null {
  const apiUrl = process.env.NEXT_PUBLIC_API_URL?.trim()
  if (!apiUrl) return null

  try {
    const parsed = new URL(apiUrl)
    return parsed.protocol === "http:" || parsed.protocol === "https:"
      ? parsed.origin
      : null
  } catch {
    return null
  }
}

export function createContentSecurityPolicy(nonce: string): string {
  const isDevelopment = process.env.NODE_ENV === "development"
  const apiOrigin = configuredApiOrigin()
  const shouldUpgradeInsecureRequests = !isDevelopment
    && process.env.NEXT_PUBLIC_CANARY_MODE !== "self-hosted"

  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${isDevelopment ? " 'unsafe-eval'" : ""}`,
    `style-src 'self' ${isDevelopment ? "'unsafe-inline'" : `'nonce-${nonce}'`}`,
    "style-src-attr 'unsafe-inline'",
    `connect-src 'self'${apiOrigin ? ` ${apiOrigin}` : ""}`,
    "img-src 'self' blob: data:",
    "font-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
    ...(shouldUpgradeInsecureRequests ? ["upgrade-insecure-requests"] : []),
  ].join("; ")
}
