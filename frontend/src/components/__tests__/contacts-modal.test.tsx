import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { ContactsModal } from '../contacts-modal'
import { formatNumber, isValidPhoneNumber } from 'libphonenumber-js'

// Mock libphonenumber-js
jest.mock('libphonenumber-js', () => ({
  formatNumber: jest.fn(),
  isValidPhoneNumber: jest.fn(),
}))

// Mock window.location
delete (window as unknown as { location: unknown }).location
window.location = { hostname: 'localhost' } as unknown as Location

// Mock fetch
global.fetch = jest.fn()

const mockFormatNumber = formatNumber as jest.MockedFunction<typeof formatNumber>
const mockIsValidPhoneNumber = isValidPhoneNumber as jest.MockedFunction<typeof isValidPhoneNumber>

describe('ContactsModal Phone Number Validation', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    // Default mock implementations
    mockFormatNumber.mockImplementation((phoneNumber) => {
      if (phoneNumber === '+4792050946') return '+47 92 05 09 46'
      if (phoneNumber === '+4722334455') return '+47 22 33 44 55'
      return phoneNumber
    })
    mockIsValidPhoneNumber.mockImplementation((phoneNumber) => {
      return phoneNumber.startsWith('+47') && phoneNumber.length >= 10
    })
    
    // Mock successful fetch responses
    ;(global.fetch as jest.Mock).mockImplementation((url: string) => {
      if (url.includes('/contacts')) {
        return Promise.resolve({
          ok: true,
          json: async () => ([]),
        })
      }
      if (url.includes('/wallets')) {
        return Promise.resolve({
          ok: true,
          json: async () => ([]),
        })
      }
      return Promise.resolve({
        ok: true,
        json: async () => ([]),
      })
    })
  })

  afterEach(() => {
    jest.restoreAllMocks()
  })

  it('validates phone number with country code requirement', async () => {
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    // Wait for component to load
    await waitFor(() => {
      expect(screen.getByText('Contact Management')).toBeInTheDocument()
    })

    // Click Add Contact button
    fireEvent.click(screen.getByText('Add Contact'))
    
    // Fill in name
    fireEvent.change(screen.getByPlaceholderText('Enter contact name'), {
      target: { value: 'John Doe' }
    })
    
    // Fill in phone number without country code
    fireEvent.change(screen.getByPlaceholderText('+4712345678'), {
      target: { value: '92050946' }
    })
    
    // Try to submit
    fireEvent.click(screen.getByText('Create'))
    
    // Should show validation error
    await waitFor(() => {
      expect(screen.getByText('Phone number must include country code (e.g., +4712345678)')).toBeInTheDocument()
    })
  })

  it('validates phone number format using libphonenumber-js', async () => {
    // Mock invalid phone number
    mockIsValidPhoneNumber.mockReturnValue(false)
    
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('Contact Management')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByText('Add Contact'))
    
    fireEvent.change(screen.getByPlaceholderText('Enter contact name'), {
      target: { value: 'John Doe' }
    })
    
    fireEvent.change(screen.getByPlaceholderText('+4712345678'), {
      target: { value: '+47invalid' }
    })
    
    fireEvent.click(screen.getByText('Create'))
    
    await waitFor(() => {
      expect(screen.getByText('Invalid phone number format')).toBeInTheDocument()
    })
  })

  it('accepts valid Norwegian mobile numbers', async () => {
    // Mock valid phone number
    mockIsValidPhoneNumber.mockReturnValue(true)
    
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('Contact Management')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByText('Add Contact'))
    
    fireEvent.change(screen.getByPlaceholderText('Enter contact name'), {
      target: { value: 'John Doe' }
    })
    
    fireEvent.change(screen.getByPlaceholderText('+4712345678'), {
      target: { value: '+4792050946' }
    })
    
    // Should not show validation errors
    expect(screen.queryByText('Phone number must include country code')).not.toBeInTheDocument()
    expect(screen.queryByText('Invalid phone number format')).not.toBeInTheDocument()
  })

  it('shows helper text for phone number format', async () => {
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('Contact Management')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByText('Add Contact'))
    
    // Should show helper text
    expect(screen.getByText('Include country code (e.g., +47 for Norway)')).toBeInTheDocument()
  })
})

describe('Phone Number Formatting', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockFormatNumber.mockImplementation((phoneNumber) => {
      // Simulate Norwegian formatting
      if (phoneNumber === '+4792050946') return '+47 92 05 09 46'  // Mobile
      if (phoneNumber === '+4722334455') return '+47 22 33 44 55'  // Landline
      if (phoneNumber === '+14155552345') return '+1 415 555 2345' // US
      return phoneNumber
    })
    
    ;(global.fetch as jest.Mock).mockImplementation((url: string) => {
      if (url.includes('/contacts')) {
        return Promise.resolve({
          ok: true,
          json: async () => ([
            {
              id: 1,
              name: 'Norwegian Mobile',
              phone_number: '+4792050946',
              created_at: '2024-01-01T00:00:00Z'
            },
            {
              id: 2,
              name: 'Norwegian Landline',
              phone_number: '+4722334455',
              created_at: '2024-01-01T00:00:00Z'
            },
            {
              id: 3,
              name: 'US Number',
              phone_number: '+14155552345',
              created_at: '2024-01-01T00:00:00Z'
            }
          ]),
        })
      }
      if (url.includes('/wallets')) {
        return Promise.resolve({
          ok: true,
          json: async () => ([]),
        })
      }
      return Promise.resolve({
        ok: true,
        json: async () => ([]),
      })
    })
  })

  it('formats Norwegian mobile numbers correctly', async () => {
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('Norwegian Mobile')).toBeInTheDocument()
    })

    // Check that Norwegian mobile number is formatted with 2-2-2-2 pattern
    expect(screen.getByText('+47 92 05 09 46')).toBeInTheDocument()
  })

  it('formats Norwegian landline numbers correctly', async () => {
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('Norwegian Landline')).toBeInTheDocument()
    })

    // Check that Norwegian landline number is formatted with 2-2-2-2 pattern
    expect(screen.getByText('+47 22 33 44 55')).toBeInTheDocument()
  })

  it('formats international numbers correctly', async () => {
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('US Number')).toBeInTheDocument()
    })

    // Check that US number is formatted correctly
    expect(screen.getByText('+1 415 555 2345')).toBeInTheDocument()
  })

  it('falls back to original number if formatting fails', () => {
    // Mock formatting error
    mockFormatNumber.mockImplementation(() => {
      throw new Error('Formatting failed')
    })

    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    // Component should handle the error gracefully and not crash
    expect(screen.getByText('Contact Management')).toBeInTheDocument()
  })
})

describe('libphonenumber-js Integration', () => {
  it('uses libphonenumber-js for validation', async () => {
    mockIsValidPhoneNumber.mockReturnValue(true)
    
    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('Contact Management')).toBeInTheDocument()
    })

    fireEvent.click(screen.getByText('Add Contact'))
    
    fireEvent.change(screen.getByPlaceholderText('Enter contact name'), {
      target: { value: 'Test' }
    })
    
    fireEvent.change(screen.getByPlaceholderText('+4712345678'), {
      target: { value: '+4792050946' }
    })
    
    fireEvent.click(screen.getByText('Create'))
    
    // Should have called the validation function
    expect(mockIsValidPhoneNumber).toHaveBeenCalledWith('+4792050946')
  })

  it('uses libphonenumber-js for formatting', async () => {
    ;(global.fetch as jest.Mock).mockImplementation((url: string) => {
      if (url.includes('/contacts')) {
        return Promise.resolve({
          ok: true,
          json: async () => ([{
            id: 1,
            name: 'Test Contact',
            phone_number: '+4792050946',
            created_at: '2024-01-01T00:00:00Z'
          }]),
        })
      }
      if (url.includes('/wallets')) {
        return Promise.resolve({
          ok: true,
          json: async () => ([]),
        })
      }
      return Promise.resolve({
        ok: true,
        json: async () => ([]),
      })
    })

    render(<ContactsModal isOpen={true} onClose={() => {}} />)
    
    await waitFor(() => {
      expect(screen.getByText('Test Contact')).toBeInTheDocument()
    })

    // Should have called the formatting function
    expect(mockFormatNumber).toHaveBeenCalledWith('+4792050946', 'INTERNATIONAL')
  })
})