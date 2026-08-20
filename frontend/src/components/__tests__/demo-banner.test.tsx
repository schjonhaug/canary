import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { DemoBanner } from '../demo-banner'

const mockUseAuth = jest.fn()

jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

describe('DemoBanner', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    localStorage.clear()
  })

  it('renders for demo users who have not dismissed it', async () => {
    mockUseAuth.mockReturnValue({
      user: { id: 1, email: 'demo@canarybitcoin.com', is_demo: true },
    })

    render(<DemoBanner />)

    expect(await screen.findByText('Demo Mode')).toBeInTheDocument()
    expect(screen.getByText("You're viewing a read-only demo wallet. All features are view-only.")).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Sign Up to Create Your Own Wallet' })).toHaveAttribute('href', '/sign-up')
  })

  it('does not render for non-demo users', () => {
    mockUseAuth.mockReturnValue({
      user: { id: 2, email: 'user@example.com', is_demo: false },
    })

    const { container } = render(<DemoBanner />)

    expect(container.firstChild).toBeNull()
  })

  it('stays hidden when the demo user previously dismissed it', () => {
    localStorage.setItem('demo_banner_dismissed', 'true')
    mockUseAuth.mockReturnValue({
      user: { id: 1, email: 'demo@canarybitcoin.com', is_demo: true },
    })

    const { container } = render(<DemoBanner />)

    expect(container.firstChild).toBeNull()
  })

  it('stores dismissal and hides when closed', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      user: { id: 1, email: 'demo@canarybitcoin.com', is_demo: true },
    })

    const { container } = render(<DemoBanner />)

    await user.click(await screen.findByRole('button', { name: 'Dismiss banner' }))

    expect(localStorage.getItem('demo_banner_dismissed')).toBe('true')
    await waitFor(() => {
      expect(container.firstChild).toBeNull()
    })
  })
})
