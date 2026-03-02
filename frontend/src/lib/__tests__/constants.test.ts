import { getDescriptorSigningType } from '../constants'

describe('getDescriptorSigningType', () => {
  it('returns { m, n } for sortedmulti descriptor', () => {
    const descriptor = 'wsh(sortedmulti(2,[aabbccdd/48h/0h/0h/2h]xpub.../0/*,[eeff0011/48h/0h/0h/2h]xpub.../0/*,[22334455/48h/0h/0h/2h]xpub.../0/*))#checksum'
    const result = getDescriptorSigningType(descriptor)
    expect(result).toEqual({ m: 2, n: 3 })
  })

  it('returns null for single-sig descriptor', () => {
    const descriptor = 'wpkh([aabbccdd/84h/0h/0h]xpub.../0/*)#checksum'
    expect(getDescriptorSigningType(descriptor)).toBeNull()
  })

  it('handles multi_a (Taproot multisig)', () => {
    const descriptor = 'tr(xpub.../0/*,multi_a(2,xpub.../0/*,xpub.../0/*))#checksum'
    const result = getDescriptorSigningType(descriptor)
    expect(result).toEqual({ m: 2, n: 2 })
  })

  it('returns null for non-multisig descriptors', () => {
    expect(getDescriptorSigningType('pkh(xpub...)#abc')).toBeNull()
    expect(getDescriptorSigningType('tr(xpub...)#abc')).toBeNull()
  })
})
