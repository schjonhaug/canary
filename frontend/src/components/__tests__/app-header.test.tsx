import { render, screen } from '@testing-library/react'
import { AppHeader } from '../app-header'

const mockUseAuth = jest.fn()
const mockUsePathname = jest.fn()

jest.mock('next/navigation', () => ({
  usePathname: () => mockUsePathname(),
}))

jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('../user-dropdown', () => ({
  UserDropdown: () => <div data-testid="user-dropdown" />,
}))

describe('AppHeader', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockUsePathname.mockReturnValue('/wallets')
  })

  it('does not render a standalone settings button in self-hosted mode', () => {
    mockUseAuth.mockReturnValue({
      isCloudMode: false,
      user: { id: 1, email: 'admin@local', is_admin: true, is_demo: false, email_verified: true },
    })

    render(<AppHeader />)

    expect(screen.queryByRole('button', { name: 'Settings' })).not.toBeInTheDocument()
    expect(screen.getByTestId('user-dropdown')).toBeInTheDocument()
  })

  it('keeps the add wallet button available for authenticated self-hosted users', () => {
    mockUseAuth.mockReturnValue({
      isCloudMode: false,
      user: { id: 1, email: 'admin@local', is_admin: true, is_demo: false, email_verified: true },
    })

    render(<AppHeader />)

    expect(screen.getByRole('button', { name: 'Add Wallet' })).toBeInTheDocument()
  })
})
