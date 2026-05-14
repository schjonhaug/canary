import React from 'react'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
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
    getUserPreferences: jest.fn(),
  },
}))

// Mock the useNtfyServerUrl hook
const mockUseNtfyServerTarget = jest.fn(() => ({ url: 'https://ntfy.sh', isBrowserSafe: true }))
jest.mock('../../hooks/useNtfyServerUrl', () => ({
  useNtfyServerTarget: () => mockUseNtfyServerTarget(),
  useNtfyServerUrl: () => mockUseNtfyServerTarget().url,
}))

// Mock the useAuth hook
const mockUseAuth = jest.fn()
jest.mock('../../contexts/auth-context', () => ({
  useAuth: () => mockUseAuth()
}))

const defaultAuthState = {
  user: {
    id: 1,
    email: 'test@example.com',
    name: 'Test User',
    is_admin: false,
    is_demo: false,
    email_verified: true
  },
  isAuthenticated: true,
  isLoading: false,
  isCloudMode: true,
  billingStatus: {
    subscription_tier: 'team',
    subscription_status: 'active',
    wallet_count: 1,
    contact_count: 2,
    limits: {
      max_wallets: 5,
      max_contacts_per_wallet: 5,
      sync_interval_seconds: 120
    }
  }
}

const mockApi = jest.requireMock('../../lib/api').api

describe('WalletContactsList', () => {
  const mockContacts: Contact[] = [
    {
      id: 'contact-1',
      wallet_checksum: 'test-checksum',
      name: 'Alice Smith',
      notification_methods: [
        {
          id: 'method-1',
          contact_id: 'contact-1',
          provider_type: 'sms',
          notification_target: '+4792050946',
          display_target: '+4792050946',
          created_at: '2024-01-01T00:00:00Z',
        }
      ],
      created_at: '2024-01-01T00:00:00Z',
      is_active: true,
    },
    {
      id: 'contact-2',
      wallet_checksum: 'test-checksum',
      name: 'Bob Johnson',
      notification_methods: [
        {
          id: 'method-2',
          contact_id: 'contact-2',
          provider_type: 'ntfy',
          notification_target: 'bob-no-8nt3y08q',
          display_target: 'bob-no-8nt3y08q',
          created_at: '2024-01-02T00:00:00Z',
        }
      ],
      created_at: '2024-01-02T00:00:00Z',
      is_active: true,
    },
  ]

  const defaultProps = {
    walletChecksum: 'test-checksum',
    contacts: mockContacts,
    onContactsUpdated: jest.fn(),
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockUseNtfyServerTarget.mockReturnValue({ url: 'https://ntfy.sh', isBrowserSafe: true })
    mockUseAuth.mockReturnValue(defaultAuthState)
    mockApi.getProviders.mockResolvedValue({ providers: [] })
    mockApi.sendContactVerification.mockResolvedValue({ message: 'Verification sent' })
    mockApi.verifyContact.mockResolvedValue({ valid: true, message: 'Verified' })
    mockApi.createContact.mockResolvedValue({ id: 1 })
    mockApi.deleteContact.mockResolvedValue({})
    mockApi.getUserPreferences.mockResolvedValue({
      preferred_fiat_currency: 'USD',
      preferred_tx_explorer_id: null,
      ntfy_server_url: null,
      ntfy_has_access_token: false,
      ntfy_has_credentials: false,
      ntfy_username: null,
    })
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

    const contactsList = screen.getByRole('list', { name: 'Contacts' })
    expect(within(contactsList).getAllByRole('listitem')).toHaveLength(2)

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

  it('sorts contacts with the same name by creation date', () => {
    const unsortedContacts = [
      {
        ...mockContacts[0],
        id: 'contact-newer',
        created_at: '2024-01-03T00:00:00Z',
        notification_methods: [
          {
            ...mockContacts[0].notification_methods[0],
            id: 'method-newer',
            contact_id: 'contact-newer',
            display_target: 'newer-target',
          },
        ],
      },
      {
        ...mockContacts[0],
        id: 'contact-older',
        created_at: '2024-01-01T00:00:00Z',
        notification_methods: [
          {
            ...mockContacts[0].notification_methods[0],
            id: 'method-older',
            contact_id: 'contact-older',
            display_target: 'older-target',
          },
        ],
      },
    ]

    render(<WalletContactsList {...defaultProps} contacts={unsortedContacts} />)

    const contactsList = screen.getByRole('list', { name: 'Contacts' })
    const contactItems = within(contactsList).getAllByRole('listitem')
    expect(contactItems[0]).toHaveTextContent('older-target')
    expect(contactItems[1]).toHaveTextContent('newer-target')
  })

  it('shows correct icons for different notification methods', () => {
    render(<WalletContactsList {...defaultProps} />)

    // SMS should show smartphone icon, ntfy should show bell icon
    const icons = document.querySelectorAll('svg')
    expect(icons.length).toBeGreaterThan(0)
  })

  it('shows edit button for contacts', async () => {
    render(<WalletContactsList {...defaultProps} />)

    const contactsList = screen.getByRole('list', { name: 'Contacts' })

    // Should show edit buttons (no delete buttons anymore)
    expect(within(contactsList).getByRole('button', { name: 'Edit Alice Smith' })).toBeInTheDocument()
    expect(within(contactsList).getByRole('button', { name: 'Edit Bob Johnson' })).toBeInTheDocument()
    const editIcons = document.querySelectorAll('svg')
    expect(editIcons.length).toBeGreaterThan(0)
  })

  it('opens edit modal when edit button is clicked', async () => {
    render(<WalletContactsList {...defaultProps} />)

    // Find and click edit button
    fireEvent.click(screen.getByRole('button', { name: 'Edit Alice Smith' }))

    // Contact modal should open in edit mode
    await waitFor(() => {
      expect(screen.getByText('Edit Contact')).toBeInTheDocument()
    })
  })

  it.each([
    ['admin', { is_admin: true }],
    ['demo', { is_demo: true }],
  ])('disables edit buttons for %s users in cloud mode', (_userType, userOverrides) => {
    mockUseAuth.mockReturnValue({
      ...defaultAuthState,
      user: {
        ...defaultAuthState.user,
        ...userOverrides,
      },
    })

    render(<WalletContactsList {...defaultProps} />)

    const editButton = screen.getByRole('button', { name: 'Edit Alice Smith' })
    expect(editButton).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Edit Bob Johnson' })).toBeDisabled()

    fireEvent.click(editButton)
    expect(screen.queryByText('Edit Contact')).not.toBeInTheDocument()
  })

  it.each([
    ['admin', { is_admin: true }],
    ['demo', { is_demo: true }],
  ])('keeps edit buttons enabled for %s users outside cloud mode', (_userType, userOverrides) => {
    mockUseAuth.mockReturnValue({
      ...defaultAuthState,
      isCloudMode: false,
      user: {
        ...defaultAuthState.user,
        ...userOverrides,
      },
    })

    render(<WalletContactsList {...defaultProps} />)

    expect(screen.getByRole('button', { name: 'Edit Alice Smith' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Edit Bob Johnson' })).toBeEnabled()
  })

  it('does not open the edit modal when clicking contact body text', () => {
    render(<WalletContactsList {...defaultProps} />)

    fireEvent.click(screen.getByText('Alice Smith'))

    expect(screen.queryByText('Edit Contact')).not.toBeInTheDocument()
  })

  it('does not open the edit modal when clicking notification links', () => {
    render(<WalletContactsList {...defaultProps} />)

    fireEvent.click(screen.getByText('+4792050946'))

    expect(screen.queryByText('Edit Contact')).not.toBeInTheDocument()
  })

  it('renders inactive contacts with the tier limit message', () => {
    render(
      <WalletContactsList
        {...defaultProps}
        contacts={[{ ...mockContacts[0], is_active: false }]}
      />
    )

    expect(screen.getByText('Inactive')).toBeInTheDocument()
    expect(screen.getByText("This contact exceeds your subscription tier limits and won't receive notifications")).toBeInTheDocument()
    expect(screen.getByText('Alice Smith')).toHaveClass('line-through')
  })

  it('renders inactive contacts with the expired subscription message', () => {
    mockUseAuth.mockReturnValue({
      ...defaultAuthState,
      billingStatus: {
        ...defaultAuthState.billingStatus,
        subscription_status: 'expired',
      },
    })

    render(
      <WalletContactsList
        {...defaultProps}
        contacts={[{ ...mockContacts[0], is_active: false }]}
      />
    )

    expect(screen.getByText("Your subscription has expired - contact won't receive notifications")).toBeInTheDocument()
  })

  it('renders empty state when no contacts', () => {
    render(<WalletContactsList {...defaultProps} contacts={[]} />)

    // Should show empty state message
    expect(screen.getByText('No contacts added yet')).toBeInTheDocument()
    // Should not show any contact items
    expect(screen.queryByText('Alice Smith')).not.toBeInTheDocument()
    expect(screen.queryByText('Bob Johnson')).not.toBeInTheDocument()
  })

  it('renders ntfy links with default ntfy.sh URL when no custom server configured', () => {
    render(<WalletContactsList {...defaultProps} />)

    const ntfyLink = screen.getByText('bob-no-8nt3y08q').closest('a')
    expect(ntfyLink).toHaveAttribute('href', 'https://ntfy.sh/bob-no-8nt3y08q')
  })

  it('renders ntfy links with custom server URL when configured', () => {
    mockUseNtfyServerTarget.mockReturnValue({ url: 'https://ntfy.example.com', isBrowserSafe: true })

    render(<WalletContactsList {...defaultProps} />)

    const ntfyLink = screen.getByText('bob-no-8nt3y08q').closest('a')
    expect(ntfyLink).toHaveAttribute('href', 'https://ntfy.example.com/bob-no-8nt3y08q')
  })

  it('renders ntfy links for browser-safe local servers', () => {
    mockUseNtfyServerTarget.mockReturnValue({ url: 'http://umbrel', isBrowserSafe: true })

    render(<WalletContactsList {...defaultProps} />)

    const ntfyLink = screen.getByText('bob-no-8nt3y08q').closest('a')
    expect(ntfyLink).toHaveAttribute('href', 'http://umbrel/bob-no-8nt3y08q')
  })

  it('renders ntfy targets as plain text when server is not browser-safe', () => {
    mockUseNtfyServerTarget.mockReturnValue({ url: 'http://ntfy_app_1', isBrowserSafe: false })

    render(<WalletContactsList {...defaultProps} />)

    expect(screen.getByText('bob-no-8nt3y08q').closest('a')).toBeNull()
  })

  it('displays multiple notification methods for a single contact', () => {
    const contactWithMultipleMethods: Contact = {
      id: 'contact-3',
      wallet_checksum: 'test-checksum',
      name: 'Charlie Brown',
      notification_methods: [
        {
          id: 'method-3',
          contact_id: 'contact-3',
          provider_type: 'sms',
          notification_target: '+4712345678',
          display_target: '+4712345678',
          created_at: '2024-01-03T00:00:00Z',
        },
        {
          id: 'method-4',
          contact_id: 'contact-3',
          provider_type: 'ntfy',
          notification_target: 'charlie-en-8nt3y08q',
          display_target: 'charlie-en-8nt3y08q',
          created_at: '2024-01-03T00:00:00Z',
        }
      ],
      created_at: '2024-01-03T00:00:00Z',
      is_active: true,
    }

    render(<WalletContactsList {...defaultProps} contacts={[contactWithMultipleMethods]} />)

    expect(screen.getByText('Charlie Brown')).toBeInTheDocument()
    expect(screen.getByText('+4712345678')).toBeInTheDocument()
    expect(screen.getByText('charlie-en-8nt3y08q')).toBeInTheDocument()
  })
})
