import React from 'react'
import { render, screen } from '@testing-library/react'
import { UpgradeModal } from '../upgrade-modal'

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

// Mock PlanComparison component
jest.mock('../plan-comparison', () => ({
  PlanComparison: () => <div data-testid="plan-comparison">Plan Comparison</div>,
}))

describe('UpgradeModal Basic Functionality', () => {
  const defaultProps = {
    isOpen: true,
    onClose: jest.fn(),
    currentTier: 'personal',
  }

  describe('Modal Visibility', () => {
    it('renders when open', () => {
      render(<UpgradeModal {...defaultProps} currentWalletCount={1} limitType="wallets" />)
      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
    })

    it('does not render when closed', () => {
      render(<UpgradeModal {...defaultProps} isOpen={false} currentWalletCount={1} />)
      expect(screen.queryByText('Wallet Limit Reached')).not.toBeInTheDocument()
    })
  })

  describe('Wallet Limit Display', () => {
    it('shows wallet limit message for personal tier', () => {
      render(<UpgradeModal {...defaultProps} currentWalletCount={1} limitType="wallets" />)
      
      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
      expect(screen.getByText(/You've reached your wallet limit of 1 wallet/)).toBeInTheDocument()
      expect(screen.getByText('Current usage: 1 / 1 wallets')).toBeInTheDocument()
      expect(screen.getByText('Personal')).toBeInTheDocument()
    })

    it('shows plural form for pro tier wallets', () => {
      render(
        <UpgradeModal 
          {...defaultProps}
          currentTier="pro"
          currentWalletCount={15}
          limitType="wallets"
        />
      )
      
      expect(screen.getByText(/You've reached your wallet limit of 15 wallets/)).toBeInTheDocument()
      expect(screen.getByText('Current usage: 15 / 15 wallets')).toBeInTheDocument()
      expect(screen.getByText('Pro')).toBeInTheDocument()
    })
  })

  describe('Contact Limit Display', () => {
    it('shows contact limit message for personal tier', () => {
      render(<UpgradeModal {...defaultProps} currentContactCount={1} limitType="contacts" />)
      
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      expect(screen.getByText(/You've reached your contact limit of 1 contact/)).toBeInTheDocument()
      expect(screen.getByText('Current usage: 1 / 1 contacts')).toBeInTheDocument()
      expect(screen.getByText(/upgrade to add more contacts/)).toBeInTheDocument()
    })

    it('shows plural form for pro tier contacts', () => {
      render(
        <UpgradeModal 
          {...defaultProps}
          currentTier="pro"
          currentContactCount={10}
          limitType="contacts"
        />
      )
      
      expect(screen.getByText(/You've reached your contact limit of 10 contacts/)).toBeInTheDocument()
      expect(screen.getByText('Current usage: 10 / 10 contacts')).toBeInTheDocument()
    })

    it('handles business tier unlimited limits', () => {
      render(
        <UpgradeModal 
          {...defaultProps}
          currentTier="business"
          currentContactCount={50}
          limitType="contacts"
        />
      )
      
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      // Business tier should show usage information (exact format may vary due to null limits)
      expect(screen.getByText(/Current usage: 50/)).toBeInTheDocument()
    })
  })

  describe('Tier Badge Display', () => {
    it('displays Personal tier badge', () => {
      render(<UpgradeModal {...defaultProps} currentWalletCount={1} />)
      expect(screen.getByText('Personal')).toBeInTheDocument()
    })

    it('displays Pro tier badge', () => {
      render(<UpgradeModal {...defaultProps} currentTier="pro" currentWalletCount={15} />)
      expect(screen.getByText('Pro')).toBeInTheDocument()
    })

    it('displays Business tier badge', () => {
      render(<UpgradeModal {...defaultProps} currentTier="business" currentWalletCount={100} />)
      expect(screen.getByText('Business')).toBeInTheDocument()
    })
  })

  describe('Default Props', () => {
    it('defaults to wallets limitType', () => {
      render(<UpgradeModal {...defaultProps} />)
      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
    })

    it('defaults wallet count to 0', () => {
      render(<UpgradeModal {...defaultProps} limitType="wallets" />)
      expect(screen.getByText('Current usage: 0 / 1 wallets')).toBeInTheDocument()
    })

    it('defaults contact count to 0', () => {
      render(<UpgradeModal {...defaultProps} limitType="contacts" />)
      expect(screen.getByText('Current usage: 0 / 1 contacts')).toBeInTheDocument()
    })
  })

  describe('PlanComparison Integration', () => {
    it('renders PlanComparison component', () => {
      render(<UpgradeModal {...defaultProps} currentWalletCount={1} />)
      expect(screen.getByTestId('plan-comparison')).toBeInTheDocument()
    })
  })

  describe('Modal Content Structure', () => {
    it('contains all expected elements', () => {
      render(<UpgradeModal {...defaultProps} currentContactCount={1} limitType="contacts" />)
      
      // Check for title with icon
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      
      // Check for description
      expect(screen.getByText(/You've reached your contact limit/)).toBeInTheDocument()
      
      // Check for current usage display
      expect(screen.getByText(/Current usage:/)).toBeInTheDocument()
      
      // Check for plan comparison
      expect(screen.getByTestId('plan-comparison')).toBeInTheDocument()
    })

    it('has proper modal attributes', () => {
      render(<UpgradeModal {...defaultProps} currentWalletCount={1} />)
      
      // Dialog should be present
      expect(screen.getByRole('dialog')).toBeInTheDocument()
    })
  })

  describe('Limit Type Flexibility', () => {
    it('correctly switches between wallet and contact modes', () => {
      const { rerender } = render(
        <UpgradeModal {...defaultProps} currentWalletCount={1} limitType="wallets" />
      )
      expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
      expect(screen.getByText(/wallet limit/)).toBeInTheDocument()

      rerender(
        <UpgradeModal {...defaultProps} currentContactCount={1} limitType="contacts" />
      )
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      expect(screen.getByText(/contact limit/)).toBeInTheDocument()
    })
  })
})