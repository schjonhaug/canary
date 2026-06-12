import React from 'react'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import WalletNotificationsPage from '../page'
import type { BalanceAlert, Contact, Wallet, WalletNotificationsResponse } from '@/types'
import { ApiError, api } from '@/lib/api'

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
  ApiError: class ApiError extends Error {
    errorCode: string | null

    constructor(message: string, _type?: string, _statusCode?: number | null, errorCode?: string | null) {
      super(message)
      this.errorCode = errorCode ?? null
    }

    getUserFriendlyMessage() {
      return this.message
    }
  },
  api: {
    getWalletNotifications: jest.fn(),
    createContact: jest.fn(),
    updateContact: jest.fn(),
    deleteContact: jest.fn(),
    createBalanceAlert: jest.fn(),
    validateBalanceAlert: jest.fn(),
    deleteBalanceAlert: jest.fn(),
    getUserPreferences: jest.fn(),
    getProviders: jest.fn(),
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
    verificationMock.isVerified = true
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
    mockApi.validateBalanceAlert.mockResolvedValue(undefined)
    mockApi.deleteBalanceAlert.mockResolvedValue(undefined)
    mockApi.getUserPreferences.mockResolvedValue({ preferred_fiat_currency: 'NOK' })
    mockApi.getProviders.mockResolvedValue({
      providers: [
        { name: 'ntfy', display_name: 'ntfy', config_schema: {} },
        { name: 'nostr', display_name: 'Nostr', config_schema: {} },
      ],
    })
  })

  it('sorts contacts by name and renders the transaction notification groups', async () => {
    mockNotificationsResponse([
      makeContact({ id: 'contact-2', name: 'Zoe' }),
      makeContact({ id: 'contact-1', name: 'alice' }),
      makeContact({
        id: 'contact-3',
        name: 'Bob',
        notification_methods: [
          {
            id: 'contact-3-method-1',
            contact_id: 'contact-3',
            provider_type: 'sms',
            notification_target: '+4792050946',
            display_target: '+4792050946',
            created_at: '2024-01-01T00:00:00Z',
            is_enabled: true,
          },
        ],
      }),
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
    expect(screen.getByText('SMS: +47 92 05 09 46')).toBeInTheDocument()
  })

  it('hides contact creation and locks transaction checkboxes for cloud read-only users', async () => {
    const user = userEvent.setup()
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: true,
      isSelfHostedMode: false,
      user: {
        id: 1,
        email: 'demo@canarybitcoin.com',
        is_admin: false,
        is_demo: true,
        email_verified: true,
      },
      billingStatus: null,
    })
    mockNotificationsResponse([makeContact({ notify_rbf: true })])

    await renderLoadedPage()

    expect(screen.queryByRole('button', { name: 'Add contact' })).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Contact actions')).not.toBeInTheDocument()

    const rbfCheckbox = screen.getByRole('checkbox', { name: /RBF replacement/i })
    expect(rbfCheckbox).toBeDisabled()
    expect(rbfCheckbox).toHaveClass('cursor-not-allowed')

    await user.click(screen.getByText('RBF replacement'))
    expect(mockApi.updateContact).not.toHaveBeenCalled()
  })

  it('hides contact creation for cloud admins', async () => {
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: true,
      isSelfHostedMode: false,
      user: {
        id: 1,
        email: 'admin@example.com',
        is_admin: true,
        is_demo: false,
        email_verified: true,
      },
      billingStatus: null,
    })
    mockNotificationsResponse([makeContact()])

    await renderLoadedPage()

    expect(screen.queryByRole('button', { name: 'Add contact' })).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Contact actions')).not.toBeInTheDocument()
    expect(screen.getByRole('checkbox', { name: /Sending/i })).toBeDisabled()
  })

  it('uses the provider selector as the only visible method label for new cloud contacts', async () => {
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
        contact_count: 0,
      },
    })
    mockNotificationsResponse([])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Add contact' }))
    await user.type(screen.getByLabelText('New contact name'), 'Alice')

    expect(screen.getByRole('combobox', { name: 'Delivery method' })).toHaveTextContent('Email')
    expect(screen.getAllByText('Email')).toHaveLength(1)
    await user.click(screen.getByRole('combobox', { name: 'Delivery method' }))
    expect(screen.queryByRole('option', { name: 'Nostr' })).not.toBeInTheDocument()
  })

  it('hides Nostr creation in self-hosted mode when the provider is not registered', async () => {
    const user = userEvent.setup()
    mockApi.getProviders.mockResolvedValue({
      providers: [{ name: 'ntfy', display_name: 'ntfy', config_schema: {} }],
    })
    mockNotificationsResponse([])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Add contact' }))

    expect(screen.queryByRole('combobox', { name: 'Delivery method' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('ntfy Topic')).toBeInTheDocument()
    expect(screen.queryByText('Nostr')).not.toBeInTheDocument()
  })

  it('creates an ntfy contact inline with selected notification settings', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Add contact' }))

    expect(screen.getByRole('heading', { name: 'New contact' })).toBeInTheDocument()

    await user.type(screen.getByLabelText('New contact name'), 'Nora')
    expect(screen.getByText('Include wallet balance in transaction notifications')).toBeInTheDocument()
    await user.click(screen.getByText('RBF replacement'))
    await user.type(screen.getByPlaceholderText('0.10'), '0.25')
    await user.click(screen.getByRole('button', { name: 'Add' }))
    expect(await screen.findByText('below 0.25 BTC')).toBeInTheDocument()
    expect(mockApi.validateBalanceAlert).toHaveBeenCalledWith('sq32h3ch', {
      alert_type: 'below',
      threshold_sats: 25000000,
      threshold_currency: undefined,
      threshold_fiat_amount: undefined,
    })
    expect(screen.getByLabelText('ntfy Topic')).toHaveValue('nora-sq32h3ch')
    await user.click(screen.getByRole('button', { name: 'Create contact' }))

    await waitFor(() => expect(mockApi.createContact).toHaveBeenCalledTimes(1))
    expect(mockApi.createContact).toHaveBeenCalledWith(
      'sq32h3ch',
      'Nora',
      [
        {
          provider_type: 'ntfy',
          notification_target: 'nora-sq32h3ch',
          is_enabled: true,
        },
      ],
      {
        notify_sending: true,
        notify_sent: true,
        notify_receiving: true,
        notify_received: true,
        notify_cpfp: false,
        notify_rbf: true,
        include_wallet_balance_in_tx_notifications: false,
      }
    )
    await waitFor(() => expect(mockApi.createBalanceAlert).toHaveBeenCalledTimes(1))
    expect(mockApi.createBalanceAlert).toHaveBeenCalledWith('sq32h3ch', {
      contact_id: 'contact-1',
      alert_type: 'below',
      threshold_sats: 25000000,
      threshold_currency: undefined,
      threshold_fiat_amount: undefined,
    })
  })

  it('validates new contact draft thresholds before adding them', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([])
    mockApi.validateBalanceAlert.mockRejectedValueOnce(
      new ApiError(
        'This alert would trigger immediately based on the current balance. Try a different threshold or alert type.',
        'validation',
        400,
        'alert_would_trigger_immediately'
      )
    )

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Add contact' }))

    await user.type(screen.getByPlaceholderText('0.10'), '0.1')
    await user.click(screen.getByRole('button', { name: 'Add' }))

    const thresholdSection = screen.getByText('Balance threshold notifications').closest('section')
    expect(thresholdSection).not.toBeNull()
    expect(
      await within(thresholdSection!).findByText(
        'This alert would trigger immediately based on the current balance. Try a different threshold or alert type.'
      )
    ).toBeInTheDocument()
    expect(screen.queryByText('below 0.1 BTC')).not.toBeInTheDocument()
    expect(mockApi.validateBalanceAlert).toHaveBeenCalledWith('sq32h3ch', {
      alert_type: 'below',
      threshold_sats: 10000000,
      threshold_currency: undefined,
      threshold_fiat_amount: undefined,
    })
  })

  it('keeps a manually edited ntfy topic when the new contact name changes', async () => {
    const user = userEvent.setup()
    mockNotificationsResponse([])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Add contact' }))

    const nameInput = screen.getByLabelText('New contact name')
    const topicInput = screen.getByLabelText('ntfy Topic')

    await user.type(nameInput, 'Nora')
    expect(topicInput).toHaveValue('nora-sq32h3ch')

    await user.clear(topicInput)
    await user.type(topicInput, 'custom-topic')
    await user.clear(nameInput)
    await user.type(nameInput, 'New Nora')

    expect(topicInput).toHaveValue('custom-topic')
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

  it('autosaves SMS contacts with the stored target instead of the display target', async () => {
    const user = userEvent.setup()
    const contact = makeContact({
      notification_methods: [
        {
          id: 'contact-1-method-1',
          contact_id: 'contact-1',
          provider_type: 'sms',
          notification_target: '+4792050946',
          display_target: '+47 92 05 09 46',
          created_at: '2024-01-01T00:00:00Z',
          is_enabled: true,
        },
      ],
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
          provider_type: 'sms',
          notification_target: '+4792050946',
          is_enabled: true,
        },
      ],
      expect.objectContaining({
        notify_rbf: true,
      })
    )
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

  it('ignores wallet-level balance thresholds without a contact', async () => {
    mockNotificationsResponse([], [makeAlert({ contact_id: undefined })])

    await renderLoadedPage()

    expect(screen.queryByText('Legacy wallet balance thresholds')).not.toBeInTheDocument()
    expect(screen.queryByText('above 1 BTC')).not.toBeInTheDocument()
    expect(mockApi.deleteBalanceAlert).not.toHaveBeenCalled()
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

  it('blocks saving a changed email target until it is verified', async () => {
    const user = userEvent.setup()
    verificationMock.isVerified = false
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
    await user.clear(screen.getByDisplayValue('alice@example.com'))
    await user.type(screen.getByPlaceholderText('your@email.com'), 'alice+new@example.com')
    await user.click(screen.getByRole('button', { name: /Save contact/ }))

    expect(
      screen.getByText('Please verify the new email address before saving the contact')
    ).toBeInTheDocument()
    expect(mockApi.updateContact).not.toHaveBeenCalled()
  })

  it('allows adding email to an existing cloud contact that only has SMS', async () => {
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
            provider_type: 'sms',
            notification_target: '+4799999999',
            display_target: '+4799999999',
            created_at: '2024-01-01T00:00:00Z',
            is_enabled: true,
          },
        ],
      }),
    ])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Contact actions' }))
    await user.click(screen.getByText('Edit contact'))
    await user.click(screen.getByRole('combobox', { name: 'Delivery method type' }))
    await user.click(await screen.findByText('Email'))
    await user.click(screen.getByRole('button', { name: 'Add delivery method' }))
    await user.type(screen.getByPlaceholderText('your@email.com'), 'alice@example.com')
    await user.click(screen.getByRole('button', { name: /Save contact/ }))

    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))
    expect(mockApi.updateContact).toHaveBeenCalledWith(
      'sq32h3ch',
      'contact-1',
      'Alice',
      [
        {
          provider_type: 'sms',
          notification_target: '+4799999999',
          is_enabled: true,
        },
        {
          provider_type: 'email',
          notification_target: 'alice@example.com',
          is_enabled: true,
        },
      ],
      expect.any(Object)
    )
  })

  it('blocks saving a new email delivery method until it is verified', async () => {
    const user = userEvent.setup()
    verificationMock.isVerified = false
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
            provider_type: 'sms',
            notification_target: '+4799999999',
            display_target: '+4799999999',
            created_at: '2024-01-01T00:00:00Z',
            is_enabled: true,
          },
        ],
      }),
    ])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Contact actions' }))
    await user.click(screen.getByText('Edit contact'))
    await user.click(screen.getByRole('combobox', { name: 'Delivery method type' }))
    await user.click(await screen.findByText('Email'))
    await user.click(screen.getByRole('button', { name: 'Add delivery method' }))
    await user.type(screen.getByPlaceholderText('your@email.com'), 'alice@example.com')
    await user.click(screen.getByRole('button', { name: /Save contact/ }))

    expect(
      screen.getByText('Please verify the new email address before saving the contact')
    ).toBeInTheDocument()
    expect(mockApi.updateContact).not.toHaveBeenCalled()
  })

  it('blocks saving a new SMS delivery method until it is verified', async () => {
    const user = userEvent.setup()
    verificationMock.isVerified = false
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
    await user.click(screen.getByRole('combobox', { name: 'Delivery method type' }))
    await user.click(await screen.findByText('SMS'))
    await user.click(screen.getByRole('button', { name: 'Add delivery method' }))
    await user.type(screen.getByPlaceholderText('+47 123 45 678'), '+4799999999')
    await user.click(screen.getByRole('button', { name: /Save contact/ }))

    expect(
      screen.getByText('Please verify the new SMS phone number before saving the contact')
    ).toBeInTheDocument()
    expect(mockApi.updateContact).not.toHaveBeenCalled()
  })

  it('allows adding SMS to an existing cloud contact that only has email', async () => {
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
    await user.click(screen.getByRole('combobox', { name: 'Delivery method type' }))
    await user.click(await screen.findByText('SMS'))
    await user.click(screen.getByRole('button', { name: 'Add delivery method' }))
    await user.type(screen.getByPlaceholderText('+47 123 45 678'), '+4799999999')
    await user.click(screen.getByRole('button', { name: /Save contact/ }))

    await waitFor(() => expect(mockApi.updateContact).toHaveBeenCalledTimes(1))
    expect(mockApi.updateContact).toHaveBeenCalledWith(
      'sq32h3ch',
      'contact-1',
      'Alice',
      [
        {
          provider_type: 'email',
          notification_target: 'alice@example.com',
          is_enabled: true,
        },
        {
          provider_type: 'sms',
          notification_target: '+4799999999',
          is_enabled: true,
        },
      ],
      expect.any(Object)
    )
  })

  it('hides the delete action for a new SMS method until it is verified', async () => {
    const user = userEvent.setup()
    verificationMock.isVerified = false
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
    await user.click(screen.getByRole('combobox', { name: 'Delivery method type' }))
    await user.click(await screen.findByText('SMS'))
    await user.click(screen.getByRole('button', { name: 'Add delivery method' }))

    expect(screen.getAllByRole('button', { name: 'Delete delivery method' })).toHaveLength(1)
  })

  it('shows the fallback addable provider after deleting another method without saving', async () => {
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
    await user.click(screen.getByRole('combobox', { name: 'Delivery method type' }))
    await user.click(await screen.findByText('SMS'))
    await user.click(screen.getByRole('button', { name: 'Add delivery method' }))
    await user.click(screen.getAllByRole('button', { name: 'Delete delivery method' })[0])

    expect(screen.getByRole('combobox', { name: 'Delivery method type' })).toHaveTextContent(
      'Email'
    )
  })

  it('allows replacing the only cloud delivery method before saving', async () => {
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

    await user.click(screen.getByRole('button', { name: 'Delete delivery method' }))

    expect(screen.queryByDisplayValue('alice@example.com')).not.toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Delivery method type' })).toHaveTextContent(
      'ntfy'
    )
    expect(screen.getByRole('button', { name: /Save contact/ })).toBeDisabled()
  })

  it('restores an unsaved deleted delivery method when adding the same provider again', async () => {
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
          {
            id: 'contact-1-method-2',
            contact_id: 'contact-1',
            provider_type: 'ntfy',
            notification_target: 'alice-topic',
            display_target: 'alice-topic',
            created_at: '2024-01-01T00:00:00Z',
            is_enabled: true,
          },
        ],
      }),
    ])

    await renderLoadedPage()
    await user.click(screen.getByRole('button', { name: 'Contact actions' }))
    await user.click(screen.getByText('Edit contact'))
    await user.click(screen.getAllByRole('button', { name: 'Delete delivery method' })[0])
    await user.click(screen.getByRole('combobox', { name: 'Delivery method type' }))
    await user.click(await screen.findByRole('option', { name: 'Email' }))
    await user.click(screen.getByRole('button', { name: 'Add delivery method' }))

    expect(screen.getByDisplayValue('alice@example.com')).toBeEnabled()
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
