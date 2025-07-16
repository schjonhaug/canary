import { useState, useCallback } from 'react'

export interface ModalState {
  isOpen: boolean
  isLoading: boolean
  error: string | null
}

export interface ModalActions {
  open: () => void
  close: () => void
  setLoading: (loading: boolean) => void
  setError: (error: string | null) => void
  clearError: () => void
  reset: () => void
}

export function useModal(initialOpen = false): ModalState & ModalActions {
  const [isOpen, setIsOpen] = useState(initialOpen)
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const open = useCallback(() => {
    setIsOpen(true)
    setError(null)
  }, [])

  const close = useCallback(() => {
    if (!isLoading) {
      setIsOpen(false)
      setError(null)
    }
  }, [isLoading])

  const clearError = useCallback(() => {
    setError(null)
  }, [])

  const reset = useCallback(() => {
    setIsOpen(false)
    setIsLoading(false)
    setError(null)
  }, [])

  return {
    isOpen,
    isLoading,
    error,
    open,
    close,
    setLoading: setIsLoading,
    setError,
    clearError,
    reset,
  }
}