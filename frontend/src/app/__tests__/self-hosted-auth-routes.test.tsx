import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import SignInPage from '../sign-in/page'
import SignOutPage from '../sign-out/page'

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

const unauthenticatedSelfHostedAuth = {
  user: null,
  billingStatus: null,
  isLoading: false,
  isAuthenticated: false,
  isSelfHostedMode: true,
  isCloudMode: false,
  login: jest.fn(),
  register: jest.fn(),
  logout: jest.fn(),
  demoLogin: jest.fn(),
  refreshBillingStatus: jest.fn(),
}

describe('self-hosted auth routes', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockUseAuth.mockReturnValue({
      ...unauthenticatedSelfHostedAuth,
      login: jest.fn(),
      logout: jest.fn(),
    })
  })

  it('renders self-hosted sign-in with the built-in admin email', () => {
    render(<SignInPage />)

    expect(screen.getByLabelText('Email')).toHaveValue('admin@local')
    expect(screen.getByLabelText('Password')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: "Don't have an account? Sign up" })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Forgot your password?' })).not.toBeInTheDocument()
  })

  it('submits self-hosted login with admin@local and the entered password', async () => {
    const user = userEvent.setup()
    const login = jest.fn().mockResolvedValue(undefined)
    mockUseAuth.mockReturnValue({
      ...unauthenticatedSelfHostedAuth,
      login,
      logout: jest.fn(),
    })

    render(<SignInPage />)

    await user.type(screen.getByLabelText('Password'), 'correct-horse-battery')
    await user.click(screen.getByRole('button', { name: 'Sign in' }))

    await waitFor(() => {
      expect(login).toHaveBeenCalledWith('admin@local', 'correct-horse-battery')
    })
  })

  it('redirects authenticated self-hosted users from sign-in to wallets', async () => {
    mockUseAuth.mockReturnValue({
      ...unauthenticatedSelfHostedAuth,
      isAuthenticated: true,
      user: { id: 1, email: 'admin@local', is_admin: true, is_demo: false, email_verified: true },
      login: jest.fn(),
      logout: jest.fn(),
    })

    render(<SignInPage />)

    await waitFor(() => {
      expect(mockPush).toHaveBeenCalledWith('/wallets')
    })
  })

  it('logs out self-hosted users and redirects to sign-in', async () => {
    const logout = jest.fn().mockResolvedValue(undefined)
    mockUseAuth.mockReturnValue({
      ...unauthenticatedSelfHostedAuth,
      isAuthenticated: true,
      user: { id: 1, email: 'admin@local', is_admin: true, is_demo: false, email_verified: true },
      login: jest.fn(),
      logout,
    })

    render(<SignOutPage />)

    await waitFor(() => {
      expect(logout).toHaveBeenCalled()
      expect(mockPush).toHaveBeenCalledWith('/sign-in')
    })
  })
})

describe('cloud auth routes', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it('keeps cloud sign-in email empty and shows cloud account links', () => {
    mockUseAuth.mockReturnValue({
      ...unauthenticatedSelfHostedAuth,
      isSelfHostedMode: false,
      isCloudMode: true,
      login: jest.fn(),
      logout: jest.fn(),
    })

    render(<SignInPage />)

    expect(screen.getByLabelText('Email')).toHaveValue('')
    expect(screen.getByRole('button', { name: "Don't have an account? Sign up" })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Forgot your password?' })).toBeInTheDocument()
  })

  it('logs out cloud users and redirects to the home page', async () => {
    const logout = jest.fn().mockResolvedValue(undefined)
    mockUseAuth.mockReturnValue({
      ...unauthenticatedSelfHostedAuth,
      isSelfHostedMode: false,
      isCloudMode: true,
      isAuthenticated: true,
      user: { id: 1, email: 'user@example.com', is_admin: false, is_demo: false, email_verified: true },
      login: jest.fn(),
      logout,
    })

    render(<SignOutPage />)

    await waitFor(() => {
      expect(logout).toHaveBeenCalled()
      expect(mockPush).toHaveBeenCalledWith('/')
    })
  })
})
