import React from 'react'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { InlineWalletNameEdit } from '../inline-wallet-name-edit'

// Mock the api module
jest.mock('../../lib/api', () => ({
  api: {
    updateWallet: jest.fn(),
  },
}))

const mockApi = jest.requireMock('../../lib/api').api

describe('InlineWalletNameEdit', () => {
  const defaultProps = {
    walletChecksum: 'test-checksum',
    currentName: 'Test Wallet',
    onNameUpdated: jest.fn(),
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockApi.updateWallet.mockResolvedValue({})
  })

  it('displays wallet name initially', () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    expect(screen.getByText('Test Wallet')).toBeInTheDocument()
  })

  it('shows edit button on hover', () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    const editButton = screen.getByRole('button')
    expect(editButton).toBeInTheDocument()
  })

  it('enters edit mode when edit button is clicked', () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    expect(screen.getByDisplayValue('Test Wallet')).toBeInTheDocument()
    // Check for buttons by their SVG icons
    const buttons = screen.getAllByRole('button')
    expect(buttons).toHaveLength(2) // save and cancel buttons
  })

  it('cancels edit mode when cancel button is clicked', () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    // Enter edit mode
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    // Cancel - second button should be cancel
    const buttons = screen.getAllByRole('button')
    const cancelButton = buttons[1] // second button is cancel
    fireEvent.click(cancelButton)
    
    expect(screen.getByText('Test Wallet')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('Test Wallet')).not.toBeInTheDocument()
  })

  it('saves wallet name when save button is clicked', async () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    // Enter edit mode
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    // Change name
    const input = screen.getByDisplayValue('Test Wallet')
    fireEvent.change(input, { target: { value: 'Updated Wallet' } })
    
    // Save - first button should be save
    const buttons = screen.getAllByRole('button')
    const saveButton = buttons[0] // first button is save
    fireEvent.click(saveButton)
    
    await waitFor(() => {
      expect(mockApi.updateWallet).toHaveBeenCalledWith('test-checksum', 'Updated Wallet')
    })
    
    await waitFor(() => {
      expect(defaultProps.onNameUpdated).toHaveBeenCalledWith('Updated Wallet')
    })
  })

  it('saves wallet name when Enter key is pressed', async () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    // Enter edit mode
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    // Change name and press Enter
    const input = screen.getByDisplayValue('Test Wallet')
    fireEvent.change(input, { target: { value: 'Updated Wallet' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    
    await waitFor(() => {
      expect(mockApi.updateWallet).toHaveBeenCalledWith('test-checksum', 'Updated Wallet')
    })
  })

  it('cancels edit mode when Escape key is pressed', () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    // Enter edit mode
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    // Press Escape
    const input = screen.getByDisplayValue('Test Wallet')
    fireEvent.keyDown(input, { key: 'Escape' })
    
    expect(screen.getByText('Test Wallet')).toBeInTheDocument()
    expect(screen.queryByDisplayValue('Test Wallet')).not.toBeInTheDocument()
  })

  it('shows error message when update fails', async () => {
    mockApi.updateWallet.mockRejectedValue(new Error('Update failed'))
    
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    // Enter edit mode and try to save
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    const input = screen.getByDisplayValue('Test Wallet')
    fireEvent.change(input, { target: { value: 'Updated Wallet' } })
    
    const buttons = screen.getAllByRole('button')
    const saveButton = buttons[0] // first button is save
    fireEvent.click(saveButton)
    
    await waitFor(() => {
      expect(screen.getByText('Update failed')).toBeInTheDocument()
    })
  })

  it('prevents saving empty wallet name', () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    // Enter edit mode
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    // Clear name
    const input = screen.getByDisplayValue('Test Wallet')
    fireEvent.change(input, { target: { value: '' } })
    
    // Save button should be disabled
    const buttons = screen.getAllByRole('button')
    const saveButton = buttons[0] // first button is save
    expect(saveButton).toBeDisabled()
  })

  it('does not call API when name is unchanged', async () => {
    render(<InlineWalletNameEdit {...defaultProps} />)
    
    // Enter edit mode
    const editButton = screen.getByRole('button')
    fireEvent.click(editButton)
    
    // Save without changing name
    const buttons = screen.getAllByRole('button')
    const saveButton = buttons[0] // first button is save
    fireEvent.click(saveButton)
    
    await waitFor(() => {
      expect(screen.getByText('Test Wallet')).toBeInTheDocument()
    })
    
    expect(mockApi.updateWallet).not.toHaveBeenCalled()
  })
})