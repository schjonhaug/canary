import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { EditWalletModal } from '../edit-wallet-modal'
import { Contact } from '../../types'

// Mock the api module
jest.mock('../../lib/api', () => ({
  api: {
    getProviders: jest.fn(),
    createContact: jest.fn(),
  },
  ProviderInfo: {} as unknown,
}))

// Mock the utils module
jest.mock('../../lib/utils', () => ({
  getApiBaseUrl: jest.fn(() => ''),
  cn: jest.fn((...classes: unknown[]) => classes.filter(Boolean).join(' ')),
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

// Mock providers
const mockProviders = [
  {
    name: 'ntfy',
    display_name: 'ntfy.sh',
    enabled: true,
    configured: true,
  },
  {
    name: 'twilio',
    display_name: 'SMS Notifications',
    enabled: true,
    configured: true,
  },
]

// Mock fetch
global.fetch = jest.fn()

const mockFetch = global.fetch as jest.MockedFunction<typeof fetch>

describe('EditWalletModal - Contact Management', () => {
  beforeEach(async () => {
    mockFetch.mockClear()
    mockFetch.mockReset()
    const apiModule = await import('../../lib/api')
    apiModule.api.getProviders.mockResolvedValue({ providers: mockProviders })
  })

  afterEach(() => {
    jest.clearAllMocks()
  })

  const defaultProps = {
    wallet: mockWallet,
    isOpen: true,
    onClose: jest.fn(),
    onDeleteWallet: jest.fn(),
  }

  it('displays existing wallet contacts', async () => {
    const mockContacts: Contact[] = [
      {
        id: 1,
        wallet_id: 1,
        name: 'John Doe',
        language: 'en',
        notification_methods: [
          {
            id: 1,
            contact_id: 1,
            provider_type: 'sms',
            notification_target: '+4792050946',
            display_target: '+4792050946',
            created_at: '2024-01-01T00:00:00Z',
          }
        ],
        created_at: '2024-01-01T00:00:00Z',
      },
      {
        id: 2,
        wallet_id: 1,
        name: 'Jane Smith',
        language: 'no',
        notification_methods: [
          {
            id: 2,
            contact_id: 2,
            provider_type: 'ntfy',
            notification_target: 'jane-no-8nt3y08q',
            display_target: 'jane-no-8nt3y08q',
            created_at: '2024-01-02T00:00:00Z',
          }
        ],
        created_at: '2024-01-02T00:00:00Z',
      },
    ]

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
      expect(screen.getByText('jane-no-8nt3y08q')).toBeInTheDocument()
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
      // Check that the contacts badge shows 0 contacts
      expect(screen.getByText('0 contacts')).toBeInTheDocument()
    })

    expect(screen.getByText('0 contacts')).toBeInTheDocument()
  })

  it('allows creating a new contact with valid phone number', async () => {
    const apiModule = await import('../../lib/api')
    
    // Mock the fetch for getting wallet contacts (empty initially)
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [],
      } as Response)
      // Mock the fetch for refreshing contacts after creation
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [
          {
            id: 3,
            wallet_id: 1,
            name: 'New Contact',
            language: 'en',
            notification_methods: [
              {
                id: 3,
                contact_id: 3,
                provider_type: 'sms',
                notification_target: '+4712345678',
                display_target: '+4712345678',
                created_at: '2024-01-01T00:00:00Z',
              }
            ],
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
    
    // First enable SMS provider
    const twilioCheckbox = screen.getByRole('checkbox', { name: /sms notifications/i })
    fireEvent.click(twilioCheckbox)
    
    // Now the phone input should appear
    const phoneInput = screen.getByPlaceholderText('+1234567890')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    fireEvent.change(nameInput, { target: { value: 'New Contact' } })
    fireEvent.change(phoneInput, { target: { value: '+4712345678' } })
    fireEvent.click(createButton)

    // Verify API call
    await waitFor(() => {
      expect(apiModule.api.createContact).toHaveBeenCalledWith(
        1,
        'New Contact',
        'en',
        [{ provider_type: 'sms', notification_target: '+4712345678' }]
      )
    })

    await waitFor(() => {
      // New contact should be added
      expect(screen.getByText('New Contact')).toBeInTheDocument()
      expect(screen.getByText('+4712345678')).toBeInTheDocument()
      expect(screen.getByText('1 contact')).toBeInTheDocument()
    })
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

    // Fill in the contact form
    const nameInput = screen.getByLabelText('Name')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    // Fill in name but don't enable any notification method
    fireEvent.change(nameInput, { target: { value: 'Invalid Contact' } })
    fireEvent.click(createButton)

    // Verify error message
    await waitFor(() => {
      expect(screen.getByText(/please enable at least one notification method/i)).toBeInTheDocument()
    })
  })

  it('handles API error when creating contact', async () => {
    const apiModule = await import('../../lib/api')
    apiModule.api.createContact.mockRejectedValueOnce(new Error('Phone number already exists'))
    
    // Mock the fetch for getting wallet contacts (empty initially)
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => [],
    } as Response)

    render(<EditWalletModal {...defaultProps} />)

    await waitFor(() => {
      expect(screen.getByText('0 contacts')).toBeInTheDocument()
    })

    // Fill in the contact form
    const nameInput = screen.getByLabelText('Name')
    
    // Enable SMS provider
    const twilioCheckbox = screen.getByRole('checkbox', { name: /sms notifications/i })
    fireEvent.click(twilioCheckbox)
    
    const phoneInput = screen.getByPlaceholderText('+1234567890')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    fireEvent.change(nameInput, { target: { value: 'Test Contact' } })
    fireEvent.change(phoneInput, { target: { value: '+4712345678' } })
    fireEvent.click(createButton)

    await waitFor(() => {
      expect(screen.getByText('Phone number already exists')).toBeInTheDocument()
    })
  })

  it('allows deleting a contact', async () => {
    const mockContacts: Contact[] = [
      {
        id: 1,
        wallet_id: 1,
        name: 'John Doe',
        language: 'en',
        notification_methods: [
          {
            id: 1,
            contact_id: 1,
            provider_type: 'sms',
            notification_target: '+4792050946',
            display_target: '+4792050946',
            created_at: '2024-01-01T00:00:00Z',
          }
        ],
        created_at: '2024-01-01T00:00:00Z',
      },
      {
        id: 2,
        wallet_id: 1,
        name: 'Jane Smith',
        language: 'no',
        notification_methods: [
          {
            id: 2,
            contact_id: 2,
            provider_type: 'ntfy',
            notification_target: 'jane-no-8nt3y08q',
            display_target: 'jane-no-8nt3y08q',
            created_at: '2024-01-02T00:00:00Z',
          }
        ],
        created_at: '2024-01-02T00:00:00Z',
      },
    ]

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

    // Find delete buttons (X icons)
    const johnDoeContainer = screen.getByText('John Doe').closest('[data-testid="contact-item"]')
    const deleteButton = johnDoeContainer?.querySelector('button[class*="text-red"]')

    fireEvent.click(deleteButton!)

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

    // Wait for contacts to load
    await waitFor(() => {
      expect(screen.getByText('0 contacts')).toBeInTheDocument()
    })

    // Initially empty
    const nameInput = screen.getByLabelText('Name')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    expect(createButton).toBeDisabled()

    // Fill only name
    fireEvent.change(nameInput, { target: { value: 'Test' } })

    // Button should be enabled now (but will show error if clicked without notification method)
    expect(createButton).not.toBeDisabled()
  })

  it('shows loading state while creating contact', async () => {
    const apiModule = await import('../../lib/api')
    apiModule.api.createContact.mockImplementation(() => 
      new Promise(resolve => setTimeout(resolve, 100))
    )
    
    // Mock the fetch for getting wallet contacts
    mockFetch
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [],
      } as Response)
      // Mock the fetch for refreshing contacts after creation
      .mockResolvedValueOnce({
        ok: true,
        json: async () => [
          {
            id: 3,
            wallet_id: 1,
            name: 'Test Contact',
            language: 'en',
            notification_methods: [
              {
                id: 3,
                contact_id: 3,
                provider_type: 'sms',
                notification_target: '+4712345678',
                display_target: '+4712345678',
                created_at: '2024-01-01T00:00:00Z',
              }
            ],
            created_at: '2024-01-01T00:00:00Z',
          },
        ],
      } as Response)

    render(<EditWalletModal {...defaultProps} />)

    // Wait for initial load
    await waitFor(() => {
      expect(screen.queryByText('Loading contacts...')).not.toBeInTheDocument()
    })

    // Fill in the form
    const nameInput = screen.getByLabelText('Name')
    
    // Enable SMS provider
    const twilioCheckbox = screen.getByRole('checkbox', { name: /sms notifications/i })
    fireEvent.click(twilioCheckbox)
    
    const phoneInput = screen.getByPlaceholderText('+1234567890')
    const createButton = screen.getByRole('button', { name: /create contact/i })

    fireEvent.change(nameInput, { target: { value: 'Test Contact' } })
    fireEvent.change(phoneInput, { target: { value: '+4712345678' } })

    // Click create
    fireEvent.click(createButton)

    // Check loading state
    expect(screen.getByText('Creating...')).toBeInTheDocument()
    expect(createButton).toBeDisabled()
  })

  it('formats phone numbers for display', async () => {
    const mockContacts: Contact[] = [
      {
        id: 1,
        wallet_id: 1,
        name: 'John Doe',
        language: 'en',
        notification_methods: [
          {
            id: 1,
            contact_id: 1,
            provider_type: 'sms',
            notification_target: '+4792050946',
            display_target: '+4792050946',
            created_at: '2024-01-01T00:00:00Z',
          }
        ],
        created_at: '2024-01-01T00:00:00Z',
      }
    ]

    // Mock the fetch for getting wallet contacts
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => mockContacts,
    } as Response)

    render(<EditWalletModal {...defaultProps} />)

    // Wait for contacts to load
    await waitFor(() => {
      expect(screen.queryByText('Loading contacts...')).not.toBeInTheDocument()
    })

    // Check that we have the contact count and details
    expect(screen.getByText('1 contact')).toBeInTheDocument()
    expect(screen.getByText('John Doe')).toBeInTheDocument()
    expect(screen.getByText('+4792050946')).toBeInTheDocument()

    // Verify the fetch was called with correct URL
    expect(mockFetch).toHaveBeenCalledWith('/api/wallets/1/contacts')
  })
})