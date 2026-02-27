/**
 * Centralized application constants and validation patterns
 *
 * This file contains shared constants, validation regex patterns, and
 * constraints used across the frontend application.
 */

// =============================================================================
// Application Constants
// =============================================================================

export const SUPPORT_EMAIL = 'support@canarybitcoin.com'

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
/**
 * Bitcoin address pre-filter pattern (loose, for UI toggling)
 *
 * Matches common Bitcoin address formats:
 * - P2PKH: 1... (mainnet), m/n... (testnet/regtest)
 * - P2SH: 3... (mainnet), 2... (testnet/regtest)
 * - Bech32: bc1... (mainnet), tb1... (testnet), bcrt1... (regtest)
 *
 * This is intentionally permissive - the backend does authoritative validation
 * with proper checksum verification via bitcoin::Address::from_str().
 */
export const BITCOIN_ADDRESS_REGEX = /^(1[1-9A-HJ-NP-Za-km-z]{25,34}|3[1-9A-HJ-NP-Za-km-z]{25,34}|bc1[a-zA-HJ-NP-Z0-9]{25,87}|[mn2][1-9A-HJ-NP-Za-km-z]{25,34}|tb1[a-zA-HJ-NP-Z0-9]{25,87}|bcrt1[a-zA-HJ-NP-Z0-9]{25,87})$/i

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
 * Check if input looks like a Bitcoin address (loose pre-filter for UI)
 * Backend does authoritative validation with proper checksum verification.
 */
export function isValidBitcoinAddress(input: string): boolean {
  return BITCOIN_ADDRESS_REGEX.test(input.trim())
}

/**
 * Check if input is a valid email address format
 */
export function isValidEmail(input: string): boolean {
  return EMAIL_REGEX.test(input.trim())
}

// =============================================================================
// Script Type Detection
// =============================================================================

/**
 * Extract script type from an output descriptor prefix.
 * Returns a script type key (e.g. 'p2wpkh', 'p2tr') or empty string if unknown.
 */
export function getDescriptorScriptType(input: string): string {
  const trimmed = input.trim()
  if (trimmed.startsWith('sh(wpkh(')) return 'p2sh'
  if (trimmed.startsWith('wpkh(')) return 'p2wpkh'
  if (trimmed.startsWith('wsh(')) return 'p2wsh'
  if (trimmed.startsWith('pkh(')) return 'p2pkh'
  if (trimmed.startsWith('tr(')) return 'p2tr'
  return ''
}

/**
 * Infer script type from a Bitcoin address prefix.
 * Returns a script type key or empty string if unknown.
 */
export function getAddressScriptType(address: string): string {
  const trimmed = address.trim()
  if (trimmed.startsWith('bc1p') || trimmed.startsWith('tb1p') || trimmed.startsWith('bcrt1p')) return 'p2tr'
  if (trimmed.startsWith('bc1q') || trimmed.startsWith('tb1q') || trimmed.startsWith('bcrt1q')) return 'p2wpkh'
  if (trimmed.startsWith('3') || trimmed.startsWith('2')) return 'p2sh'
  if (trimmed.startsWith('1') || trimmed.startsWith('m') || trimmed.startsWith('n')) return 'p2pkh'
  return ''
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
