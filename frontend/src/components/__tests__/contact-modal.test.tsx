import React from 'react'
import { render, screen, fireEvent, waitFor, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ContactModal } from '../contact-modal'

// Mock the usePhonePlaceholder hook to return a consistent value
jest.mock('../../hooks/usePhonePlaceholder', () => ({
  usePhonePlaceholder: () => '+1 234 567 8900',
}))

// Mock the api module but keep ApiError from the real module
jest.mock('../../lib/api', () => {
  const actual = jest.requireActual('../../lib/api')
  return {
    ApiError: actual.ApiError,
    api: {
      getProviders: jest.fn(),
      sendContactVerification: jest.fn(),
      verifyContact: jest.fn(),
      createContact: jest.fn(),
      updateContact: jest.fn(),
      deleteContact: jest.fn(),
      getUserPreferences: jest.fn(),
    },
  }
})

const mockApi = jest.requireMock('../../lib/api').api
const { ApiError } = jest.requireMock('../../lib/api')

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
    mockApi.getUserPreferences.mockResolvedValue({
      preferred_fiat_currency: 'USD',
      ntfy_server_url: null,
      ntfy_has_access_token: false,
      ntfy_has_credentials: false,
      ntfy_username: null,
    })
  })

  describe('Basic Modal Behavior', () => {
    it('renders modal when open', async () => {
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      expect(screen.getByText('Add New Contact')).toBeInTheDocument()
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
    })

    it('does not render when closed', () => {
      render(<ContactModal {...defaultProps} isOpen={false} />)
      
      expect(screen.queryByText('Add New Contact')).not.toBeInTheDocument()
    })

    it('renders edit mode title when editing contact', async () => {
      await act(async () => {
        render(<ContactModal {...defaultProps} editContact={mockContact} />)
      })
      
      expect(screen.getByText('Edit Contact')).toBeInTheDocument()
    })

    it('calls onClose when X button is clicked', async () => {
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })
      
      // Find the close button by its data attribute
      fireEvent.click(screen.getByRole('button', { name: 'Close' }))
      expect(defaultProps.onClose).toHaveBeenCalled()
    })
  })

  describe('Provider Loading', () => {
    it('loads providers when modal opens', async () => {
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(mockApi.getProviders).toHaveBeenCalled()
      })

      // Provider names come from translations: add.providers.ntfy, .twilio, .email
      expect(screen.getByText('ntfy.sh Notifications')).toBeInTheDocument()
      expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      expect(screen.getByText('Email Notifications')).toBeInTheDocument()
    })
  })

  describe('New Contact Creation', () => {
    it('allows creating contact with ntfy only', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('ntfy.sh Notifications')).toBeInTheDocument()
      })

      // Fill in name and select ntfy
      await user.type(screen.getByLabelText('Name'), 'Test Contact')
      await user.click(screen.getByRole('checkbox', { name: /ntfy\.sh Notifications/ }))

      // Submit
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(mockApi.createContact).toHaveBeenCalledWith(
          'test-checksum',
          'Test Contact',
          [{ provider_type: 'ntfy', notification_target: 'test-contact-test-che' }]
        )
      })
    })

    it('shows validation error when name is empty', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('ntfy.sh Notifications')).toBeInTheDocument()
      })

      await user.click(screen.getByRole('checkbox', { name: /ntfy\.sh Notifications/ }))
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(screen.getByText('Contact name is required')).toBeInTheDocument()
      })
    })
  })

  describe('SMS Verification Flow', () => {
    it('shows SMS verification button when phone number is entered', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      // Phone input should appear
      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
      await user.type(phoneInput, '+4712345678')

      // Verification button should appear
      expect(screen.getByText('Verify')).toBeInTheDocument()
    })

    it('sends SMS verification and shows code input', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
      await user.type(phoneInput, '+4712345678')

      // Send verification
      await user.click(screen.getByText('Verify'))

      await waitFor(() => {
        expect(mockApi.sendContactVerification).toHaveBeenCalledWith(
          'test-checksum',
          'SMS Contact',
          '+4712345678',
          undefined
        )
      })

      // Verification code input should appear
      expect(screen.getByLabelText('Verification Code')).toBeInTheDocument()
      expect(screen.getByText('Code sent to +47 12345678')).toBeInTheDocument()
    })

    it('verifies SMS code successfully', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
      await user.type(phoneInput, '+4712345678')

      // Send and verify
      await user.click(screen.getByText('Verify'))

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

      await waitFor(() => {
        expect(screen.getAllByText((content, element) => {
          return element?.textContent?.includes('SMS verified successfully') ?? false
        })[0]).toBeInTheDocument()
      })
    })

    it('shows SMS verification error', async () => {
      const user = userEvent.setup()
      mockApi.verifyContact.mockResolvedValue({ valid: false, message: 'Invalid code' })

      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
      await user.type(phoneInput, '+4712345678')

      await user.click(screen.getByText('Verify'))

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
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
      await user.type(phoneInput, '+4712345678')

      // Try to submit without verifying
      await user.click(screen.getByText('Create Contact'))

      expect(screen.getByText('Please verify the SMS code before saving the contact')).toBeInTheDocument()
    })
  })

  describe('Email Verification Flow', () => {
    it('shows email verification button when email is entered', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'Email Contact')
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      const emailInput = screen.getByPlaceholderText('your@email.com')
      await user.type(emailInput, 'test@example.com')

      expect(screen.getByText('Verify')).toBeInTheDocument()
    })

    it('handles auto-verified email (user\'s own email)', async () => {
      const user = userEvent.setup()
      mockApi.sendContactVerification.mockResolvedValue({ 
        message: 'Auto-verified', 
        auto_verified: true 
      })

      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'Email Contact')
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      const emailInput = screen.getByPlaceholderText('your@email.com')
      await user.type(emailInput, 'your@email.com')

      await user.click(screen.getByText('Verify'))

      await waitFor(() => {
        expect(screen.getAllByText((content, element) => {
          return element?.textContent?.includes('Email verified successfully') ?? false
        })[0]).toBeInTheDocument()
      })

      // Should not show OTP input for auto-verified
      expect(screen.queryByLabelText('Verification Code')).not.toBeInTheDocument()
    })

    it('verifies email code successfully', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'Email Contact')
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      const emailInput = screen.getByPlaceholderText('your@email.com')
      await user.type(emailInput, 'test@example.com')

      // Send verification
      await user.click(screen.getByText('Verify'))

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
      await act(async () => {
        render(<ContactModal {...defaultProps} editContact={mockContact} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      // Should be populated with existing data
      const nameInput = screen.getByLabelText('Name') as HTMLInputElement
      expect(nameInput.value).toBe('Test Contact')

      // SMS should be checked and phone number filled
      expect(screen.getByRole('checkbox', { name: /SMS Notifications/ })).toBeChecked()
      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900') as HTMLInputElement
      expect(phoneInput.value).toBe('+4799999999')

      // Should show as already verified (no verification button)
      expect(screen.queryByText('Verify')).not.toBeInTheDocument()
    })

    it('requires verification when phone number changes in edit mode', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} editContact={mockContact} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      // Change phone number
      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
      await user.clear(phoneInput)
      await user.type(phoneInput, '+4788888888')

      // Verification button should appear
      expect(screen.getByText('Verify')).toBeInTheDocument()
    })

    it('reverts to verified state when phone number reverts to original', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} editContact={mockContact} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')

      // Change phone number
      await user.clear(phoneInput)
      await user.type(phoneInput, '+4788888888')
      expect(screen.getByText('Verify')).toBeInTheDocument()

      // Revert to original
      await user.clear(phoneInput)
      await user.type(phoneInput, '+4799999999')

      // Should be verified again
      expect(screen.queryByText('Verify')).not.toBeInTheDocument()
    })

    it('preserves unchanged email when editing contact name', async () => {
      // Contact with email
      const contactWithEmail = {
        id: 2,
        name: 'Email Contact',
        created_at: '2024-01-01T00:00:00Z',
        notification_methods: [
          {
            id: 2,
            provider_type: 'email' as const,
            notification_target: 'test@example.com',
            display_target: 'test@example.com',
            verified: true,
            created_at: '2024-01-01T00:00:00Z',
          },
        ],
      }

      const user = userEvent.setup()
      mockApi.updateContact.mockResolvedValue({ id: 2 })

      await act(async () => {
        render(<ContactModal {...defaultProps} editContact={contactWithEmail} />)
      })

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      // Change only the name (email unchanged)
      const nameInput = screen.getByLabelText('Name')
      await user.clear(nameInput)
      await user.type(nameInput, 'Updated Name')

      // Submit the form
      await user.click(screen.getByRole('button', { name: /Update Contact/i }))

      // Check that updateContact was called with the unchanged email preserved
      await waitFor(() => {
        expect(mockApi.updateContact).toHaveBeenCalledWith(
          'test-checksum',
          2,
          'Updated Name',
          expect.arrayContaining([
            expect.objectContaining({ provider_type: 'email', notification_target: 'test@example.com' }),
          ])
        )
      })
    })
  })

  describe('Multiple Provider Support', () => {
    it('allows enabling multiple notification providers', async () => {
      const user = userEvent.setup()
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('ntfy.sh Notifications')).toBeInTheDocument()
      })

      // Enable multiple providers
      await user.click(screen.getByRole('checkbox', { name: /ntfy\.sh Notifications/ }))
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))
      await user.click(screen.getByRole('checkbox', { name: /Email Notifications/ }))

      // Verify all provider sections are visible
      expect(screen.getByPlaceholderText('+1 234 567 8900')).toBeInTheDocument()
      expect(screen.getByPlaceholderText('your@email.com')).toBeInTheDocument()
      expect(screen.getAllByText('Verify')).toHaveLength(2) // SMS and Email verification buttons
    })
  })

  describe('Error Handling', () => {
    it('shows validation error when trying to create contact without name', async () => {
      const user = userEvent.setup()

      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('ntfy.sh Notifications')).toBeInTheDocument()
      })

      await user.click(screen.getByRole('checkbox', { name: /ntfy\.sh Notifications/ }))
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(screen.getByText('Contact name is required')).toBeInTheDocument()
      })
    })

    it('clears errors when user starts typing', async () => {
      const user = userEvent.setup()
      mockApi.sendContactVerification.mockRejectedValue(new ApiError('Invalid phone number format', 'validation', 400, 'invalid_phone_number'))

      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      await user.type(screen.getByLabelText('Name'), 'SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))

      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
      await user.type(phoneInput, 'invalid')
      await user.click(screen.getByText('Verify'))

      await waitFor(() => {
        expect(screen.getAllByText((content, element) => {
          return element?.textContent?.includes('Invalid phone number format') ?? false
        })[0]).toBeInTheDocument()
      })

      // Start typing - error should clear
      mockApi.sendContactVerification.mockClear()
      await user.clear(phoneInput)
      await user.type(phoneInput, '+47')

      expect(screen.queryByText((content, element) => {
        return element?.textContent?.includes('Invalid phone number format') ?? false
      })).not.toBeInTheDocument()
    })
  })

  describe('State Management', () => {
    it('cleans up state when modal closes', async () => {
      let renderResult
      await act(async () => {
        renderResult = render(<ContactModal {...defaultProps} />)
      })
      const { rerender } = renderResult

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      const user = userEvent.setup()
      
      // Set up some state
      await user.type(screen.getByLabelText('Name'), 'Test')
      await user.click(screen.getByRole('checkbox', { name: /SMS Notifications/ }))
      const phoneInput = screen.getByPlaceholderText('+1 234 567 8900')
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

    it('displays duplicate email error from API', async () => {
      const user = userEvent.setup()
      
      // Mock email verification success - using same pattern as existing tests  
      mockApi.sendContactVerification.mockResolvedValue({ 
        message: 'Email verified automatically for user accounts' 
      })
      
      // Mock createContact to return duplicate conflict error
      mockApi.createContact.mockRejectedValue(new ApiError("Duplicate notification targets: Email 'your@email.com' is already used by contact 'John'", 'conflict', 409, 'duplicate_notification_targets'))
      
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        expect(screen.getByText('Email Notifications')).toBeInTheDocument()
      })

      // Fill in contact details - only enable ntfy to simplify test
      await user.type(screen.getByLabelText('Name'), 'Duplicate Contact')
      await user.click(screen.getByRole('checkbox', { name: /ntfy\.sh Notifications/ }))

      // Try to create contact - should fail with duplicate error
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(screen.getByText(/already in use/)).toBeInTheDocument()
      })

      // Modal should still be open
      expect(screen.getByText('Create Contact')).toBeInTheDocument()
    })

    it('displays duplicate phone error from API', async () => {
      const user = userEvent.setup()

      // Mock createContact to return duplicate conflict error
      mockApi.createContact.mockRejectedValue(new ApiError("Duplicate notification targets: Phone number '+4712345678' is already used by contact 'Alice'", 'conflict', 409, 'duplicate_notification_targets'))
      
      await act(async () => {
        render(<ContactModal {...defaultProps} />)
      })

      await waitFor(() => {
        // SMS Notifications comes from translation add.providers.twilio
        expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
      })

      // Fill in contact details - only enable ntfy to simplify test
      await user.type(screen.getByLabelText('Name'), 'Duplicate SMS Contact')
      await user.click(screen.getByRole('checkbox', { name: /ntfy\.sh Notifications/ }))

      // Try to create contact - should fail with duplicate error
      await user.click(screen.getByText('Create Contact'))

      await waitFor(() => {
        expect(screen.getByText(/already in use/)).toBeInTheDocument()
      })

      // Modal should still be open
      expect(screen.getByText('Create Contact')).toBeInTheDocument()
    })
  })
})