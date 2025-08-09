import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ContactModal } from '../contact-modal'

// Mock the api module
jest.mock('../../lib/api', () => ({
  api: {
    getProviders: jest.fn(),
    sendContactVerification: jest.fn(),
    verifyContact: jest.fn(),
    createContact: jest.fn(),
    deleteContact: jest.fn(),
  },
}))

const mockApi = jest.requireMock('../../lib/api').api

const mockProviders = [
  {
    name: 'ntfy',
    display_name: 'Push Notifications',
    config_schema: {},
  },
  {
    name: 'twilio',
    display_name: 'SMS Notifications',
    config_schema: {},
  },
  {
    name: 'email',
    display_name: 'Email Notifications',
    config_schema: {},
  },
]

const mockContact = {
  id: 1,
  name: 'Test Contact',
  language: 'en' as const,
  created_at: '2024-01-01T00:00:00Z',
  notification_methods: [
    {
      id: 1,
      provider_type: 'sms' as const,
      notification_target: '+4799999999',
      display_target: '+4799999999',
      verified: true,
      created_at: '2024-01-01T00:00:00Z',
    },
  ],
}

describe('ContactModal', () => {
  const defaultProps = {
    isOpen: true,
    onClose: jest.fn(),
    walletChecksum: 'test-checksum',
    onContactSaved: jest.fn(),
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getProviders.mockResolvedValue({ providers: mockProviders })
    mockApi.sendContactVerification.mockResolvedValue({ message: 'Verification sent' })
    mockApi.verifyContact.mockResolvedValue({ valid: true, message: 'Verified' })
    mockApi.createContact.mockResolvedValue({ id: 1 })
  })

  describe('Basic Modal Behavior', () => {
    it('renders modal when open', async () => {
      render(<ContactModal {...defaultProps} />)
      
      expect(screen.getByText('Add New Contact')).toBeInTheDocument()
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
      expect(screen.getByLabelText('Language')).toBeInTheDocument()
    })

    it('does not render when closed', () => {
      render(<ContactModal {...defaultProps} isOpen={false} />)
      
      expect(screen.queryByText('Add New Contact')).not.toBeInTheDocument()
    })

    it('renders edit mode title when editing contact', async () => {
      render(<ContactModal {...defaultProps} editContact={mockContact} />)
      
      expect(screen.getByText('Edit Contact')).toBeInTheDocument()
    })

    it('calls onClose when cancel button is clicked', async () => {
      render(<ContactModal {...defaultProps} />)
      
      fireEvent.click(screen.getByText('Cancel'))
      expect(defaultProps.onClose).toHaveBeenCalled()
    })
  })

  describe('Provider Loading', () => {
    it('loads providers when modal opens', async () => {
      render(<ContactModal {...defaultProps} />)
      
      await waitFor(() => {
        expect(mockApi.getProviders).toHaveBeenCalled()
      })

      expect(screen.getByText('Push Notifications')).toBeInTheDocument()
      expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      expect(screen.getByText('Email Notifications')).toBeInTheDocument()
    })
  })

  describe('New Contact Creation', () => {
    it('allows creating contact with ntfy only', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('Push Notifications')).toBeInTheDocument()
      })

      // Fill in name and select ntfy
      await user.type(screen.getByLabelText('Name'), 'Test Contact')
      await user.click(screen.getByRole('checkbox', { name: /Push Notifications/ }))

      // Submit
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(mockApi.createContact).toHaveBeenCalledWith(
          'test-checksum',
          'Test Contact',
          'en',
          [{ provider_type: 'ntfy', notification_target: '' }]
        )
      })
    })

    it('shows validation error when name is empty', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('Push Notifications')).toBeInTheDocument()
      })

      await user.click(screen.getByRole('checkbox', { name: /Push Notifications/ }))
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(screen.getByText('Contact name is required')).toBeInTheDocument()
      })
    })
  })

  describe('SMS Verification Flow', () => {
    it('shows SMS verification button when phone number is entered', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      // Phone input should appear
      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, '+4712345678')

      // Verification button should appear
      expect(screen.getByText('Send Verification Code')).toBeInTheDocument()
    })

    it('sends SMS verification and shows code input', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, '+4712345678')

      // Send verification
      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(mockApi.sendContactVerification).toHaveBeenCalledWith(
          'test-checksum',
          'SMS Contact',
          'en',
          '+4712345678',
          undefined
        )
      })

      // Verification code input should appear
      expect(screen.getByLabelText('Verification Code')).toBeInTheDocument()
      expect(screen.getByText('Code sent to +4712345678')).toBeInTheDocument()
    })

    it('verifies SMS code successfully', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, '+4712345678')

      // Send and verify
      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByLabelText('Verification Code')).toBeInTheDocument()
      })

      const codeInput = screen.getByLabelText('Verification Code')
      await user.type(codeInput, '123456')
      await user.click(screen.getByText('Verify'))

      await waitFor(() => {
        expect(mockApi.verifyContact).toHaveBeenCalledWith(
          'test-checksum',
          '123456',
          '+4712345678',
          undefined
        )
      })

      expect(screen.getByText('SMS verified successfully')).toBeInTheDocument()
    })

    it('shows SMS verification error', async () => {
      const user = userEvent.setup()
      mockApi.verifyContact.mockResolvedValue({ valid: false, message: 'Invalid code' })

      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, '+4712345678')

      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByLabelText('Verification Code')).toBeInTheDocument()
      })

      const codeInput = screen.getByLabelText('Verification Code')
      await user.type(codeInput, '000000')
      await user.click(screen.getByText('Verify'))

      await waitFor(() => {
        expect(screen.getByText('Invalid code')).toBeInTheDocument()
      })
    })

    it('prevents submission without SMS verification', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, '+4712345678')

      // Try to submit without verifying
      await user.click(screen.getByText('Create Contact'))

      expect(screen.getByText('Please verify the SMS code before saving the contact')).toBeInTheDocument()
    })
  })

  describe('Email Verification Flow', () => {
    it('shows email verification button when email is entered', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'Email Contact')
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      const emailInput = screen.getByPlaceholderText('user@example.com')
      await user.type(emailInput, 'test@example.com')

      expect(screen.getByText('Send Verification Code')).toBeInTheDocument()
    })

    it('handles auto-verified email (user\'s own email)', async () => {
      const user = userEvent.setup()
      mockApi.sendContactVerification.mockResolvedValue({ 
        message: 'Auto-verified', 
        auto_verified: true 
      })

      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'Email Contact')
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      const emailInput = screen.getByPlaceholderText('user@example.com')
      await user.type(emailInput, 'user@example.com')

      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByText('Email verified successfully')).toBeInTheDocument()
      })

      // Should not show OTP input for auto-verified
      expect(screen.queryByLabelText('Verification Code')).not.toBeInTheDocument()
    })

    it('verifies email code successfully', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'Email Contact')
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      const emailInput = screen.getByPlaceholderText('user@example.com')
      await user.type(emailInput, 'test@example.com')

      // Send verification
      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByLabelText('Verification Code')).toBeInTheDocument()
      })

      const codeInput = screen.getByLabelText('Verification Code')
      await user.type(codeInput, '654321')
      await user.click(screen.getByText('Verify'))

      await waitFor(() => {
        expect(mockApi.verifyContact).toHaveBeenCalledWith(
          'test-checksum',
          '654321',
          undefined,
          'test@example.com'
        )
      })

      expect(screen.getByText('Email verified successfully')).toBeInTheDocument()
    })
  })

  describe('Edit Contact Mode', () => {
    it('loads existing contact data in edit mode', async () => {
      render(<ContactModal {...defaultProps} editContact={mockContact} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      // Should be populated with existing data
      const nameInput = screen.getByLabelText('Name') as HTMLInputElement
      expect(nameInput.value).toBe('Test Contact')

      // SMS should be checked and phone number filled
      expect(screen.getByRole('checkbox', { name: /SMS Notifications/ })).toBeChecked()
      const phoneInput = screen.getByPlaceholderText('+1234567890') as HTMLInputElement
      expect(phoneInput.value).toBe('+4799999999')

      // Should show as already verified (no verification button)
      expect(screen.queryByText('Send Verification Code')).not.toBeInTheDocument()
    })

    it('requires verification when phone number changes in edit mode', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} editContact={mockContact} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      // Change phone number
      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.clear(phoneInput)
      await user.type(phoneInput, '+4788888888')

      // Verification button should appear
      expect(screen.getByText('Send Verification Code')).toBeInTheDocument()
    })

    it('reverts to verified state when phone number reverts to original', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} editContact={mockContact} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      const phoneInput = screen.getByPlaceholderText('+1234567890')
      
      // Change phone number
      await user.clear(phoneInput)
      await user.type(phoneInput, '+4788888888')
      expect(screen.getByText('Send Verification Code')).toBeInTheDocument()

      // Revert to original
      await user.clear(phoneInput)
      await user.type(phoneInput, '+4799999999')
      
      // Should be verified again
      expect(screen.queryByText('Send Verification Code')).not.toBeInTheDocument()
    })
  })

  describe('Multiple Provider Support', () => {
    it('allows creating contact with multiple verified providers', async () => {
      const user = userEvent.setup()
      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('Push Notifications')).toBeInTheDocument()
      })

      // Fill name
      await user.type(screen.getByLabelText('Name'), 'Multi Contact')

      // Enable ntfy (no verification needed)
      await user.click(screen.getByRole('checkbox', { name: /Push Notifications/ }))

      // Enable and verify SMS
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))
      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, '+4712345678')
      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByLabelText('Verification Code')).toBeInTheDocument()
      })

      const smsCodeInput = screen.getByLabelText('Verification Code')
      await user.type(smsCodeInput, '123456')
      await user.click(screen.getByText('Verify'))

      await waitFor(() => {
        expect(screen.getByText('SMS verified successfully')).toBeInTheDocument()
      })

      // Enable and verify email (auto-verified)
      mockApi.sendContactVerification.mockResolvedValueOnce({ 
        message: 'Auto-verified', 
        auto_verified: true 
      })
      
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))
      const emailInput = screen.getByPlaceholderText('user@example.com')
      await user.type(emailInput, 'user@example.com')
      await user.click(screen.getAllByText('Send Verification Code')[1]) // Second button for email

      await waitFor(() => {
        expect(screen.getByText('Email verified successfully')).toBeInTheDocument()
      })

      // Submit
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(mockApi.createContact).toHaveBeenCalledWith(
          'test-checksum',
          'Multi Contact',
          'en',
          [
            { provider_type: 'ntfy', notification_target: '' },
            { provider_type: 'email', notification_target: 'user@example.com' },
            { provider_type: 'sms', notification_target: '+4712345678' }
          ]
        )
      })
    })
  })

  describe('Error Handling', () => {
    it('shows phone number validation errors under phone input', async () => {
      const user = userEvent.setup()
      mockApi.sendContactVerification.mockRejectedValue(new Error('Invalid phone number format'))

      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, 'invalid')
      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByText('Invalid phone number format')).toBeInTheDocument()
      })

      // Error should appear under phone input, not at top
      const phoneSection = phoneInput.closest('div')
      expect(phoneSection).toContainElement(screen.getByText('Invalid phone number format'))
    })

    it('shows email validation errors under email input', async () => {
      const user = userEvent.setup()
      mockApi.sendContactVerification.mockRejectedValue(new Error('Invalid email address'))

      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'Email Contact')
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      const emailInput = screen.getByPlaceholderText('user@example.com')
      await user.type(emailInput, 'invalid-email')
      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByText('Invalid email address')).toBeInTheDocument()
      })
    })

    it('clears errors when user starts typing', async () => {
      const user = userEvent.setup()
      mockApi.sendContactVerification.mockRejectedValue(new Error('Invalid phone number format'))

      render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, 'invalid')
      await user.click(screen.getByText('Send Verification Code'))

      await waitFor(() => {
        expect(screen.getByText('Invalid phone number format')).toBeInTheDocument()
      })

      // Start typing - error should clear
      mockApi.sendContactVerification.mockClear()
      await user.clear(phoneInput)
      await user.type(phoneInput, '+47')

      expect(screen.queryByText('Invalid phone number format')).not.toBeInTheDocument()
    })
  })

  describe('State Management', () => {
    it('cleans up state when modal closes', async () => {
      const { rerender } = render(<ContactModal {...defaultProps} />)

      await waitFor(() => {
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      const user = userEvent.setup()
      
      // Set up some state
      await user.type(screen.getByLabelText('Name'), 'Test')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))
      const phoneInput = screen.getByPlaceholderText('+1234567890')
      await user.type(phoneInput, '+4712345678')

      // Close modal
      rerender(<ContactModal {...defaultProps} isOpen={false} />)

      // Reopen
      rerender(<ContactModal {...defaultProps} isOpen={true} />)

      // Should be clean state
      await waitFor(() => {
        expect(screen.getByLabelText('Name')).toBeInTheDocument()
      })
      
      const nameInput = screen.getByLabelText('Name') as HTMLInputElement
      expect(nameInput.value).toBe('')
      expect(screen.getByRole('checkbox', { name: /SMS Notifications/ })).not.toBeChecked()
    })
  })
})