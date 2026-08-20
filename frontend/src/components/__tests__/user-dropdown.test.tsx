import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { UserDropdown } from '../user-dropdown'
import { SELF_HOSTED_ADMIN_EMAIL } from '@/lib/constants'

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

describe('UserDropdown', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it('shows self-hosted Settings and Sign out without cloud-only items', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      user: { id: 1, email: SELF_HOSTED_ADMIN_EMAIL, name: 'Admin', is_admin: true, is_demo: false, email_verified: true },
      isCloudMode: false,
      isSelfHostedMode: true,
    })

    render(<UserDropdown />)

    expect(screen.getByRole('button', { name: /admin/i })).toBeInTheDocument()
    expect(screen.queryByText(SELF_HOSTED_ADMIN_EMAIL)).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /admin/i }))

    expect(screen.getAllByText('Admin')).toHaveLength(2)
    expect(screen.queryByText(SELF_HOSTED_ADMIN_EMAIL)).not.toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Settings' })).toHaveAttribute('href', '/settings')
    expect(screen.queryByRole('link', { name: 'Contact' })).not.toBeInTheDocument()
    expect(screen.queryByRole('link', { name: 'Subscription' })).not.toBeInTheDocument()

    await user.click(screen.getByRole('menuitem', { name: 'Sign out' }))

    expect(mockPush).toHaveBeenCalledWith('/sign-out')
  })

  it('keeps cloud non-admin menu items visible', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      user: { id: 2, email: 'user@example.com', name: 'Cloud User', is_admin: false, is_demo: false, email_verified: true },
      isCloudMode: true,
      isSelfHostedMode: false,
    })

    render(<UserDropdown />)

    expect(screen.getByRole('button', { name: /cloud user/i })).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /cloud user/i }))

    expect(screen.getByText('user@example.com')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Settings' })).toHaveAttribute('href', '/settings')
    expect(screen.getByRole('link', { name: 'Contact' })).toHaveAttribute('href', '/contact')
    expect(screen.getByRole('link', { name: 'Subscription' })).toHaveAttribute('href', '/subscription')
  })

  it('keeps cloud admin subscription hidden', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      user: { id: 3, email: 'admin@example.com', name: 'Cloud Admin', is_admin: true, is_demo: false, email_verified: true },
      isCloudMode: true,
      isSelfHostedMode: false,
    })

    render(<UserDropdown />)

    await user.click(screen.getByRole('button', { name: /cloud admin/i }))

    expect(screen.getByRole('link', { name: 'Settings' })).toHaveAttribute('href', '/settings')
    expect(screen.getByRole('link', { name: 'Contact' })).toHaveAttribute('href', '/contact')
    expect(screen.queryByRole('link', { name: 'Subscription' })).not.toBeInTheDocument()
  })

  it('keeps cloud demo dropdown minimal', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      user: { id: 4, email: 'demo@canarybitcoin.com', name: 'Demo User', is_admin: false, is_demo: true, email_verified: true },
      isCloudMode: true,
      isSelfHostedMode: false,
    })

    render(<UserDropdown />)

    await user.click(screen.getByRole('button', { name: /demo user/i }))

    expect(screen.queryByRole('link', { name: 'Settings' })).not.toBeInTheDocument()
    expect(screen.queryByRole('link', { name: 'Contact' })).not.toBeInTheDocument()
    expect(screen.queryByRole('link', { name: 'Subscription' })).not.toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Sign out' })).toBeInTheDocument()
  })
})
