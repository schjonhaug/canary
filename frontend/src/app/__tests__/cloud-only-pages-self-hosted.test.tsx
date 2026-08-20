import { render } from '@testing-library/react'
import SubscriptionPage from '../subscription/page'
import ContactPage from '../contact/page'
import SignUpPage from '../sign-up/page'
import ForgotPasswordPage from '../forgot-password/page'
import SignUpSuccessPage from '../sign-up/success/page'
import ResetPasswordPage from '../reset-password/[token]/page'
import VerifyEmailPage from '../verify-email/[token]/page'
import DemoPage from '../demo/page'

const mockPush = jest.fn()
const mockSearchParamGet = jest.fn(() => null)
const mockNotFound = jest.fn(() => {
  throw new Error('NEXT_NOT_FOUND')
})

jest.mock('next/navigation', () => ({
  notFound: () => mockNotFound(),
  useRouter: () => ({
    push: mockPush,
  }),
  useParams: () => ({
    token: 'test-token',
  }),
  useSearchParams: () => ({
    get: mockSearchParamGet,
  }),
}))

const authMock = {
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

jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => authMock,
}))

jest.mock('../../lib/api', () => ({
  api: {
    submitContactForm: jest.fn(),
    forgotPassword: jest.fn(),
    resetPassword: jest.fn(),
    createCustomerPortalSession: jest.fn(),
  },
  ApiError: class extends Error {},
}))

jest.mock('../subscription/success', () => {
  const MockBillingSuccessPage = () => <div>Billing Success</div>
  MockBillingSuccessPage.displayName = 'MockBillingSuccessPage'
  return MockBillingSuccessPage
})

jest.mock('../subscription/cancel', () => {
  const MockBillingCancelPage = () => <div>Billing Cancel</div>
  MockBillingCancelPage.displayName = 'MockBillingCancelPage'
  return MockBillingCancelPage
})

describe('cloud-only pages in self-hosted mode', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockSearchParamGet.mockReturnValue(null)
  })

  it.each([
    ['subscription', SubscriptionPage],
    ['contact', ContactPage],
    ['sign-up', SignUpPage],
    ['forgot-password', ForgotPasswordPage],
    ['sign-up success', SignUpSuccessPage],
    ['reset-password token', ResetPasswordPage],
    ['verify-email token', VerifyEmailPage],
    ['demo', DemoPage],
  ])('calls notFound for %s', (_, PageComponent) => {
    expect(() => render(<PageComponent />)).toThrow('NEXT_NOT_FOUND')
    expect(mockNotFound).toHaveBeenCalled()
  })

  it('calls notFound for subscription success state', () => {
    mockSearchParamGet.mockImplementation((key: string) => key === 'success' ? 'true' : null)

    expect(() => render(<SubscriptionPage />)).toThrow('NEXT_NOT_FOUND')
    expect(mockNotFound).toHaveBeenCalled()
  })

  it('calls notFound for subscription cancelled state', () => {
    mockSearchParamGet.mockImplementation((key: string) => key === 'cancelled' ? 'true' : null)

    expect(() => render(<SubscriptionPage />)).toThrow('NEXT_NOT_FOUND')
    expect(mockNotFound).toHaveBeenCalled()
  })

  it('does not trigger demo login side effects before notFound', () => {
    expect(() => render(<DemoPage />)).toThrow('NEXT_NOT_FOUND')
    expect(authMock.demoLogin).not.toHaveBeenCalled()
    expect(authMock.logout).not.toHaveBeenCalled()
  })
})
