import { render, screen, waitFor } from '@testing-library/react'
import VerifyEmailPage from './page'
import { ApiError } from '@/lib/utils'

const mockPush = jest.fn()
const mockVerifyEmail = jest.fn()

jest.mock('next/navigation', () => ({
  useParams: () => ({ token: 'test-token' }),
  useRouter: () => ({
    push: mockPush,
  }),
}))

jest.mock('@/lib/api', () => ({
  api: {
    verifyEmail: (...args: unknown[]) => mockVerifyEmail(...args),
  },
  ApiError: jest.requireActual('@/lib/utils').ApiError,
}))

describe('VerifyEmailPage', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it('shows success state when verification succeeds', async () => {
    mockVerifyEmail.mockResolvedValue({ message: 'ok' })

    const { container } = render(<VerifyEmailPage />)

    await waitFor(() => {
      expect(container.querySelector('[data-slot="card-title"]')).toHaveTextContent('Email verified')
      expect(screen.getByText('Your email has been verified. You can now sign in.')).toBeInTheDocument()
    })
  })

  it('shows translated ApiError message when verification fails', async () => {
    mockVerifyEmail.mockRejectedValue(new ApiError('Resource not found', 'not_found', 404, 'invalid_verification_token'))

    render(<VerifyEmailPage />)

    await waitFor(() => {
      expect(screen.getByText('Invalid or expired verification token.')).toBeInTheDocument()
    })
  })
})
