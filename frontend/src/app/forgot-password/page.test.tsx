import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ComponentProps } from 'react'
import ForgotPasswordPage from './page'
import { ApiError } from '@/lib/utils'

const mockPush = jest.fn()
const mockForgotPassword = jest.fn()

jest.mock('next/navigation', () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}))

jest.mock('next/image', () => ({
  __esModule: true,
  // eslint-disable-next-line @next/next/no-img-element
  default: (props: ComponentProps<'img'>) => <img {...props} alt={props.alt || ''} />,
}))

jest.mock('@/contexts/auth-context', () => ({
  useAuth: () => ({
    isSelfHostedMode: false,
  }),
}))

jest.mock('@/lib/api', () => ({
  api: {
    forgotPassword: (...args: unknown[]) => mockForgotPassword(...args),
  },
  ApiError: jest.requireActual('@/lib/utils').ApiError,
}))

describe('ForgotPasswordPage', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it('shows translated ApiError messages from forgot password requests', async () => {
    const user = userEvent.setup()
    mockForgotPassword.mockRejectedValue(new ApiError('User not found.', 'not_found', 404, 'user_not_found'))

    render(<ForgotPasswordPage />)

    await user.type(screen.getByLabelText('Email'), 'missing@example.com')
    await user.click(screen.getByRole('button', { name: 'Send reset link' }))

    await waitFor(() => {
      expect(screen.getByText('User not found.')).toBeInTheDocument()
    })
  })
})
