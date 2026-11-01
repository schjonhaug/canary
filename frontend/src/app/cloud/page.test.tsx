import { render, screen } from '@testing-library/react'
import CloudPage, { metadata } from './page'

const testNonce = 'cloud-page-test-nonce'

const mockNotFound = jest.fn(() => {
  throw new Error('NEXT_NOT_FOUND')
})

jest.mock('next/navigation', () => ({
  notFound: () => mockNotFound(),
}))

jest.mock('next/headers', () => ({
  headers: jest.fn(async () => new Headers({ 'x-nonce': testNonce })),
}))

jest.mock('@/components/cloud-page', () => {
  const MockCloudPageContent = ({ nonce }: { nonce?: string }) => <div data-nonce={nonce}>Canary Cloud content</div>
  MockCloudPageContent.displayName = 'MockCloudPageContent'
  return MockCloudPageContent
})

const originalMode = process.env.NEXT_PUBLIC_CANARY_MODE

describe('/cloud', () => {
  afterEach(() => {
    process.env.NEXT_PUBLIC_CANARY_MODE = originalMode
    jest.clearAllMocks()
  })

  it('renders in Cloud mode with the request nonce for its structured data', async () => {
    process.env.NEXT_PUBLIC_CANARY_MODE = 'cloud'
    render(await CloudPage())

    expect(screen.getByText('Canary Cloud content')).toHaveAttribute('data-nonce', testNonce)
    expect(mockNotFound).not.toHaveBeenCalled()
  })

  it('returns not found in self-hosted mode', async () => {
    process.env.NEXT_PUBLIC_CANARY_MODE = 'self-hosted'

    await expect(CloudPage()).rejects.toThrow('NEXT_NOT_FOUND')
    expect(mockNotFound).toHaveBeenCalled()
  })

  it('has dedicated hosted-subscription metadata', () => {
    expect(metadata.title).toBe('Canary Cloud | Hosted Bitcoin Wallet Monitoring')
    expect(metadata.description).toContain('hosted Canary subscription')
    expect(metadata.alternates).toEqual(expect.objectContaining({ canonical: 'https://canarybitcoin.com/cloud' }))
  })
})
