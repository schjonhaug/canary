import { render, screen } from '@testing-library/react'
import CloudPage, { metadata } from './page'

const mockNotFound = jest.fn(() => {
  throw new Error('NEXT_NOT_FOUND')
})

jest.mock('next/navigation', () => ({
  notFound: () => mockNotFound(),
}))

jest.mock('@/components/cloud-page', () => {
  const MockCloudPageContent = () => <div>Canary Cloud content</div>
  MockCloudPageContent.displayName = 'MockCloudPageContent'
  return MockCloudPageContent
})

const originalMode = process.env.NEXT_PUBLIC_CANARY_MODE

describe('/cloud', () => {
  afterEach(() => {
    process.env.NEXT_PUBLIC_CANARY_MODE = originalMode
    jest.clearAllMocks()
  })

  it('renders in Cloud mode', () => {
    process.env.NEXT_PUBLIC_CANARY_MODE = 'cloud'
    render(<CloudPage />)

    expect(screen.getByText('Canary Cloud content')).toBeInTheDocument()
    expect(mockNotFound).not.toHaveBeenCalled()
  })

  it('returns not found in self-hosted mode', () => {
    process.env.NEXT_PUBLIC_CANARY_MODE = 'self-hosted'

    expect(() => render(<CloudPage />)).toThrow('NEXT_NOT_FOUND')
    expect(mockNotFound).toHaveBeenCalled()
  })

  it('has dedicated hosted-subscription metadata', () => {
    expect(metadata.title).toBe('Canary Cloud | Hosted Bitcoin Wallet Monitoring')
    expect(metadata.description).toContain('hosted Canary subscription')
    expect(metadata.alternates).toEqual(expect.objectContaining({ canonical: 'https://canarybitcoin.com/cloud' }))
  })
})
