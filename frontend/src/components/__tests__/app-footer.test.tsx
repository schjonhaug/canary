import { render, screen } from '@testing-library/react'
import { AppFooter } from '../app-footer'

const mockUseAuth = jest.fn()
const mockUseBlockHeader = jest.fn()
const mockUseRelativeTime = jest.fn()

jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('../../hooks/useBlockHeader', () => ({
  useBlockHeader: () => mockUseBlockHeader(),
}))

jest.mock('../../hooks/useRelativeTime', () => ({
  useRelativeTime: (timestamp: number | undefined) => mockUseRelativeTime(timestamp),
}))

jest.mock('../../hooks/useFormatters', () => ({
  useFormatters: () => ({
    formatNumber: (value: number) => value.toLocaleString('en-US'),
  }),
}))

describe('AppFooter', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockUseAuth.mockReturnValue({ isCloudMode: false })
    mockUseBlockHeader.mockReturnValue({
      blockHeader: {
        height: 892441,
        timestamp: 1744178400,
        network: 'mainnet',
      },
    })
  })

  it('does not render the timestamp separator while relative time is empty', () => {
    mockUseRelativeTime.mockReturnValue('')

    render(<AppFooter />)

    const blockInfo = screen.getByText('Block 892,441')
    expect(blockInfo).toBeInTheDocument()
    expect(blockInfo).not.toHaveTextContent('•')
  })

  it('renders the timestamp separator with relative time text', () => {
    mockUseRelativeTime.mockReturnValue('2 minutes ago')

    render(<AppFooter />)

    expect(screen.getByText('Block 892,441 • 2 minutes ago')).toBeInTheDocument()
  })

  it('renders the network fallback when no block header is available', () => {
    mockUseBlockHeader.mockReturnValue({ blockHeader: null })
    mockUseRelativeTime.mockReturnValue('')

    render(<AppFooter />)

    expect(screen.getByText('Connecting to network...')).toBeInTheDocument()
  })

  it('replaces the GitHub link with the version and points at the release', () => {
    mockUseRelativeTime.mockReturnValue('')

    render(<AppFooter />)

    expect(screen.getByRole('link', { name: 'Version 1.6.4' })).toHaveAttribute(
      'href',
      'https://github.com/schjonhaug/canary/releases/tag/v1.6.4'
    )
    expect(screen.queryByRole('link', { name: 'GitHub' })).not.toBeInTheDocument()
  })

  it('does not show the version on cloud', () => {
    mockUseAuth.mockReturnValue({ isCloudMode: true })
    mockUseRelativeTime.mockReturnValue('')

    render(<AppFooter />)

    expect(screen.getByText('Canary Wallet')).toBeInTheDocument()
    expect(screen.queryByRole('link', { name: 'Version 1.6.4' })).not.toBeInTheDocument()
  })

  it('falls back to the GitHub repo link when the build version is unavailable', () => {
    const previousVersion = process.env.NEXT_PUBLIC_APP_VERSION
    process.env.NEXT_PUBLIC_APP_VERSION = ''
    mockUseRelativeTime.mockReturnValue('')

    try {
      render(<AppFooter />)
      expect(screen.getByRole('link', { name: 'GitHub' })).toHaveAttribute(
        'href',
        'https://github.com/schjonhaug/canary'
      )
    } finally {
      process.env.NEXT_PUBLIC_APP_VERSION = previousVersion
    }
  })
})
