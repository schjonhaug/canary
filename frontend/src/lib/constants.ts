/**
 * Centralized application constants and validation patterns
 *
 * This file contains shared constants, validation regex patterns, and
 * constraints used across the frontend application.
 */

// =============================================================================
// Application Constants
// =============================================================================

export const SUPPORT_EMAIL = process.env.NEXT_PUBLIC_SUPPORT_EMAIL || 'support@canarybitcoin.com'

// =============================================================================
// Validation Regex Patterns
// =============================================================================

/**
 * Extended Public Key (XPUB) validation pattern
 *
 * Validates Bitcoin extended public keys in various formats:
 * - xpub: Standard mainnet
 * - ypub: BIP49 (P2WPKH-nested-in-P2SH) mainnet
 * - zpub: BIP84 (Native SegWit) mainnet
 * - tpub: Standard testnet/regtest
 * - upub: BIP49 testnet/regtest
 * - vpub: BIP84 testnet/regtest
 *
 * Uses Base58Check encoding (excludes 0, O, I, l characters)
 * Total length: 111-112 characters (4 prefix + 107-108 data)
 */
export const XPUB_REGEX = /^[xyztuv]pub[1-9A-HJ-NP-Za-km-z]{107,108}$/

/**
 * Output descriptor format validation pattern
 *
 * Validates Bitcoin output descriptor function prefixes:
 * - wpkh: Native SegWit (P2WPKH)
 * - wsh: Witness Script Hash (P2WSH)
 * - sh: Script Hash (P2SH)
 * - pkh: Public Key Hash (P2PKH)
 * - tr: Taproot (P2TR)
 */
export const DESCRIPTOR_REGEX = /^(wpkh|wsh|sh|pkh|tr)\(/

/**
 * Email address validation pattern
 *
 * Basic email format validation:
 * - Non-whitespace, non-@ characters before @
 * - Non-whitespace, non-@ characters for domain
 * - Non-whitespace, non-@ characters for TLD
 */
export const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

// =============================================================================
// Validation Helper Functions
// =============================================================================

/**
 * Check if input is a valid extended public key format
 */
export function isValidXpub(input: string): boolean {
  return XPUB_REGEX.test(input.trim())
}

/**
 * Check if input is a valid output descriptor format
 */
export function isValidDescriptor(input: string): boolean {
  return DESCRIPTOR_REGEX.test(input.trim())
}

/**
 * Check if input is a valid email address format
 */
export function isValidEmail(input: string): boolean {
  return EMAIL_REGEX.test(input.trim())
}

// =============================================================================
// Validation Constraints
// =============================================================================

/**
 * Email validation constraints
 */
export const EMAIL_CONSTRAINTS = {
  MAX_LENGTH: 255,
} as const

/**
 * Message validation constraints (for contact form)
 */
export const MESSAGE_CONSTRAINTS = {
  MIN_LENGTH: 10,
  MAX_LENGTH: 5000,
} as const
