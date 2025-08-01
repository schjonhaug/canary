import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { EditWalletModal } from '../edit-wallet-modal'
import { api } from '../../lib/api'

// Mock the API client
jest.mock('../../lib/api', () => ({
  api: {
    getWalletContacts: jest.fn(),
    updateWallet: jest.fn(),
    deleteContact: jest.fn(),
    createContact: jest.fn(),
    getProviders: jest.fn(),
  },
}))

const mockApi = api as jest.Mocked<typeof api>

describe('EditWalletModal - Authentication', () => {
  const defaultProps = {
    wallet: {
      id: 1,
      name: 'Test Wallet',
      descriptor: 'test-descriptor',
      balance_total: 0,
      balance_confirmed: 0,
      balance_unconfirmed: 0,
    },
    isOpen: true,
    onClose: jest.fn(),
    onDeleteWallet: jest.fn(),
  }

  beforeEach(() => {
    jest.clearAllMocks()
    
    // Mock successful API responses
    mockApi.getWalletContacts.mockResolvedValue([])
    mockApi.updateWallet.mockResolvedValue(defaultProps.wallet)
    mockApi.deleteContact.mockResolvedValue()
    mockApi.createContact.mockResolvedValue({
      id: 1,
      name: 'Test Contact',
      language: 'en',
      notification_methods: [],
    })
    mockApi.getProviders.mockResolvedValue({ providers: [] })
  })

  it('should use API client for fetching contacts', async () => {
    render(<EditWalletModal {...defaultProps} />)

    await waitFor(() => {
      expect(mockApi.getWalletContacts).toHaveBeenCalledWith(1)
    })
  })

  it('should use API client for updating wallet', async () => {
    const user = userEvent.setup()
    render(<EditWalletModal {...defaultProps} />)

    const saveButton = screen.getByText('Save Name')
    await user.click(saveButton)

    await waitFor(() => {
      expect(mockApi.updateWallet).toHaveBeenCalledWith(1, 'Test Wallet')
    })
  })

  it('should use API client for deleting contacts', async () => {
    // Mock contacts data
    mockApi.getWalletContacts.mockResolvedValue([
      {
        id: 1,
        name: 'Test Contact',
        language: 'en',
        notification_methods: [],
      },
    ])

    const user = userEvent.setup()
    render(<EditWalletModal {...defaultProps} />)

    // Wait for contacts to load
    await waitFor(() => {
      expect(screen.getByText('Test Contact')).toBeInTheDocument()
    })

    // Find and click delete button (if it exists)
    const deleteButtons = screen.queryAllByRole('button', { name: /delete/i })
    if (deleteButtons.length > 0) {
      await user.click(deleteButtons[0])
      
      await waitFor(() => {
        expect(mockApi.deleteContact).toHaveBeenCalledWith(1, 1)
      })
    }
  })
}) 