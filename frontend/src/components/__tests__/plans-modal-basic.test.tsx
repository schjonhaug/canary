import React from 'react'
import { render, screen } from '@testing-library/react'
import { PlansModal } from '../plans-modal'

// Mock the api module
jest.mock('../../lib/api', () => ({
  api: {
    createCheckoutSession: jest.fn(),
  },
}))

// Mock the auth context
jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => ({
    isAuthenticated: true,
    refreshBillingStatus: jest.fn(),
  }),
}))

const mockPlanComparison = jest.fn(() => <div data-testid="plan-comparison">Plan Comparison</div>)

// Mock PlanComparison component
jest.mock('../plan-comparison', () => ({
  PlanComparison: (props: unknown) => mockPlanComparison(props),
}))

describe('PlansModal Basic Functionality', () => {
  const defaultProps = {
    isOpen: true,
    onClose: jest.fn(),
    currentTier: 'personal',
  }

  describe('Modal Visibility', () => {
    beforeEach(() => {
      mockPlanComparison.mockClear()
    })

    it('renders when open', () => {
      render(<PlansModal {...defaultProps} currentWalletCount={1} limitType="wallets" />)
      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
    })

    it('does not render when closed', () => {
      render(<PlansModal {...defaultProps} isOpen={false} currentWalletCount={1} />)
      expect(screen.queryByText('Wallet Limit Reached')).not.toBeInTheDocument()
    })
  })

  describe('Wallet Limit Display', () => {
    it('shows wallet limit message for personal tier', () => {
      render(<PlansModal {...defaultProps} currentWalletCount={1} limitType="wallets" />)

      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
      expect(screen.getByText(/You've reached your.*limit of 1 on the Personal plan/)).toBeInTheDocument()
      expect(screen.getByText('Current usage: 1 / 1')).toBeInTheDocument()
      // "Personal" tier name appears in the description
      expect(screen.getByText(/Personal plan/)).toBeInTheDocument()
    })

    it('shows plural form for team tier wallets', () => {
      render(
        <PlansModal
          {...defaultProps}
          currentTier="team"
          currentWalletCount={5}
          limitType="wallets"
        />
      )

      // Check for text that might be split across elements
      expect(screen.getAllByText((content, element) => {
        return (element?.textContent?.includes("You've reached your") && element?.textContent?.includes("limit of 5 on the Team plan")) ?? false
      })[0]).toBeInTheDocument()
      expect(screen.getByText('Current usage: 5 / 5')).toBeInTheDocument()
      // "Team" tier name appears in the description
      expect(screen.getByText(/Team plan/)).toBeInTheDocument()
    })
  })

  describe('Contact Limit Display', () => {
    it('shows contact limit message for personal tier', () => {
      render(<PlansModal {...defaultProps} currentContactCount={1} limitType="contacts" />)

      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      expect(screen.getByText(/You've reached your.*limit of 1 on the Personal plan/)).toBeInTheDocument()
      expect(screen.getByText('Current usage: 1 / 1')).toBeInTheDocument()
      expect(screen.getByText(/upgrade to add more/)).toBeInTheDocument()
    })

    it('shows plural form for team tier contacts', () => {
      render(
        <PlansModal
          {...defaultProps}
          currentTier="team"
          currentContactCount={5}
          limitType="contacts"
        />
      )

      // Check for text that might be split across elements
      expect(screen.getAllByText((content, element) => {
        return (element?.textContent?.includes("You've reached your") && element?.textContent?.includes("limit of 5 on the Team plan")) ?? false
      })[0]).toBeInTheDocument()
      expect(screen.getByText('Current usage: 5 / 5')).toBeInTheDocument()
    })

  })

  describe('Tier Display in Description', () => {
    it('displays Personal tier in description', () => {
      render(<PlansModal {...defaultProps} currentWalletCount={1} />)
      expect(screen.getByText(/Personal plan/)).toBeInTheDocument()
    })

    it('displays Team tier in description', () => {
      render(<PlansModal {...defaultProps} currentTier="team" currentWalletCount={5} />)
      expect(screen.getByText(/Team plan/)).toBeInTheDocument()
    })

  })

  describe('Default Props', () => {
    it('defaults to wallets limitType', () => {
      render(<PlansModal {...defaultProps} />)
      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
    })

    it('defaults wallet count to 0', () => {
      render(<PlansModal {...defaultProps} limitType="wallets" />)
      expect(screen.getByText('Current usage: 0 / 1')).toBeInTheDocument()
    })

    it('defaults contact count to 0', () => {
      render(<PlansModal {...defaultProps} limitType="contacts" />)
      expect(screen.getByText('Current usage: 0 / 1')).toBeInTheDocument()
    })
  })

  describe('PlanComparison Integration', () => {
    it('renders PlanComparison component', () => {
      render(<PlansModal {...defaultProps} currentWalletCount={1} />)
      expect(screen.getByTestId('plan-comparison')).toBeInTheDocument()
    })

    it('does not offer a second checkout to active BTCPay subscribers', () => {
      render(
        <PlansModal
          {...defaultProps}
          currentWalletCount={1}
          billingStatus={{
            subscription_status: 'active',
            can_manage_billing: false,
          }}
        />
      )

      expect(mockPlanComparison).toHaveBeenCalled()
      const lastCall = mockPlanComparison.mock.calls.at(-1)?.[0] as {
        hasPaidSubscription?: boolean
        onUpgrade?: unknown
      }
      expect(lastCall?.hasPaidSubscription).toBe(true)
      expect(lastCall?.onUpgrade).toBeUndefined()
    })
  })

  describe('Modal Content Structure', () => {
    it('contains all expected elements', () => {
      render(<PlansModal {...defaultProps} currentContactCount={1} limitType="contacts" />)
      
      // Check for title with icon
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      
      // Check for description - use flexible matcher for text split across elements
      expect(screen.getAllByText((content, element) => {
        return element?.textContent?.includes("You've reached your contact limit") ?? false
      })[0]).toBeInTheDocument()
      
      // Check for current usage display
      expect(screen.getByText(/Current usage:/)).toBeInTheDocument()
      
      // Check for plan comparison
      expect(screen.getByTestId('plan-comparison')).toBeInTheDocument()
    })

    it('has proper modal attributes', () => {
      render(<PlansModal {...defaultProps} currentWalletCount={1} />)
      
      // Dialog should be present
      expect(screen.getByRole('dialog')).toBeInTheDocument()
    })
  })

  describe('Limit Type Flexibility', () => {
    it('correctly switches between wallet and contact modes', () => {
      const { rerender } = render(
        <PlansModal {...defaultProps} currentWalletCount={1} limitType="wallets" />
      )
      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
      expect(screen.getAllByText((content, element) => {
        return element?.textContent?.includes("wallet limit") ?? false
      })[0]).toBeInTheDocument()

      rerender(
        <PlansModal {...defaultProps} currentContactCount={1} limitType="contacts" />
      )
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      expect(screen.getAllByText((content, element) => {
        return element?.textContent?.includes("contact limit") ?? false
      })[0]).toBeInTheDocument()
    })
  })
})
