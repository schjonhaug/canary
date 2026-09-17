import { render, waitFor, act } from '@testing-library/react'
import { AuthProvider } from '../auth-context'
import { api } from '@/lib/api'

const mockPush = jest.fn()

jest.mock('next/navigation', () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}))

jest.mock('@/lib/api', () => ({
  api: {
    getMe: jest.fn(),
    getBillingStatus: jest.fn(),
  },
}))

describe('AuthProvider session expiry', () => {
  const originalPathname = window.location.pathname

  beforeEach(() => {
    jest.clearAllMocks()
    ;(api.getMe as jest.Mock).mockRejectedValue(new Error('unauthenticated'))
    window.history.replaceState({}, '', '/')
  })

  afterEach(() => {
    window.history.replaceState({}, '', originalPathname)
  })

  it('does not send visitors from the homepage to sign-in when an admin session expires', async () => {
    render(
      <AuthProvider>
        <div>child</div>
      </AuthProvider>,
    )

    await waitFor(() => {
      expect(api.getMe).toHaveBeenCalled()
    })

    await act(async () => {
      window.dispatchEvent(new CustomEvent('canary-auth-expired'))
    })

    expect(mockPush).not.toHaveBeenCalled()
  })

  it('sends visitors from protected pages to sign-in when an admin session expires', async () => {
    window.history.replaceState({}, '', '/wallets')

    render(
      <AuthProvider>
        <div>child</div>
      </AuthProvider>,
    )

    await waitFor(() => {
      expect(api.getMe).toHaveBeenCalled()
    })

    await act(async () => {
      window.dispatchEvent(new CustomEvent('canary-auth-expired'))
    })

    expect(mockPush).toHaveBeenCalledWith('/sign-in')
  })
})
