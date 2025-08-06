"use client"

import { createContext, useContext } from 'react'
import { Wallet } from '@/types'

// Create context for sharing wallet data
interface WalletsContextType {
  wallets: Wallet[]
  error: string | null
  lastUpdate: number | null
  isConnected: boolean
  onCreateWallet: () => void
  currentWallet?: Wallet | null
  setCurrentWallet?: (wallet: Wallet | null) => void
}

export const WalletsContext = createContext<WalletsContextType | null>(null)

export const useWalletsContext = () => {
  const context = useContext(WalletsContext)
  if (!context) {
    throw new Error('useWalletsContext must be used within WalletsLayout')
  }
  return context
}