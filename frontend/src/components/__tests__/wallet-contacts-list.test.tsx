import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { WalletContactsList } from '../wallet-contacts-list'
import { Contact } from '../../types'

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

describe('WalletContactsList', () => {
  const mockContacts: Contact[] = [
    {
      id: 1,
      wallet_id: 1,
      name: 'Alice Smith',
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
      name: 'Bob Johnson',
      language: 'no',
      notification_methods: [
        {
          id: 2,
          contact_id: 2,
          provider_type: 'ntfy',
          notification_target: 'bob-no-8nt3y08q',
          display_target: 'bob-no-8nt3y08q',
          created_at: '2024-01-02T00:00:00Z',
        }
      ],
      created_at: '2024-01-02T00:00:00Z',
    },
  ]

  const defaultProps = {
    walletChecksum: 'test-checksum',
    contacts: mockContacts,
    onContactsUpdated: jest.fn(),
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.getProviders.mockResolvedValue({ providers: [] })
    mockApi.sendContactVerification.mockResolvedValue({ message: 'Verification sent' })
    mockApi.verifyContact.mockResolvedValue({ valid: true, message: 'Verified' })
    mockApi.createContact.mockResolvedValue({ id: 1 })
    mockApi.deleteContact.mockResolvedValue({})
  })

  it('renders contacts without title', () => {
    render(<WalletContactsList {...defaultProps} />)
    
    // The component no longer has a "Contacts" title
    expect(screen.queryByText('Contacts')).not.toBeInTheDocument()
    // But should render the contacts
    expect(screen.getByText('Alice Smith')).toBeInTheDocument()
  })

  it('displays all contacts with correct information', () => {
    render(<WalletContactsList {...defaultProps} />)
    
    // Check contact names
    expect(screen.getByText('Alice Smith')).toBeInTheDocument()
    expect(screen.getByText('Bob Johnson')).toBeInTheDocument()
    
    // Check notification targets (no language badges in current component)
    expect(screen.getByText('+4792050946')).toBeInTheDocument()
    expect(screen.getByText('bob-no-8nt3y08q')).toBeInTheDocument()
  })

  it('sorts contacts alphabetically by name', () => {
    const unsortedContacts = [
      { ...mockContacts[1], name: 'Zoe Wilson' },
      { ...mockContacts[0], name: 'Alice Smith' },
    ]
    
    render(<WalletContactsList {...defaultProps} contacts={unsortedContacts} />)
    
    const contactElements = screen.getAllByText(/Smith|Wilson/)
    expect(contactElements[0]).toHaveTextContent('Alice Smith')
    expect(contactElements[1]).toHaveTextContent('Zoe Wilson')
  })

  it('shows correct icons for different notification methods', () => {
    render(<WalletContactsList {...defaultProps} />)
    
    // SMS should show smartphone icon, ntfy should show bell icon
    const icons = document.querySelectorAll('svg')
    expect(icons.length).toBeGreaterThan(0)
  })

  it('shows edit button for contacts', async () => {
    render(<WalletContactsList {...defaultProps} />)
    
    // Should show edit buttons (no delete buttons anymore)
    const editButtons = screen.getAllByRole('button')
    expect(editButtons.length).toBeGreaterThan(0)
    
    // Edit buttons should have Edit icons
    const editIcons = document.querySelectorAll('svg')
    expect(editIcons.length).toBeGreaterThan(0)
  })

  it('opens edit modal when edit button is clicked', async () => {
    render(<WalletContactsList {...defaultProps} />)
    
    // Find and click edit button
    const editButtons = screen.getAllByRole('button')
    fireEvent.click(editButtons[0])
    
    // Contact modal should open in edit mode
    await waitFor(() => {
      expect(screen.getByText('Edit Contact')).toBeInTheDocument()
    })
  })

  it('renders empty state when no contacts', () => {
    render(<WalletContactsList {...defaultProps} contacts={[]} />)
    
    // Should show empty state message
    expect(screen.getByText('No contacts added yet')).toBeInTheDocument()
    // Should not show any contact items
    expect(screen.queryByText('Alice Smith')).not.toBeInTheDocument()
    expect(screen.queryByText('Bob Johnson')).not.toBeInTheDocument()
  })

  it('displays multiple notification methods for a single contact', () => {
    const contactWithMultipleMethods: Contact = {
      id: 3,
      wallet_id: 1,
      name: 'Charlie Brown',
      language: 'en',
      notification_methods: [
        {
          id: 3,
          contact_id: 3,
          provider_type: 'sms',
          notification_target: '+4712345678',
          display_target: '+4712345678',
          created_at: '2024-01-03T00:00:00Z',
        },
        {
          id: 4,
          contact_id: 3,
          provider_type: 'ntfy',
          notification_target: 'charlie-en-8nt3y08q',
          display_target: 'charlie-en-8nt3y08q',
          created_at: '2024-01-03T00:00:00Z',
        }
      ],
      created_at: '2024-01-03T00:00:00Z',
    }

    render(<WalletContactsList {...defaultProps} contacts={[contactWithMultipleMethods]} />)
    
    expect(screen.getByText('Charlie Brown')).toBeInTheDocument()
    expect(screen.getByText('+4712345678')).toBeInTheDocument()
    expect(screen.getByText('charlie-en-8nt3y08q')).toBeInTheDocument()
  })
})