import { render } from '@testing-library/react'
import SubscriptionPage from '../subscription/page'
import ContactPage from '../contact/page'
import SignInPage from '../sign-in/page'
import SignUpPage from '../sign-up/page'
import ForgotPasswordPage from '../forgot-password/page'
import SignUpSuccessPage from '../sign-up/success/page'
import ResetPasswordPage from '../reset-password/[token]/page'
import VerifyEmailPage from '../verify-email/[token]/page'
import SignOutPage from '../sign-out/page'
import DemoPage from '../demo/page'

const mockPush = jest.fn()
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
    get: jest.fn(() => null),
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

jest.mock('../subscription/success', () => () => <div>Billing Success</div>)
jest.mock('../subscription/cancel', () => () => <div>Billing Cancel</div>)

describe('cloud-only pages in self-hosted mode', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it.each([
    ['subscription', SubscriptionPage],
    ['contact', ContactPage],
    ['sign-in', SignInPage],
    ['sign-up', SignUpPage],
    ['forgot-password', ForgotPasswordPage],
    ['sign-up success', SignUpSuccessPage],
    ['reset-password token', ResetPasswordPage],
    ['verify-email token', VerifyEmailPage],
    ['sign-out', SignOutPage],
    ['demo', DemoPage],
  ])('calls notFound for %s', (_, PageComponent) => {
    expect(() => render(<PageComponent />)).toThrow('NEXT_NOT_FOUND')
    expect(mockNotFound).toHaveBeenCalled()
  })
})
