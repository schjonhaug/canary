import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { EditWalletModal } from '../edit-wallet-modal'

// Mock libphonenumber-js
jest.mock('libphonenumber-js', () => ({
  formatNumber: jest.fn((number) => number),
  parsePhoneNumber: jest.fn((number) => ({
    isValid: () => number.startsWith('+47') && number.length >= 11,
    format: (format: string) => format === 'E.164' ? number : number,
  })),
}))

const mockWallet = {
  id: 1,
  name: 'Test Wallet',
  descriptor: 'test-descriptor',
  wallet_filename: 'test.sqlite',
  hex_color: '#ff0000',
  created_at: '2024-01-01T00:00:00Z',
  balance_total: 1000000,
  last_activity: '2024-01-01T00:00:00Z',
  contact_count: 2,
}

const mockContacts = [
  {
    id: 1,
    wallet_id: 1,
    name: 'John Doe',
    phone_number: '+4792050946',
    language: 'no' as const,
    created_at: '2024-01-01T00:00:00Z',
  },
  {
    id: 2,
    wallet_id: 1,
    name: 'Jane Smith',
    phone_number: '+4722334455',
    language: 'no' as const,
    created_at: '2024-01-01T00:00:00Z',
  },
]

// Mock fetch
global.fetch = jest.fn()

const mockFetch = global.fetch as jest.MockedFunction<typeof fetch>

describe('EditWalletModal - Contact Management', () => {
  beforeEach(() => {
    mockFetch.mockClear()
    mockFetch.mockReset()
  })

  afterEach(() => {
    jest.clearAllMocks()
  })

  const defaultProps = {
    wallet: mockWallet,
    isOpen: true,
    onClose: jest.fn(),
    onWalletUpdated: jest.fn(),
    onDeleteWallet: jest.fn(),
  }

  it('displays existing wallet contacts', async () => {
    // Mock the fetch for getting wallet contacts
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => mockContacts,
    } as Response)

    render(<EditWalletModal {...defaultProps} />)

    await waitFor(() => {
      expect(screen.getByText('John Doe')).toBeInTheDocument()
      expect(screen.getByText('Jane Smith')).toBeInTheDocument()
      expect(screen.getByText('+4792050946')).toBeInTheDocument()
      expect(screen.getByText('+4722334455')).toBeInTheDocument()
    })

    expect(screen.getByText('2 contacts')).toBeInTheDocument()
  })

  it('shows empty state when no contacts exist', async () => {
    // Mock the fetch for getting wallet contacts (empty array)
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => [],
    } as Response)

    render(<EditWalletModal {...defaultProps} />)

    await waitFor(() => {
      expect(screen.getByText('No contacts are receiving SMS notifications for this wallet.')).toBeInTheDocument()
    })

    expect(screen.getByText('0 contacts')).toBeInTheDocument()
  })

  it('allows creating a new contact with valid phone number', async () => {
    // Mock the fetch for getting wallet contacts (empty initially)
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [],
      } as Response)
      // Mock the fetch for creating contact
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          message: 'Contact created successfully',
          contact_id: 3,
        }),
      } as Response)
      // Mock the fetch for refreshing contacts after creation
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [
          {
            id: 3,
            wallet_id: 1,
            name: 'New Contact',
            phone_number: '+4798765432',
            language: 'no' as const,
            created_at: '2024-01-01T00:00:00Z',
          },
        ],
      } as Response)

    render(<EditWalletModal {...defaultProps} />)

    // Wait for initial load
    await waitFor(() => {
      expect(screen.getByText('0 contacts')).toBeInTheDocument()
    })

    // Fill in the contact form
    const nameInput = screen.getByLabelText('Name')
    const phoneInput = screen.getByLabelText('Phone Number')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    fireEvent.change(nameInput, { target: { value: 'New Contact' } })
    fireEvent.change(phoneInput, { target: { value: '+4798765432' } })

    // Create the contact
    fireEvent.click(createButton)

    await waitFor(() => {
      expect(screen.getByText('New Contact')).toBeInTheDocument()
      expect(screen.getByText('+4798765432')).toBeInTheDocument()
    })

    // Verify the API calls
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/wallets/1/contacts',
      expect.objectContaining({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: 'New Contact',
          phone_number: '+4798765432',
          language: 'no',
        }),
      })
    )
  })

  it('shows error for invalid phone number', async () => {
    // Mock the fetch for getting wallet contacts (empty initially)
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => [],
    } as Response)

    render(<EditWalletModal {...defaultProps} />)

    await waitFor(() => {
      expect(screen.getByText('0 contacts')).toBeInTheDocument()
    })

    // Fill in the contact form with invalid phone
    const nameInput = screen.getByLabelText('Name')
    const phoneInput = screen.getByLabelText('Phone Number')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    fireEvent.change(nameInput, { target: { value: 'Invalid Contact' } })
    fireEvent.change(phoneInput, { target: { value: '12345' } }) // Invalid phone

    // Try to create the contact
    fireEvent.click(createButton)

    await waitFor(() => {
      expect(screen.getByText(/Invalid phone number format/)).toBeInTheDocument()
    })

    // Verify no API call was made for creation
    expect(mockFetch).toHaveBeenCalledTimes(1) // Only the initial fetch
  })

  it('handles API error when creating contact', async () => {
    // Mock the fetch for getting wallet contacts (empty initially)
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [],
      } as Response)
      // Mock the fetch for creating contact (error)
      .mockResolvedValueOnce({
        ok: false,
        json: async () => ({
          error: 'Phone number already exists',
        }),
      } as Response)

    render(<EditWalletModal {...defaultProps} />)

    await waitFor(() => {
      expect(screen.getByText('0 contacts')).toBeInTheDocument()
    })

    // Fill in the contact form
    const nameInput = screen.getByLabelText('Name')
    const phoneInput = screen.getByLabelText('Phone Number')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    fireEvent.change(nameInput, { target: { value: 'Test Contact' } })
    fireEvent.change(phoneInput, { target: { value: '+4798765432' } })

    // Create the contact
    fireEvent.click(createButton)

    await waitFor(() => {
      expect(screen.getByText('Phone number already exists')).toBeInTheDocument()
    })
  })

  it('allows deleting a contact', async () => {
    // Mock the fetch for getting wallet contacts
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        json: async () => mockContacts,
      } as Response)
      // Mock the fetch for deleting contact
      .mockResolvedValueOnce({
        ok: true,
      } as Response)
      // Mock the fetch for refreshing contacts after deletion
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [mockContacts[1]], // Only Jane remains
      } as Response)

    render(<EditWalletModal {...defaultProps} />)

    // Wait for contacts to load
    await waitFor(() => {
      expect(screen.getByText('John Doe')).toBeInTheDocument()
      expect(screen.getByText('Jane Smith')).toBeInTheDocument()
    })

    // Find and click the delete button for John Doe
    const deleteButtons = screen.getAllByRole('button', { name: '' }) // X buttons have no text
    const johnDeleteButton = deleteButtons.find(button => 
      button.closest('[data-testid="contact-item"]')?.textContent?.includes('John Doe')
    )

    if (johnDeleteButton) {
      fireEvent.click(johnDeleteButton)
    }

    await waitFor(() => {
      expect(screen.queryByText('John Doe')).not.toBeInTheDocument()
      expect(screen.getByText('Jane Smith')).toBeInTheDocument()
    })

    // Verify the API call
    expect(mockFetch).toHaveBeenCalledWith(
      '/api/wallets/1/contacts/1',
      expect.objectContaining({
        method: 'DELETE',
      })
    )
  })

  it('disables create button when form is incomplete', async () => {
    // Mock the fetch for getting wallet contacts
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => [],
    } as Response)

    render(<EditWalletModal {...defaultProps} />)

    await waitFor(() => {
      expect(screen.getByText('0 contacts')).toBeInTheDocument()
    })

    const createButton = screen.getByRole('button', { name: /create contact/i })
    
    // Button should be disabled initially
    expect(createButton).toBeDisabled()

    // Fill in only name
    const nameInput = screen.getByLabelText('Name')
    fireEvent.change(nameInput, { target: { value: 'Test' } })
    
    // Button should still be disabled
    expect(createButton).toBeDisabled()

    // Fill in phone number
    const phoneInput = screen.getByLabelText('Phone Number')
    fireEvent.change(phoneInput, { target: { value: '+4798765432' } })
    
    // Button should now be enabled
    expect(createButton).not.toBeDisabled()
  })

  it('shows loading state while creating contact', async () => {
    // Mock the fetch for getting wallet contacts
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [],
      } as Response)
      // Mock a slow response for creating contact
      .mockImplementationOnce(() => 
        new Promise(resolve => 
          setTimeout(() => resolve({
            ok: true,
            json: async () => ({ message: 'Contact created successfully', contact_id: 3 }),
          } as Response), 100)
        )
      )
      // Mock the fetch for refreshing contacts after creation
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [
          {
            id: 3,
            wallet_id: 1,
            name: 'Test Contact',
            phone_number: '+4798765432',
            language: 'no' as const,
            created_at: '2024-01-01T00:00:00Z',
          },
        ],
      } as Response)

    render(<EditWalletModal {...defaultProps} />)

    // Wait for initial load and empty state
    await waitFor(() => {
      expect(screen.queryByText('Loading contacts...')).not.toBeInTheDocument()
    })

    // Fill in the form
    const nameInput = screen.getByLabelText('Name')
    const phoneInput = screen.getByLabelText('Phone Number')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    fireEvent.change(nameInput, { target: { value: 'Test Contact' } })
    fireEvent.change(phoneInput, { target: { value: '+4798765432' } })

    // Click create
    fireEvent.click(createButton)

    // Check loading state
    expect(screen.getByText('Creating...')).toBeInTheDocument()
    expect(createButton).toBeDisabled()
  })

  it('formats phone numbers for display', async () => {
    // Mock the fetch for getting wallet contacts
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => [mockContacts[0]],
    } as Response)

    render(<EditWalletModal {...defaultProps} />)

    // Wait for contacts to load - first make sure loading is finished
    await waitFor(() => {
      expect(screen.queryByText('Loading contacts...')).not.toBeInTheDocument()
    }, { timeout: 3000 })

    // Check that we have the contact count and details
    expect(screen.getByText('1 contact')).toBeInTheDocument()
    expect(screen.getByText('John Doe')).toBeInTheDocument()
    expect(screen.getByText('+4792050946')).toBeInTheDocument()

    // Verify the fetch was called with correct URL
    expect(mockFetch).toHaveBeenCalledWith('/api/wallets/1/contacts')
    
    // Verify formatNumber was called
    const { formatNumber } = await import('libphonenumber-js')
    expect(formatNumber).toHaveBeenCalledWith('+4792050946', 'INTERNATIONAL')
  })
})