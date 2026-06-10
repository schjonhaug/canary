import React from 'react'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import WalletNotificationsPage from '../page'
import type { BalanceAlert, Contact, Wallet, WalletNotificationsResponse } from '@/types'
import { api } from '@/lib/api'

const mockPush = jest.fn()
const mockSetCurrentWallet = jest.fn()
const mockUseAuth = jest.fn()

Object.defineProperties(Element.prototype, {
  hasPointerCapture: {
    value: jest.fn(() => false),
  },
  setPointerCapture: {
    value: jest.fn(),
  },
  releasePointerCapture: {
    value: jest.fn(),
  },
  scrollIntoView: {
    value: jest.fn(),
  },
})

jest.mock('next/navigation', () => ({
  useParams: () => ({ checksum: 'sq32h3ch' }),
  useRouter: () => ({
    push: mockPush,
  }),
}))

jest.mock('@/contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('@/contexts/wallets-context', () => ({
  useWalletsContext: () => ({
    setCurrentWallet: mockSetCurrentWallet,
  }),
}))

jest.mock('@/components/wallet-detail', () => ({
  WalletDetailHeader: ({ walletName }: { walletName: string }) => (
    <div data-testid="wallet-detail-header">{walletName}</div>
  ),
  WalletDetailSkeleton: () => <div data-testid="wallet-detail-skeleton" />,
  getWalletDetailErrorState: jest.fn(() => null),
}))

jest.mock('@/components/plans-modal', () => ({
  PlansModal: ({ isOpen, limitType, currentContactCount }: {
    isOpen: boolean
    limitType: string
    currentContactCount: number
  }) =>
    isOpen ? (
      <div data-testid="plans-modal">
        {limitType}:{currentContactCount}
      </div>
    ) : null,
}))

jest.mock('@/hooks/useNtfyServerUrl', () => ({
  useNtfyServerTarget: () => ({
    url: 'http://localhost:8080',
    defaultTopic: 'canary-dev-topic',
    isBrowserSafe: true,
  }),
}))

const verificationMock = {
  verificationSent: false,
  verificationCode: '',
  verificationPhone: null,
  verificationAddress: null,
  isVerified: true,
  showSuccess: false,
  isSending: false,
  isVerifying: false,
  verificationError: null,
  phoneError: null,
  emailError: null,
  timeRemaining: 0,
  formatTime: jest.fn(() => '0:00'),
  setVerificationCode: jest.fn(),
  clearPhoneError: jest.fn(),
  clearEmailError: jest.fn(),
  clearVerificationError: jest.fn(),
  sendVerification: jest.fn(),
  verifyCode: jest.fn(),
  resendCode: jest.fn(),
  reset: jest.fn(),
  resetForPhoneChange: jest.fn(),
  resetForEmailChange: jest.fn(),
  revertToOriginal: jest.fn(),
  setVerified: jest.fn(),
}

jest.mock('@/hooks/useSmsVerification', () => ({
  useSmsVerification: () => verificationMock,
}))

jest.mock('@/hooks/useEmailVerification', () => ({
  useEmailVerification: () => verificationMock,
}))

jest.mock('@/hooks/usePhonePlaceholder', () => ({
  usePhonePlaceholder: () => '+47 123 45 678',
}))

jest.mock('@/lib/api', () => ({
  ApiError: class ApiError extends Error {},
  api: {
    getWalletNotifications: jest.fn(),
    createContact: jest.fn(),
    updateContact: jest.fn(),
    deleteContact: jest.fn(),
    createBalanceAlert: jest.fn(),
    deleteBalanceAlert: jest.fn(),
    getUserPreferences: jest.fn(),
  },
}))

const mockApi = api as jest.Mocked<typeof api>

const wallet: Wallet = {
  checksum: 'sq32h3ch',
  name: 'Regtest Wallet',
  descriptor: 'wpkh([abcd/84h/1h/0h]tpub/0/*)',
  wallet_filename: 'regtest-wallet',
  hex_color: '#f59e0b',
  created_at: '2024-01-01T00:00:00Z',
  balance_total: 50000000,
  last_activity: null,
  status: 'ready',
  contact_count: 1,
  is_active: true,
  wallet_type: 'descriptor',
}

function makeContact(overrides: Partial<Contact> = {}): Contact {
  const id = overrides.id ?? 'contact-1'
  return {
    id,
    wallet_checksum: 'sq32h3ch',
    name: 'Alice',
    notification_methods: [
      {
        id: `${id}-method-1`,
        contact_id: id,
        provider_type: 'ntfy',
        notification_target: 'alice-topic',
        display_target: 'alice-topic',
        created_at: '2024-01-01T00:00:00Z',
        is_enabled: true,
      },
    ],
    created_at: '2024-01-01T00:00:00Z',
    is_active: true,
    notify_sending: true,
    notify_sent: true,
    notify_receiving: true,
    notify_received: true,
    notify_cpfp: false,
    notify_rbf: false,
    include_wallet_balance_in_tx_notifications: false,
    ...overrides,
  }
}

function makeAlert(overrides: Partial<BalanceAlert> = {}): BalanceAlert {
  return {
    id: 'alert-1',
    wallet_checksum: 'sq32h3ch',
    contact_id: 'contact-1',
    threshold_sats: 100000000,
    alert_type: 'above',
    is_active: true,
    created_at: '2024-01-01T00:00:00Z',
    ...overrides,
  }
}

function mockNotificationsResponse(
  contacts: Contact[] = [makeContact()],
  balanceAlerts: BalanceAlert[] = []
) {
  const response: WalletNotificationsResponse = {
    timestamp: Date.now(),
    wallet,
    contacts,
    balance_alerts: balanceAlerts,
  }
  mockApi.getWalletNotifications.mockResolvedValue(response)
}

async function renderLoadedPage() {
  render(<WalletNotificationsPage />)
  await screen.findByRole('heading', { name: 'Notifications' })
}

describe('WalletNotificationsPage', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: false,
      isSelfHostedMode: true,
      user: { id: 1, email: 'test@example.com' },
      billingStatus: null,
    })
    mockApi.createContact.mockResolvedValue(makeContact())
    mockApi.updateContact.mockResolvedValue(makeContact())
    mockApi.deleteContact.mockResolvedValue(undefined)
    mockApi.createBalanceAlert.mockResolvedValue(makeAlert())
    mockApi.deleteBalanceAlert.mockResolvedValue(undefined)
    mockApi.getUserPreferences.mockResolvedValue({ preferred_fiat_currency: 'NOK' })
  })

  it('sorts contacts by name and renders the transaction notification groups', async () => {
    mockNotificationsResponse([
      makeContact({ id: 'contact-2', name: 'Zoe' }),
      makeContact({ id: 'contact-1', name: 'alice' }),
      makeContact({ id: 'contact-3', name: 'Bob' }),
    ])

    await renderLoadedPage()

    const names = screen.getAllByRole('heading', { level: 2 }).map((heading) => heading.textContent)
    expect(names).toEqual(['alice', 'Bob', 'Zoe'])
    expect(screen.getAllByText('Transaction notifications')).toHaveLength(3)
    expect(screen.getAllByText('Activity')).toHaveLength(3)
    expect(screen.getAllByText('First confirmation')).toHaveLength(3)
    expect(screen.getAllByText('Replacements / fee bumps')).toHaveLength(3)
    expect(screen.getAllByText('Sending')).toHaveLength(3)
    expect(screen.getAllByText('Receiving')).toHaveLength(3)
    expect(screen.getAllByText('Sent')).toHaveLength(3)
    expect(screen.getAllByText('Received')).toHaveLength(3)
    expect(screen.getAllByText('RBF replacement')).toHaveLength(3)
    expect(screen.getAllByText('CPFP fee bump')).toHaveLength(3)
  })

  it('creates an ntfy contact inline with default transaction notification settings', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Add contact' }))

    expect(screen.getByRole('heading', { name: 'New contact' })).toBeInTheDocument()
    expect(screen.queryByText('Transaction notifications')).not.toBeInTheDocument()

    await user.type(screen.getByLabelText('New contact name'), 'Nora')
    await user.click(screen.getByRole('button', { name: 'Next' }))
    await screen.findByLabelText('ntfy Topic')
    await user.click(screen.getByRole('button', { name: 'Create contact' }))

    await waitFor(() => expect(mockApi.createContact).toHaveBeenCalledTimes(1))
    expect(mockApi.createContact).toHaveBeenCalledWith(
      'sq32h3ch',
      'Nora',
      [
        {
          provider_type: 'ntfy',
          notification_target: 'canary-dev-topic',
          is_enabled: true,
        },
      ],
      {
        notify_sending: true,
        notify_sent: true,
        notify_receiving: true,
        notify_received: true,
        notify_cpfp: false,
        notify_rbf: false,
        include_wallet_balance_in_tx_notifications: false,
      }
    )
  })

  it('autosaves transaction notification checkbox changes', async () => {
    const user = userEvent.setup()
    const contact = makeContact({
      notify_rbf: true,
      notify_cpfp: false,
      include_wallet_balance_in_tx_notifications: true,
    })
    mockNotificationsResponse([contact])

    await renderLoadedPage()
    await user.click(screen.getByText('RBF replacement'))

    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))
    expect(mockApi.updateContact).toHaveBeenCalledWith(
      'sq32h3ch',
      'contact-1',
      'Alice',
      [
        {
          provider_type: 'ntfy',
          notification_target: 'alice-topic',
          is_enabled: true,
        },
      ],
      {
        notify_sending: true,
        notify_sent: true,
        notify_receiving: true,
        notify_received: true,
        notify_cpfp: false,
        notify_rbf: false,
        include_wallet_balance_in_tx_notifications: true,
      }
    )
    expect(await screen.findByText('Saved')).toBeInTheDocument()
  })

  it('preserves wallet balance preference when the last transaction category is disabled', async () => {
    const user = userEvent.setup()
    const contact = makeContact({
      notify_sending: true,
      notify_sent: false,
      notify_receiving: false,
      notify_received: false,
      notify_cpfp: false,
      notify_rbf: false,
      include_wallet_balance_in_tx_notifications: true,
    })
    mockNotificationsResponse([contact])

    await renderLoadedPage()
    await user.click(screen.getByText('Sending'))

    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))
    expect(mockApi.updateContact).toHaveBeenCalledWith(
      'sq32h3ch',
      'contact-1',
      'Alice',
      [
        {
          provider_type: 'ntfy',
          notification_target: 'alice-topic',
          is_enabled: true,
        },
      ],
      {
        notify_sending: false,
        notify_sent: false,
        notify_receiving: false,
        notify_received: false,
        notify_cpfp: false,
        notify_rbf: false,
        include_wallet_balance_in_tx_notifications: true,
      }
    )
  })

  it('queues contact saves behind in-flight transaction autosaves', async () => {
    const user = userEvent.setup()
    const updateResolvers: Array<(value: Contact) => void> = []
    mockApi.updateContact.mockImplementation(
      () =>
        new Promise<Contact>((resolve) => {
          updateResolvers.push(resolve)
        })
    )
    mockNotificationsResponse([
      makeContact({
        notify_rbf: true,
        include_wallet_balance_in_tx_notifications: true,
      }),
    ])

    await renderLoadedPage()
    await user.click(screen.getByText('RBF replacement'))
    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))

    await user.click(screen.getByLabelText('Contact actions'))
    await user.click(await screen.findByText('Edit contact'))
    const nameInput = screen.getByLabelText('Contact name')
    await user.clear(nameInput)
    await user.type(nameInput, 'Alicia')
    await user.click(screen.getByRole('button', { name: /Save contact/ }))

    expect(mockApi.updateContact).toHaveBeenCalledTimes(1)
    await act(async () => {
      updateResolvers.shift()?.(makeContact())
    })

    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(2))
    expect(mockApi.updateContact).toHaveBeenLastCalledWith(
      'sq32h3ch',
      'contact-1',
      'Alicia',
      [
        {
          provider_type: 'ntfy',
          notification_target: 'alice-topic',
          is_enabled: true,
        },
      ],
      {
        notify_sending: true,
        notify_sent: true,
        notify_receiving: true,
        notify_received: true,
        notify_cpfp: false,
        notify_rbf: false,
        include_wallet_balance_in_tx_notifications: true,
      }
    )
    await act(async () => {
      updateResolvers.shift()?.(makeContact({ name: 'Alicia', notify_rbf: false }))
    })
  })

  it('waits for queued transaction autosaves before deleting a contact', async () => {
    const user = userEvent.setup()
    const updateResolvers: Array<(value: Contact) => void> = []
    mockApi.updateContact.mockImplementation(
      () =>
        new Promise<Contact>((resolve) => {
          updateResolvers.push(resolve)
        })
    )
    mockNotificationsResponse([
      makeContact({
        notify_rbf: true,
      }),
    ])

    await renderLoadedPage()
    await user.click(screen.getByText('RBF replacement'))
    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))

    await user.click(screen.getByLabelText('Contact actions'))
    await user.click(await screen.findByText('Delete contact'))

    expect(mockApi.deleteContact).not.toHaveBeenCalled()

    await act(async () => {
      updateResolvers.shift()?.(makeContact({ notify_rbf: false }))
    })

    await waitFor(() =>
      expect(mockApi.deleteContact).toHaveBeenCalledWith('sq32h3ch', 'contact-1')
    )
    expect(mockApi.updateContact).toHaveBeenCalledTimes(1)
  })

  it('adds and deletes balance threshold notifications from the contact card', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([makeContact()], [makeAlert()])

    await renderLoadedPage()

    expect(screen.getByText('above 1 BTC')).toBeInTheDocument()

    await user.click(screen.getByText('Below'))
    await user.type(screen.getByPlaceholderText('0.10'), '0.25')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    await waitFor(() => expect(mockApi.createBalanceAlert).toHaveBeenCalledTimes(1))
    expect(mockApi.createBalanceAlert).toHaveBeenCalledWith('sq32h3ch', {
      contact_id: 'contact-1',
      alert_type: 'below',
      threshold_sats: 25000000,
    })

    await user.click(screen.getByRole('button', { name: 'Delete threshold' }))
    await waitFor(() => expect(mockApi.deleteBalanceAlert).toHaveBeenCalledWith('alert-1'))
  })

  it('shows legacy wallet-level balance thresholds when they have no contact', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([], [makeAlert({ contact_id: undefined })])

    await renderLoadedPage()

    expect(screen.getByText('Legacy wallet balance thresholds')).toBeInTheDocument()
    expect(screen.getByText('above 1 BTC')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Delete wallet-level threshold' }))

    await waitFor(() => expect(mockApi.deleteBalanceAlert).toHaveBeenCalledWith('alert-1'))
  })

  it('hides inactive migrated wallet-level balance thresholds', async () => {
    mockNotificationsResponse(
      [makeContact()],
      [
        makeAlert({
          id: 'legacy-alert',
          contact_id: undefined,
          threshold_sats: 0,
          alert_type: 'equals',
          is_active: false,
        }),
        makeAlert({
          id: 'contact-alert',
          contact_id: 'contact-1',
          threshold_sats: 0,
          alert_type: 'equals',
        }),
      ]
    )

    await renderLoadedPage()

    expect(screen.queryByText('Legacy wallet balance thresholds')).not.toBeInTheDocument()
    expect(screen.getByText('equals 0 BTC')).toBeInTheDocument()
  })

  it('keeps inactive standalone wallet-level balance thresholds visible', async () => {
    mockNotificationsResponse(
      [],
      [
        makeAlert({
          contact_id: undefined,
          threshold_sats: 0,
          alert_type: 'equals',
          is_active: false,
        }),
      ]
    )

    await renderLoadedPage()

    expect(screen.getByText('Legacy wallet balance thresholds')).toBeInTheDocument()
    expect(screen.getByText('equals 0 BTC')).toBeInTheDocument()
  })

  it('keeps inactive contact-level balance thresholds visible', async () => {
    mockNotificationsResponse(
      [makeContact()],
      [
        makeAlert({
          contact_id: 'contact-1',
          threshold_sats: 0,
          alert_type: 'equals',
          is_active: false,
        }),
      ]
    )

    await renderLoadedPage()

    expect(screen.queryByText('Legacy wallet balance thresholds')).not.toBeInTheDocument()
    expect(screen.getByText('equals 0 BTC')).toBeInTheDocument()
  })

  it('uses the preferred fiat currency for threshold notifications', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([makeContact()])

    await renderLoadedPage()

    await user.click(screen.getByRole('combobox', { name: 'Threshold currency' }))
    await user.click(await screen.findByText('NOK'))
    await user.type(screen.getByPlaceholderText('10000'), '1000')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    await waitFor(() => expect(mockApi.createBalanceAlert).toHaveBeenCalledTimes(1))
    expect(mockApi.createBalanceAlert).toHaveBeenCalledWith('sq32h3ch', {
      contact_id: 'contact-1',
      alert_type: 'below',
      threshold_currency: 'NOK',
      threshold_fiat_amount: 1000,
    })
  })

  it('does not allow inline editing of email targets without verification', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: true,
      isSelfHostedMode: false,
      user: { id: 1, email: 'test@example.com', subscription_tier: 'team' },
      billingStatus: {
        subscription_tier: 'team',
        subscription_status: 'active',
        stripe_customer_id: 'cus_123',
        limits: { max_wallets: 5, max_contacts_per_wallet: 5, sync_interval_seconds: 60 },
        wallet_count: 1,
        contact_count: 1,
      },
    })
    mockNotificationsResponse([
      makeContact({
        notification_methods: [
          {
            id: 'contact-1-method-1',
            contact_id: 'contact-1',
            provider_type: 'email',
            notification_target: 'alice@example.com',
            display_target: 'alice@example.com',
            created_at: '2024-01-01T00:00:00Z',
            is_enabled: true,
          },
        ],
      }),
    ])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Contact actions' }))
    await user.click(screen.getByText('Edit contact'))

    expect(screen.getByDisplayValue('alice@example.com')).toBeDisabled()
  })

  it('opens the upgrade modal instead of the wizard when the cloud contact limit is reached', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: true,
      isSelfHostedMode: false,
      user: { id: 1, email: 'test@example.com', subscription_tier: 'personal' },
      billingStatus: {
        subscription_tier: 'personal',
        subscription_status: 'trialing',
        stripe_customer_id: 'cus_123',
        limits: { max_wallets: 1, max_contacts_per_wallet: 1, sync_interval_seconds: 600 },
        wallet_count: 1,
        contact_count: 1,
      },
    })
    mockNotificationsResponse([makeContact()])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Add contact' }))

    expect(screen.getByTestId('plans-modal')).toHaveTextContent('contacts:1')
    expect(screen.queryByRole('heading', { name: 'New contact' })).not.toBeInTheDocument()
  })

  it('shows threshold validation errors next to the add threshold controls', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([makeContact()])

    await renderLoadedPage()

    await user.type(screen.getByPlaceholderText('0.10'), 'not-btc')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    const thresholdSection = screen.getByText('Balance threshold notifications').closest('section')
    expect(thresholdSection).not.toBeNull()
    expect(within(thresholdSection!).getByText('Enter a valid BTC amount')).toBeInTheDocument()
  })
})
