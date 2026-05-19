import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { UserDropdown } from '../user-dropdown'

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

  it('shows a self-hosted admin label without exposing admin@local', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      user: { id: 1, email: 'admin@local', name: 'Admin', is_admin: true, is_demo: false, email_verified: true },
      isCloudMode: false,
      isSelfHostedMode: true,
    })

    render(<UserDropdown />)

    expect(screen.getByRole('button', { name: /admin/i })).toBeInTheDocument()
    expect(screen.queryByText('admin@local')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /admin/i }))

    expect(screen.getAllByText('Admin')).toHaveLength(2)
    expect(screen.queryByText('admin@local')).not.toBeInTheDocument()
  })

  it('keeps cloud user email visible', async () => {
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
  })
})
