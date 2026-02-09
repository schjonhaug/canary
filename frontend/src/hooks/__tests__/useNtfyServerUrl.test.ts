import { normalizeNtfyUrl } from '../useNtfyServerUrl'

describe('normalizeNtfyUrl', () => {
  it('returns null for empty string', () => {
    expect(normalizeNtfyUrl('')).toBeNull()
    expect(normalizeNtfyUrl('  ')).toBeNull()
  })

  it('accepts valid https URLs', () => {
    expect(normalizeNtfyUrl('https://ntfy.example.com')).toBe('https://ntfy.example.com')
  })

  it('accepts valid http URLs', () => {
    expect(normalizeNtfyUrl('http://ntfy.local:8080')).toBe('http://ntfy.local:8080')
  })

  it('strips trailing slashes', () => {
    expect(normalizeNtfyUrl('https://ntfy.example.com/')).toBe('https://ntfy.example.com')
    expect(normalizeNtfyUrl('https://ntfy.example.com///')).toBe('https://ntfy.example.com')
  })

  it('prepends https:// when no scheme is present', () => {
    expect(normalizeNtfyUrl('ntfy.example.com')).toBe('https://ntfy.example.com')
  })

  it('rejects javascript: URLs', () => {
    expect(normalizeNtfyUrl('javascript:alert(1)')).toBeNull()
  })

  it('rejects data: URLs', () => {
    expect(normalizeNtfyUrl('data:text/html,<script>alert(1)</script>')).toBeNull()
  })

  it('rejects ftp: URLs', () => {
    expect(normalizeNtfyUrl('ftp://ntfy.example.com')).toBeNull()
  })

  it('preserves path components', () => {
    expect(normalizeNtfyUrl('https://example.com/ntfy')).toBe('https://example.com/ntfy')
  })

  it('preserves port numbers', () => {
    expect(normalizeNtfyUrl('https://ntfy.local:8443')).toBe('https://ntfy.local:8443')
  })
})
