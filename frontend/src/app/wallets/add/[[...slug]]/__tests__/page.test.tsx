import { render, screen, waitFor, act } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import AddWalletPage from '../page'
import { SAMPLE_WALLET_SLUG } from '@/components/add-wallet-form'

// Mock next/navigation
const mockPush = jest.fn()
const mockReplace = jest.fn()
jest.mock('next/navigation', () => ({
  useRouter: () => ({
    push: mockPush,
    replace: mockReplace,
  }),
}))

// Mock API (for checkout and billing)
jest.mock('../../../../../lib/api', () => ({
  api: {
    getBillingPricing: jest.fn().mockResolvedValue({ tiers: [] }),
    createCheckoutSession: jest.fn(),
  },
}))

// Mock useBlockHeader - default to connected
const mockRefresh = jest.fn()
let useBlockHeaderMockValue = {
  blockHeader: { network: 'regtest', height: 100, timestamp: 1234567890 },
  isConnected: true,
  refresh: mockRefresh,
  isLoading: false,
}

jest.mock('../../../../../hooks/useBlockHeader', () => ({
  useBlockHeader: () => useBlockHeaderMockValue,
}))

// Default wallets context mock
const defaultWalletsContextMock = {
  wallets: [],
  isLoading: false,
  error: null,
  lastUpdate: null,
  isConnected: true,
}

let walletsContextMockValue = { ...defaultWalletsContextMock }

jest.mock('../../../../../contexts/wallets-context', () => ({
  useWalletsContext: () => walletsContextMockValue,
}))

// Default auth mock - self-hosted mode
const defaultAuthMock = {
  user: null,
  billingStatus: null,
  isSelfHostedMode: true,
  isCloudMode: false,
  isLoading: false,
  isAuthenticated: true,
  refreshBillingStatus: jest.fn(),
}

let authMockValue = { ...defaultAuthMock }

jest.mock('../../../../../contexts/auth-context', () => ({
  useAuth: () => authMockValue,
}))

// Helper to render with slug
function renderWithSlug(slug?: string[]) {
  const params = Promise.resolve({ slug })
  return render(<AddWalletPage params={params} />)
}

// Default useBlockHeader mock value
const defaultBlockHeaderMock = {
  blockHeader: { network: 'regtest', height: 100, timestamp: 1234567890 },
  isConnected: true,
  refresh: mockRefresh,
  isLoading: false,
}

describe('AddWalletPage', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    authMockValue = { ...defaultAuthMock }
    walletsContextMockValue = { ...defaultWalletsContextMock }
    useBlockHeaderMockValue = { ...defaultBlockHeaderMock }
  })

  describe('URL Routing', () => {
    it('shows choose step when slug is undefined', async () => {
      await act(async () => {
        renderWithSlug(undefined)
      })

      // Wait for loading to complete and wallet grid to appear
      await waitFor(() => {
        expect(screen.getByText('Sparrow')).toBeInTheDocument()
      })

      // Should show wallet grid
      expect(screen.getByText('BlueWallet')).toBeInTheDocument()
      expect(screen.getByText('Electrum')).toBeInTheDocument()
    })

    it('shows instructions step with form for valid wallet ID', async () => {
      await act(async () => {
        renderWithSlug(['sparrow'])
      })

      await waitFor(() => {
        expect(screen.getByText(/Follow these steps to export your descriptor/)).toBeInTheDocument()
      })

      // Should show Sparrow-specific content
      expect(screen.getByText(/Open your wallet in Sparrow/)).toBeInTheDocument()

      // Should also show form elements on same page
      expect(screen.getByLabelText('Wallet Name')).toBeInTheDocument()
      expect(screen.getByText(/Paste your/)).toBeInTheDocument()
    })

    it('redirects legacy /wallet-id/form URLs to /wallet-id', async () => {
      await act(async () => {
        renderWithSlug(['sparrow', 'form'])
      })

      await waitFor(() => {
        expect(mockReplace).toHaveBeenCalledWith('/wallets/add/sparrow')
      })
    })

    it('shows form step when slug is form (skipped instructions)', async () => {
      await act(async () => {
        renderWithSlug(['form'])
      })

      await waitFor(() => {
        expect(screen.getByText(/Paste your output descriptor or XPUB below/)).toBeInTheDocument()
      })

      expect(screen.getByLabelText('Wallet Name')).toBeInTheDocument()
    })

    it('shows form with prefilled data for bacon wallet', async () => {
      await act(async () => {
        renderWithSlug([SAMPLE_WALLET_SLUG])
      })

      await waitFor(() => {
        expect(screen.getByText(/prefilled the Bacon sample wallet/)).toBeInTheDocument()
      })

      // Should have prefilled name
      const nameInput = screen.getByLabelText('Wallet Name') as HTMLInputElement
      await waitFor(() => {
        expect(nameInput.value).toBe('Bacon')
      })
    })

    it('redirects to choose step for invalid wallet ID', async () => {
      await act(async () => {
        renderWithSlug(['invalid-wallet-id'])
      })

      await waitFor(() => {
        expect(mockReplace).toHaveBeenCalledWith('/wallets/add')
      })
    })
  })

  describe('Self-hosted Mode', () => {
    beforeEach(() => {
      authMockValue = {
        ...defaultAuthMock,
        isSelfHostedMode: true,
        isCloudMode: false,
      }
    })

    it('shows Bacon wallet option for first wallet', async () => {
      walletsContextMockValue = { ...defaultWalletsContextMock, wallets: [] }

      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText('Use Bacon Wallet')).toBeInTheDocument()
      })

      expect(screen.getByText(/Try with a sample wallet/)).toBeInTheDocument()
    })

    it('hides Bacon wallet option when wallets exist', async () => {
      walletsContextMockValue = {
        ...defaultWalletsContextMock,
        wallets: [{ checksum: 'test', name: 'Test Wallet' }] as never[],
      }

      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText('Sparrow')).toBeInTheDocument()
      })

      expect(screen.queryByText('Use Bacon Wallet')).not.toBeInTheDocument()
    })
  })

  describe('Cloud Mode', () => {
    beforeEach(() => {
      authMockValue = {
        ...defaultAuthMock,
        isSelfHostedMode: false,
        isCloudMode: true,
        isAuthenticated: true,
        user: { id: 1, email: 'test@example.com', subscription_tier: 'personal' },
        billingStatus: {
          subscription_tier: 'personal',
          subscription_status: 'active',
        },
      }
    })

    it('hides Bacon wallet option in cloud mode', async () => {
      walletsContextMockValue = { ...defaultWalletsContextMock, wallets: [] }

      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText('Sparrow')).toBeInTheDocument()
      })

      expect(screen.queryByText('Use Bacon Wallet')).not.toBeInTheDocument()
    })

    it('shows upgrade prompt when wallet limit reached', async () => {
      walletsContextMockValue = {
        ...defaultWalletsContextMock,
        wallets: [{ checksum: 'test', name: 'Test Wallet' }] as never[],
      }

      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText('Wallet Limit Reached')).toBeInTheDocument()
      })
    })
  })

  describe('Navigation', () => {
    it('navigates to wallet instructions when wallet is selected', async () => {
      const user = userEvent.setup()

      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText('Sparrow')).toBeInTheDocument()
      })

      await user.click(screen.getByText('Sparrow'))

      expect(mockPush).toHaveBeenCalledWith('/wallets/add/sparrow')
    })

    it('navigates to form when skip link is clicked', async () => {
      const user = userEvent.setup()

      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText(/I already have my output descriptor/)).toBeInTheDocument()
      })

      await user.click(screen.getByText(/I already have my output descriptor/))

      expect(mockPush).toHaveBeenCalledWith('/wallets/add/form')
    })

    it('navigates to bacon form when Bacon wallet is clicked', async () => {
      const user = userEvent.setup()
      walletsContextMockValue = { ...defaultWalletsContextMock, wallets: [] }

      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText('Use Bacon Wallet')).toBeInTheDocument()
      })

      await user.click(screen.getByText('Use Bacon Wallet'))

      expect(mockPush).toHaveBeenCalledWith(`/wallets/add/${SAMPLE_WALLET_SLUG}`)
    })
  })

  describe('Breadcrumb Navigation', () => {
    it('shows correct breadcrumb for choose step', async () => {
      await act(async () => {
        renderWithSlug(undefined)
      })

      await waitFor(() => {
        expect(screen.getByText('Wallets')).toBeInTheDocument()
        expect(screen.getByText('Add Wallet')).toBeInTheDocument()
      })
    })

    it('shows correct breadcrumb for instructions step', async () => {
      await act(async () => {
        renderWithSlug(['sparrow'])
      })

      await waitFor(() => {
        expect(screen.getByText('Wallets')).toBeInTheDocument()
        // There are two "Add Wallet" texts - one in breadcrumb, one as submit button
        expect(screen.getAllByText('Add Wallet').length).toBeGreaterThanOrEqual(1)
        expect(screen.getByText('Sparrow')).toBeInTheDocument()
      })
    })

    it('allows navigating back via breadcrumb', async () => {
      const user = userEvent.setup()

      await act(async () => {
        renderWithSlug(['sparrow'])
      })

      // Find the breadcrumb "Add Wallet" button (not the form submit button)
      // The breadcrumb button is within nav and doesn't have type="submit"
      await waitFor(() => {
        const navElement = screen.getByRole('navigation')
        expect(navElement).toBeInTheDocument()
      })

      const navElement = screen.getByRole('navigation')
      const breadcrumbButton = navElement.querySelector('button')
      expect(breadcrumbButton).toBeInTheDocument()

      await user.click(breadcrumbButton!)

      expect(mockPush).toHaveBeenCalledWith('/wallets/add')
    })
  })

  describe('Electrum Unavailable', () => {
    beforeEach(() => {
      useBlockHeaderMockValue = {
        ...defaultBlockHeaderMock,
        isConnected: false,
      }
    })

    it('shows error message when trying to add wallet while Electrum is unavailable', async () => {
      const user = userEvent.setup()

      await act(async () => {
        renderWithSlug(['form'])
      })

      await waitFor(() => {
        expect(screen.getByLabelText('Wallet Name')).toBeInTheDocument()
      })

      // Fill out the form
      await user.type(screen.getByLabelText('Wallet Name'), 'Test Wallet')
      await user.type(screen.getByLabelText('Wallet Descriptor or XPUB'), 'xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K')

      // Click submit button (use type="submit" to distinguish from breadcrumb button)
      const formSubmitButton = document.querySelector('button[type="submit"]')
      expect(formSubmitButton).toBeInTheDocument()
      await user.click(formSubmitButton!)

      // Should show error message
      await waitFor(() => {
        expect(screen.getByText(/Electrum server is unavailable/i)).toBeInTheDocument()
      })
    })

    it('shows retry button when Electrum is unavailable', async () => {
      const user = userEvent.setup()

      await act(async () => {
        renderWithSlug(['form'])
      })

      await waitFor(() => {
        expect(screen.getByLabelText('Wallet Name')).toBeInTheDocument()
      })

      // Fill out the form and submit
      await user.type(screen.getByLabelText('Wallet Name'), 'Test Wallet')
      await user.type(screen.getByLabelText('Wallet Descriptor or XPUB'), 'xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K')

      const formSubmitButton = document.querySelector('button[type="submit"]')
      await user.click(formSubmitButton!)

      // Should show retry button
      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Retry/i })).toBeInTheDocument()
      })
    })

    it('calls refresh when retry button is clicked', async () => {
      const user = userEvent.setup()

      await act(async () => {
        renderWithSlug(['form'])
      })

      await waitFor(() => {
        expect(screen.getByLabelText('Wallet Name')).toBeInTheDocument()
      })

      // Fill out the form and submit
      await user.type(screen.getByLabelText('Wallet Name'), 'Test Wallet')
      await user.type(screen.getByLabelText('Wallet Descriptor or XPUB'), 'xpub6DEzNop46vmxR49zYWFnMwmEfawSNmAMf6dLH5YKDY463twtvw1XD7ihwJRLPRGZJz799VPFzXHpZu6WdhT29WnaeuChS6aZHZPFmqczR5K')

      const formSubmitButton = document.querySelector('button[type="submit"]')
      await user.click(formSubmitButton!)

      // Wait for retry button and click it
      await waitFor(() => {
        expect(screen.getByRole('button', { name: /Retry/i })).toBeInTheDocument()
      })

      await user.click(screen.getByRole('button', { name: /Retry/i }))

      // Should call refresh
      expect(mockRefresh).toHaveBeenCalled()
    })
  })
})
