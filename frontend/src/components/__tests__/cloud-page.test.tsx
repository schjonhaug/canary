import { render, screen } from '@testing-library/react'
import CloudPageContent from '../cloud-page'

const mockPlanComparison = jest.fn(() => <div data-testid="plan-comparison">Plan comparison</div>)

jest.mock('../plan-comparison', () => ({
  PlanComparison: (props: unknown) => mockPlanComparison(props),
}))

describe('CloudPageContent', () => {
  beforeEach(() => {
    mockPlanComparison.mockClear()
    render(<CloudPageContent />)
  })

  it('places pricing before the privacy FAQ', () => {
    const pricing = screen.getByRole('heading', { name: 'Canary Cloud plans' })
    const faq = screen.getByRole('heading', { name: 'Privacy questions' })

    expect(pricing.compareDocumentPosition(faq) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.getByText('Canary Cloud stores the descriptors, XPUBs, or addresses you supply, including single-sig and multisig wallets.')).toBeInTheDocument()
    expect(screen.getByText('Yes. That watch-only information can reveal wallet addresses, balances, and transaction history.')).toBeInTheDocument()
    expect(screen.getByText('Your Canary account and subscription can connect wallet information to your account and billing identity.')).toBeInTheDocument()
    expect(screen.getByText('No. Canary Cloud never receives private keys and cannot sign transactions or spend your funds.')).toBeInTheDocument()
  })

  it('integrates the existing pricing comparison and signup flow', () => {
    expect(screen.getByTestId('plan-comparison')).toBeInTheDocument()
    expect(mockPlanComparison).toHaveBeenCalledWith(expect.objectContaining({
      showPricing: true,
      showCallToAction: true,
      showUnifiedTrialButton: true,
    }))
    expect(screen.getByText('Email notifications')).toBeInTheDocument()
    expect(screen.getByText('SMS notifications')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Start free trial/ })).toHaveAttribute('href', '/sign-up')
    expect(screen.getByRole('link', { name: /Sign up/ })).toHaveAttribute('href', '/sign-up')
  })

  it('links to self hosting, demo, sign in, and GitHub', () => {
    expect(screen.getAllByRole('link', { name: /self host/i })[0]).toHaveAttribute('href', '/#install')
    expect(screen.getAllByRole('link', { name: /demo/i })[0]).toHaveAttribute('href', '/demo')
    expect(screen.getAllByRole('link', { name: /sign in/i })[0]).toHaveAttribute('href', '/sign-in')
    expect(screen.getAllByRole('link', { name: /GitHub/i })[0]).toHaveAttribute('target', '_blank')
  })
})
