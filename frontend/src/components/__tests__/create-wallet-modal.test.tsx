import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { CreateWalletModal } from '../create-wallet-modal'

// Mock the api module
jest.mock('../../lib/api', () => ({
  api: {
    createWallet: jest.fn(),
  },
}))

// Mock the auth context
jest.mock('../../contexts/auth-context', () => ({
  useAuth: jest.fn(),
}))

// Mock the useModal hook
const mockUseModal = {
  isLoading: false,
  error: null,
  setLoading: jest.fn(),
  setError: jest.fn(),
  clearError: jest.fn(),
  reset: jest.fn(),
}

jest.mock('../../hooks/useModal', () => ({
  useModal: () => mockUseModal,
}))

const mockApi = jest.requireMock('../../lib/api').api
const mockUseAuth = jest.requireMock('../../contexts/auth-context').useAuth

// Mock environment variable for auth
const originalEnv = process.env

describe('CreateWalletModal', () => {
  const defaultProps = {
    isOpen: true,
    onClose: jest.fn(),
    onWalletCreated: jest.fn(),
    isFirstWallet: false,
  }

  const mockUser = {
    id: 1,
    phone_number: '+4799999901',
    name: 'Alice Johnson',
    is_admin: false,
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.createWallet.mockResolvedValue({})
    mockUseAuth.mockReturnValue({ user: null })
    process.env = { ...originalEnv }
    // Reset mock functions
    mockUseModal.setLoading.mockClear()
    mockUseModal.setError.mockClear()
    mockUseModal.clearError.mockClear()
    mockUseModal.reset.mockClear()
  })

  afterEach(() => {
    process.env = originalEnv
  })

  describe('Basic Modal Behavior', () => {
    it('renders modal when open', () => {
      render(<CreateWalletModal {...defaultProps} />)
      
      expect(screen.getByText('Add Wallet for Monitoring')).toBeInTheDocument()
      expect(screen.getByLabelText('Wallet Name')).toBeInTheDocument()
      expect(screen.getByLabelText('Output Descriptor or Extended Public Key')).toBeInTheDocument()
    })

    it('does not render when closed', () => {
      render(<CreateWalletModal {...defaultProps} isOpen={false} />)
      
      expect(screen.queryByText('Add Wallet for Monitoring')).not.toBeInTheDocument()
    })

    it('calls onClose when cancel button is clicked', () => {
      render(<CreateWalletModal {...defaultProps} />)
      
      fireEvent.click(screen.getByText('Cancel'))
      expect(defaultProps.onClose).toHaveBeenCalled()
    })
  })

  describe('Wallet Name Prefilling - SAAS Mode', () => {
    beforeEach(() => {
      process.env.NEXT_PUBLIC_CANARY_MODE = 'saas'
      mockUseAuth.mockReturnValue({ user: mockUser })
    })

    it('prefills wallet name with user name when first wallet and in SAAS mode', () => {
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const nameInput = screen.getByLabelText('Wallet Name') as HTMLInputElement
      expect(nameInput.value).toBe('Alice Johnson')
    })

    it('does not prefill name for subsequent wallets even in SAAS mode', () => {
      render(<CreateWalletModal {...defaultProps} isFirstWallet={false} />)
      
      const nameInput = screen.getByLabelText('Wallet Name') as HTMLInputElement
      expect(nameInput.value).toBe('')
    })

    it('does not prefill name when user has no name even if first wallet', () => {
      mockUseAuth.mockReturnValue({ 
        user: { ...mockUser, name: undefined } 
      })
      
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const nameInput = screen.getByLabelText('Wallet Name') as HTMLInputElement
      expect(nameInput.value).toBe('')
    })
  })

  describe('Wallet Name Prefilling - FOSS Mode', () => {
    beforeEach(() => {
      process.env.NEXT_PUBLIC_CANARY_MODE = 'foss'
      mockUseAuth.mockReturnValue({ user: mockUser })
    })

    it('does not prefill name in FOSS mode even for first wallet', () => {
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const nameInput = screen.getByLabelText('Wallet Name') as HTMLInputElement
      expect(nameInput.value).toBe('')
    })
  })

  describe('Focus Management', () => {
    beforeEach(() => {
      process.env.NEXT_PUBLIC_CANARY_MODE = 'saas'
      mockUseAuth.mockReturnValue({ user: mockUser })
    })

    it('focuses descriptor field when name is prefilled (first wallet in SAAS mode)', () => {
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const descriptorTextarea = screen.getByLabelText('Output Descriptor or Extended Public Key')
      expect(document.activeElement).toBe(descriptorTextarea)
    })

    it('focuses name field when name is not prefilled (subsequent wallets)', () => {
      render(<CreateWalletModal {...defaultProps} isFirstWallet={false} />)
      
      const nameInput = screen.getByLabelText('Wallet Name')
      expect(document.activeElement).toBe(nameInput)
    })

    it('focuses name field in FOSS mode', () => {
      process.env.NEXT_PUBLIC_CANARY_MODE = 'foss'
      
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const nameInput = screen.getByLabelText('Wallet Name')
      expect(document.activeElement).toBe(nameInput)
    })
  })

  describe('Form Submission', () => {
    it('successfully adds wallet with valid input', async () => {
      render(<CreateWalletModal {...defaultProps} />)
      
      const nameInput = screen.getByLabelText('Wallet Name')
      const descriptorInput = screen.getByLabelText('Output Descriptor or Extended Public Key')
      const submitButton = screen.getByText('Add Wallet')
      
      fireEvent.change(nameInput, { target: { value: 'My Wallet' } })
      fireEvent.change(descriptorInput, { 
        target: { value: 'wpkh([fingerprint/derivation]xpub.../0/*)#checksum' } 
      })
      
      fireEvent.click(submitButton)
      
      await waitFor(() => {
        expect(mockApi.createWallet).toHaveBeenCalledWith({
          name: 'My Wallet',
          descriptor: 'wpkh([fingerprint/derivation]xpub.../0/*)#checksum',
          isFreshWallet: undefined,
          scriptType: 'p2wpkh'
        })
      })
      
      expect(defaultProps.onWalletCreated).toHaveBeenCalled()
    })

    it('adds wallet with prefilled name when user submits', async () => {
      process.env.NEXT_PUBLIC_CANARY_MODE = 'saas'
      mockUseAuth.mockReturnValue({ user: mockUser })
      
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const descriptorInput = screen.getByLabelText('Output Descriptor or Extended Public Key')
      const submitButton = screen.getByText('Add Wallet')
      
      // Name should be prefilled, just add descriptor
      fireEvent.change(descriptorInput, { 
        target: { value: 'wpkh([fingerprint/derivation]xpub.../0/*)#checksum' } 
      })
      
      fireEvent.click(submitButton)
      
      await waitFor(() => {
        expect(mockApi.createWallet).toHaveBeenCalledWith({
          name: 'Alice Johnson',
          descriptor: 'wpkh([fingerprint/derivation]xpub.../0/*)#checksum',
          isFreshWallet: undefined,
          scriptType: 'p2wpkh'
        })
      })
    })

    it('user can modify prefilled name before submission', async () => {
      process.env.NEXT_PUBLIC_CANARY_MODE = 'saas'
      mockUseAuth.mockReturnValue({ user: mockUser })
      
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const nameInput = screen.getByLabelText('Wallet Name')
      const descriptorInput = screen.getByLabelText('Output Descriptor or Extended Public Key')
      const submitButton = screen.getByText('Add Wallet')
      
      // Modify the prefilled name
      fireEvent.change(nameInput, { target: { value: 'My Personal Wallet' } })
      fireEvent.change(descriptorInput, { 
        target: { value: 'wpkh([fingerprint/derivation]xpub.../0/*)#checksum' } 
      })
      
      fireEvent.click(submitButton)
      
      await waitFor(() => {
        expect(mockApi.createWallet).toHaveBeenCalledWith({
          name: 'My Personal Wallet',
          descriptor: 'wpkh([fingerprint/derivation]xpub.../0/*)#checksum',
          isFreshWallet: undefined,
          scriptType: 'p2wpkh'
        })
      })
    })
  })

  describe('Form Validation', () => {
    it('shows error when wallet name is empty', async () => {
      render(<CreateWalletModal {...defaultProps} />)
      
      const descriptorInput = screen.getByLabelText('Output Descriptor or Extended Public Key')
      const submitButton = screen.getByText('Add Wallet')
      
      fireEvent.change(descriptorInput, { 
        target: { value: 'wpkh([fingerprint/derivation]xpub.../0/*)#checksum' } 
      })
      
      fireEvent.click(submitButton)
      
      expect(mockUseModal.setError).toHaveBeenCalledWith('Wallet name is required')
    })

    it('shows error when descriptor is empty', async () => {
      render(<CreateWalletModal {...defaultProps} />)
      
      const nameInput = screen.getByLabelText('Wallet Name')
      const submitButton = screen.getByText('Add Wallet')
      
      fireEvent.change(nameInput, { target: { value: 'My Wallet' } })
      fireEvent.click(submitButton)
      
      expect(mockUseModal.setError).toHaveBeenCalledWith('Output descriptor or extended public key is required')
    })
  })

  describe('Different Canary Modes', () => {
    it('handles FOSS mode', () => {
      process.env.NEXT_PUBLIC_CANARY_MODE = 'foss'
      mockUseAuth.mockReturnValue({ 
        user: { id: 1, phone_number: 'FOSS', name: 'Admin', is_admin: true } 
      })
      
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const nameInput = screen.getByLabelText('Wallet Name') as HTMLInputElement
      expect(nameInput.value).toBe('')
      expect(document.activeElement).toBe(nameInput)
    })

    it('handles missing environment variable', () => {
      delete process.env.NEXT_PUBLIC_CANARY_MODE
      mockUseAuth.mockReturnValue({ user: mockUser })
      
      render(<CreateWalletModal {...defaultProps} isFirstWallet={true} />)
      
      const nameInput = screen.getByLabelText('Wallet Name') as HTMLInputElement
      expect(nameInput.value).toBe('')
      expect(document.activeElement).toBe(nameInput)
    })
  })
})