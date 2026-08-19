export const CSP_NONCE_HEADER = "x-nonce"

export function createContentSecurityPolicyNonce(): string {
  return Buffer.from(crypto.randomUUID()).toString("base64")
}

export function createContentSecurityPolicy(nonce: string): string {
  const isDevelopment = process.env.NODE_ENV === "development"
  const shouldUpgradeInsecureRequests = !isDevelopment
    && process.env.NEXT_PUBLIC_CANARY_MODE !== "self-hosted"

  return [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${isDevelopment ? " 'unsafe-eval'" : ""}`,
    `style-src 'self' ${isDevelopment ? "'unsafe-inline'" : `'nonce-${nonce}'`}`,
    "style-src-attr 'unsafe-inline'",
    "img-src 'self' blob: data:",
    "font-src 'self'",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
    "frame-ancestors 'none'",
    ...(shouldUpgradeInsecureRequests ? ["upgrade-insecure-requests"] : []),
  ].join("; ")
}
