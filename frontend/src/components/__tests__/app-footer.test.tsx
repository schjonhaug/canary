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
})
