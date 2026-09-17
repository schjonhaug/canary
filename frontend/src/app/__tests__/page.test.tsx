import { render, screen, waitFor } from '@testing-library/react'
import HomePage from '../page'

const mockPush = jest.fn()
const mockUseAuth = jest.fn()

jest.mock('next/navigation', () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}))

jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('../../components/landing-page', () => ({
  __esModule: true,
  default: () => <div data-testid="landing-page" />,
}))

jest.mock('../../components/ui/loading-spinner', () => ({
  LoadingSpinner: () => <div data-testid="loading-spinner" />,
}))

describe('HomePage', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it('shows the cloud landing page while the session probe is still loading', () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: false,
      isLoading: true,
      isCloudMode: true,
      isSelfHostedMode: false,
      user: null,
    })

    render(<HomePage />)

    expect(screen.getByTestId('landing-page')).toBeInTheDocument()
    expect(screen.queryByTestId('loading-spinner')).not.toBeInTheDocument()
    expect(mockPush).not.toHaveBeenCalled()
  })

  it('keeps unauthenticated cloud visitors on the landing page', async () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: false,
      isLoading: false,
      isCloudMode: true,
      isSelfHostedMode: false,
      user: null,
    })

    render(<HomePage />)

    expect(screen.getByTestId('landing-page')).toBeInTheDocument()
    await waitFor(() => {
      expect(mockPush).not.toHaveBeenCalled()
    })
  })

  it('sends unauthenticated self-hosted visitors to sign-in', async () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: false,
      isLoading: false,
      isCloudMode: false,
      isSelfHostedMode: true,
      user: null,
    })

    render(<HomePage />)

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/sign-in')
    })
  })

  it('sends authenticated cloud users to wallets', async () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: true,
      isSelfHostedMode: false,
      user: { is_admin: false },
    })

    render(<HomePage />)

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/wallets')
    })
  })
})
