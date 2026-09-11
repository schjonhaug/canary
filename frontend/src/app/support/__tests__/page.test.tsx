import React from 'react'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import SupportPage from '../page'
import { api } from '@/lib/api'

const mockPush = jest.fn()
const mockUseAuth = jest.fn()

jest.mock('next/navigation', () => ({
  useRouter: () => ({
    push: mockPush,
  }),
}))

jest.mock('@/contexts/auth-context', () => ({
  useAuth: () => mockUseAuth(),
}))

jest.mock('@/lib/api', () => ({
  ...jest.requireActual('@/lib/api'),
  api: {
    getSupportAccess: jest.fn(),
    createSupportAccess: jest.fn(),
    revokeSupportAccess: jest.fn(),
  },
}))

jest.mock('@/components/wallet-cards', () => ({
  WalletCards: ({ wallets }: { wallets: { name: string }[] }) => (
    <div data-testid="wallet-cards">{wallets.map((wallet) => wallet.name).join(',')}</div>
  ),
}))

describe('SupportPage', () => {
  beforeEach(() => {
    jest.clearAllMocks()
    mockUseAuth.mockReturnValue({
      isAuthenticated: true,
      isLoading: false,
      isCloudMode: true,
      user: { is_admin: true, email: 'admin@example.com' },
    })
    ;(api.getSupportAccess as jest.Mock).mockResolvedValue({
      grant: null,
      timestamp: 1,
      wallets: [],
    })
  })

  it('opens one customer account from email and reason', async () => {
    ;(api.createSupportAccess as jest.Mock).mockResolvedValue({
      grant: {
        target_user_id: 'customer-id',
        target_email: 'alice@example.com',
        reason: 'customer asked about sync',
        expires_at: 1_800_000_000,
      },
      timestamp: 2,
      wallets: [{ name: 'Alice Wallet' }],
    })

    render(<SupportPage />)

    const submit = await screen.findByRole('button', { name: 'Open account' })
    fireEvent.change(screen.getByLabelText('Customer email'), {
      target: { value: 'alice@example.com' },
    })
    fireEvent.change(screen.getByLabelText('Reason'), {
      target: { value: 'customer asked about sync' },
    })
    fireEvent.submit(submit.closest('form')!)

    await waitFor(() => {
      expect(api.createSupportAccess).toHaveBeenCalledWith(
        'alice@example.com',
        'customer asked about sync'
      )
      expect(screen.getByText('Viewing alice@example.com')).toBeInTheDocument()
    })
    expect(screen.getByTestId('wallet-cards')).toHaveTextContent('Alice Wallet')
  })
})
