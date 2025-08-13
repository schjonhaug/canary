import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

// Create a test component that mimics the wallet detail page contact limit behavior
import { hasReachedContactLimit, getContactLimit } from '../../lib/utils'

// Mock the PlansModal component
const MockPlansModal = ({ isOpen, onClose, limitType, currentContactCount }: any) => {
  if (!isOpen) return null
  return (
    <div data-testid="plans-modal">
      <h2>Contact Limit Reached</h2>
      <p>Current usage: {currentContactCount} contacts</p>
      <p>Limit type: {limitType}</p>
      <button onClick={onClose}>Close</button>
    </div>
  )
}

// Mock the ContactModal component
const MockContactModal = ({ isOpen, onClose }: any) => {
  if (!isOpen) return null
  return (
    <div data-testid="contact-modal">
      <h2>Add New Contact</h2>
      <button onClick={onClose}>Close</button>
    </div>
  )
}

// Test component that simulates the wallet detail page contact button logic
const ContactLimitTestComponent = ({ 
  userTier, 
  currentContactCount, 
  contacts = [] 
}: { 
  userTier: string
  currentContactCount: number
  contacts?: any[]
}) => {
  const [isAddContactModalOpen, setIsAddContactModalOpen] = React.useState(false)
  const [isPlansModalOpen, setIsPlansModalOpen] = React.useState(false)
  
  const user = { subscription_tier: userTier }
  
  const handleAddContact = () => {
    // Check contact limits before opening create modal
    if (user && hasReachedContactLimit(contacts?.length || currentContactCount, user.subscription_tier)) {
      setIsPlansModalOpen(true)
      return
    }
    
    setIsAddContactModalOpen(true)
  }

  return (
    <div>
      <div data-testid="contact-count">
        Current contacts: {currentContactCount}
      </div>
      <div data-testid="user-tier">
        User tier: {userTier}
      </div>
      <div data-testid="contact-limit">
        Contact limit: {getContactLimit(userTier) === null ? 'unlimited' : getContactLimit(userTier)}
      </div>
      <button onClick={handleAddContact} data-testid="add-contact-btn">
        Add Contact
      </button>
      
      <MockContactModal
        isOpen={isAddContactModalOpen}
        onClose={() => setIsAddContactModalOpen(false)}
      />
      
      <MockPlansModal
        isOpen={isPlansModalOpen}
        onClose={() => setIsPlansModalOpen(false)}
        limitType="contacts"
        currentContactCount={currentContactCount}
      />
    </div>
  )
}

describe('Contact Limit Enforcement', () => {
  describe('Subscription Limit Functions', () => {
    describe('getContactLimit', () => {
      it('returns correct limits for each tier', () => {
        expect(getContactLimit('personal')).toBe(1)
        expect(getContactLimit('pro')).toBe(10)
      })

      it('handles case insensitive tier names', () => {
        expect(getContactLimit('PERSONAL')).toBe(1)
        expect(getContactLimit('Pro')).toBe(10)
        expect(getContactLimit('BUSINESS')).toBe(null)
      })

      it('defaults to personal limit for invalid tiers', () => {
        expect(getContactLimit('invalid')).toBe(1)
        expect(getContactLimit('')).toBe(1)
      })
    })

    describe('hasReachedContactLimit', () => {
      it('returns true when personal limit is reached', () => {
        expect(hasReachedContactLimit(1, 'personal')).toBe(true)
        expect(hasReachedContactLimit(2, 'personal')).toBe(true)
      })

      it('returns false when personal limit is not reached', () => {
        expect(hasReachedContactLimit(0, 'personal')).toBe(false)
      })

      it('returns true when pro limit is reached', () => {
        expect(hasReachedContactLimit(10, 'pro')).toBe(true)
        expect(hasReachedContactLimit(15, 'pro')).toBe(true)
      })

      it('returns false when pro limit is not reached', () => {
        expect(hasReachedContactLimit(5, 'pro')).toBe(false)
        expect(hasReachedContactLimit(9, 'pro')).toBe(false)
      })

    })
  })

  describe('Personal Tier Contact Limits', () => {
    it('shows contact modal when no contacts exist', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={0} 
        />
      )

      expect(screen.getByTestId('contact-count')).toHaveTextContent('Current contacts: 0')
      expect(screen.getByTestId('contact-limit')).toHaveTextContent('Contact limit: 1')

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()
      expect(screen.queryByTestId('plans-modal')).not.toBeInTheDocument()
    })

    it('shows upgrade modal when limit is reached (1 contact)', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={1} 
        />
      )

      expect(screen.getByTestId('contact-count')).toHaveTextContent('Current contacts: 1')

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
      expect(screen.getByText('Current usage: 1 contacts')).toBeInTheDocument()
      expect(screen.getByText('Limit type: contacts')).toBeInTheDocument()
      expect(screen.queryByTestId('contact-modal')).not.toBeInTheDocument()
    })

    it('shows upgrade modal when over limit (edge case)', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={2} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
      expect(screen.queryByTestId('contact-modal')).not.toBeInTheDocument()
    })
  })

  describe('Pro Tier Contact Limits', () => {
    it('shows contact modal when under limit', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="pro" 
          currentContactCount={5} 
        />
      )

      expect(screen.getByTestId('contact-limit')).toHaveTextContent('Contact limit: 10')

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()
      expect(screen.queryByTestId('plans-modal')).not.toBeInTheDocument()
    })

    it('shows contact modal when at limit minus 1', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="pro" 
          currentContactCount={9} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()
      expect(screen.queryByTestId('plans-modal')).not.toBeInTheDocument()
    })

    it('shows upgrade modal when limit is reached (10 contacts)', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="pro" 
          currentContactCount={10} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
      expect(screen.getByText('Current usage: 10 contacts')).toBeInTheDocument()
      expect(screen.queryByTestId('contact-modal')).not.toBeInTheDocument()
    })

    it('shows upgrade modal when over limit', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="pro" 
          currentContactCount={15} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
      expect(screen.queryByTestId('contact-modal')).not.toBeInTheDocument()
    })
  })


  describe('Edge Cases', () => {
    it('handles zero contacts correctly', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={0} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()
    })

    it('handles undefined/null contact arrays', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={0}
          contacts={undefined as any}
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()
    })

    it('uses contacts array length when provided', async () => {
      const user = userEvent.setup()
      const mockContacts = [{ id: 1 }, { id: 2 }] // 2 contacts
      
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={0} // This should be ignored
          contacts={mockContacts}
        />
      )

      // Should use contacts.length (2) instead of currentContactCount (0)
      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
    })

    it('handles case insensitive tier names', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="PERSONAL" 
          currentContactCount={1} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))

      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
    })
  })

  describe('Modal Interactions', () => {
    it('can close upgrade modal', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={1} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))
      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()

      await user.click(screen.getByText('Close'))
      expect(screen.queryByTestId('plans-modal')).not.toBeInTheDocument()
    })

    it('can close contact modal', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={0} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))
      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()

      await user.click(screen.getByText('Close'))
      expect(screen.queryByTestId('contact-modal')).not.toBeInTheDocument()
    })
  })

  describe('Integration Scenarios', () => {
    it('Alice scenario: personal user with 0 contacts can add', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={0} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))
      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()
      expect(screen.queryByTestId('plans-modal')).not.toBeInTheDocument()
    })

    it('Alice scenario: personal user with 1 contact sees upgrade modal', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="personal" 
          currentContactCount={1} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))
      expect(screen.queryByTestId('contact-modal')).not.toBeInTheDocument()
      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
      expect(screen.getByText('Contact Limit Reached')).toBeInTheDocument()
    })

    it('Bob scenario: pro user with 9 contacts can add', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="pro" 
          currentContactCount={9} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))
      expect(screen.getByTestId('contact-modal')).toBeInTheDocument()
      expect(screen.queryByTestId('plans-modal')).not.toBeInTheDocument()
    })

    it('Bob scenario: pro user with 10 contacts sees upgrade modal', async () => {
      const user = userEvent.setup()
      render(
        <ContactLimitTestComponent 
          userTier="pro" 
          currentContactCount={10} 
        />
      )

      await user.click(screen.getByTestId('add-contact-btn'))
      expect(screen.queryByTestId('contact-modal')).not.toBeInTheDocument()
      expect(screen.getByTestId('plans-modal')).toBeInTheDocument()
    })

  })
})