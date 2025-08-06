import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { AddContactInline } from '../add-contact-inline'

// Mock the api module
jest.mock('../../lib/api', () => ({
  api: {
    getProviders: jest.fn(),
    createContact: jest.fn(),
  },
}))

const mockApi = jest.requireMock('../../lib/api').api

describe('AddContactInline', () => {
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

  const defaultProps = {
    walletChecksum: 'test-checksum',
    onContactAdded: jest.fn(),
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getProviders.mockResolvedValue({ providers: mockProviders })
    mockApi.createContact.mockResolvedValue({})
  })

  it('shows add contact button initially', () => {
    render(<AddContactInline {...defaultProps} />)
    
    expect(screen.getByText('Add Contact')).toBeInTheDocument()
  })

  it('expands form when add contact button is clicked', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByText('Add New Contact')).toBeInTheDocument()
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
      expect(screen.getByLabelText('Language')).toBeInTheDocument()
    })
  })

  it('loads providers when form is expanded', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(mockApi.getProviders).toHaveBeenCalled()
      expect(screen.getByText('ntfy.sh')).toBeInTheDocument()
      expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
    })
  })

  it('shows phone input when SMS provider is enabled', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByText('SMS Notifications')).toBeInTheDocument()
    })

    // Enable SMS provider
    const smsCheckbox = screen.getByRole('checkbox', { name: /sms notifications/i })
    fireEvent.click(smsCheckbox)
    
    expect(screen.getByPlaceholderText('+1234567890')).toBeInTheDocument()
  })

  it('creates contact with SMS notification method', async () => {
    mockApi.createContact.mockResolvedValue({
      id: 1,
      name: 'Test Contact',
      language: 'en',
      notification_methods: [{
        id: 1,
        contact_id: 1,
        provider_type: 'sms',
        notification_target: '+4712345678',
        display_target: '+4712345678',
        created_at: '2024-01-01T00:00:00Z',
      }],
    })

    render(<AddContactInline {...defaultProps} />)
    
    // Expand form
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
    })

    // Fill form
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Test Contact' } })
    
    // Enable SMS provider
    const smsCheckbox = screen.getByRole('checkbox', { name: /sms notifications/i })
    fireEvent.click(smsCheckbox)
    
    // Fill phone number
    const phoneInput = screen.getByPlaceholderText('+1234567890')
    fireEvent.change(phoneInput, { target: { value: '+4712345678' } })
    
    // Create contact
    fireEvent.click(screen.getByText('Create Contact'))
    
    await waitFor(() => {
      expect(mockApi.createContact).toHaveBeenCalledWith(
        'test-checksum',
        'Test Contact',
        'en',
        [{ provider_type: 'sms', notification_target: '+4712345678' }]
      )
    })
    
    await waitFor(() => {
      expect(defaultProps.onContactAdded).toHaveBeenCalled()
    })
  })

  it('creates contact with ntfy notification method', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    // Expand form
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
    })

    // Fill form
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Test Contact' } })
    
    // Enable ntfy provider
    const ntfyCheckbox = screen.getByRole('checkbox', { name: /ntfy\.sh/i })
    fireEvent.click(ntfyCheckbox)
    
    // Create contact
    fireEvent.click(screen.getByText('Create Contact'))
    
    await waitFor(() => {
      expect(mockApi.createContact).toHaveBeenCalledWith(
        'test-checksum',
        'Test Contact',
        'en',
        [{ provider_type: 'ntfy', notification_target: '' }]
      )
    })
  })

  it('disables create button when no name is provided', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByText('Create Contact')).toBeInTheDocument()
    })

    // Create button should be disabled when name is empty
    const createButton = screen.getByText('Create Contact')
    expect(createButton).toBeDisabled()
  })

  it('shows error when no notification method is enabled', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
    })

    // Fill name only
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Test Contact' } })
    
    // Try to create without enabling any provider
    fireEvent.click(screen.getByText('Create Contact'))
    
    await waitFor(() => {
      expect(screen.getByText('Please enable at least one notification method')).toBeInTheDocument()
    })
  })

  it('cancels form and collapses', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByText('Add New Contact')).toBeInTheDocument()
    })

    // Cancel
    fireEvent.click(screen.getByText('Cancel'))
    
    expect(screen.getByText('Add Contact')).toBeInTheDocument()
    expect(screen.queryByText('Add New Contact')).not.toBeInTheDocument()
  })

  it('supports Norwegian language selection', async () => {
    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByLabelText('Language')).toBeInTheDocument()
    })

    // Change language to Norwegian
    const languageSelect = screen.getByLabelText('Language')
    fireEvent.change(languageSelect, { target: { value: 'no' } })
    
    // Fill form and create contact
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Test Contact' } })
    
    const ntfyCheckbox = screen.getByRole('checkbox', { name: /ntfy\.sh/i })
    fireEvent.click(ntfyCheckbox)
    
    fireEvent.click(screen.getByText('Create Contact'))
    
    await waitFor(() => {
      expect(mockApi.createContact).toHaveBeenCalledWith(
        'test-checksum',
        'Test Contact',
        'no',
        [{ provider_type: 'ntfy', notification_target: '' }]
      )
    })
  })

  it('shows loading state during contact creation', async () => {
    mockApi.createContact.mockImplementation(() => 
      new Promise(resolve => setTimeout(resolve, 100))
    )

    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
    })

    // Fill form
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Test Contact' } })
    
    const ntfyCheckbox = screen.getByRole('checkbox', { name: /ntfy\.sh/i })
    fireEvent.click(ntfyCheckbox)
    
    // Create contact
    fireEvent.click(screen.getByText('Create Contact'))
    
    expect(screen.getByText('Creating...')).toBeInTheDocument()
    expect(screen.getByText('Creating...')).toBeDisabled()
  })

  it('shows API error message when creation fails', async () => {
    mockApi.createContact.mockRejectedValue(new Error('Network error'))

    render(<AddContactInline {...defaultProps} />)
    
    fireEvent.click(screen.getByText('Add Contact'))
    
    await waitFor(() => {
      expect(screen.getByLabelText('Name')).toBeInTheDocument()
    })

    // Fill form
    fireEvent.change(screen.getByLabelText('Name'), { target: { value: 'Test Contact' } })
    
    const ntfyCheckbox = screen.getByRole('checkbox', { name: /ntfy\.sh/i })
    fireEvent.click(ntfyCheckbox)
    
    // Create contact
    fireEvent.click(screen.getByText('Create Contact'))
    
    await waitFor(() => {
      expect(screen.getByText('Network error')).toBeInTheDocument()
    })
  })
})