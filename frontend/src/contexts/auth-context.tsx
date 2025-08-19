'use client'

import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react'
import { useRouter } from 'next/navigation'
import { api } from '@/lib/api'

interface User {
  id: number
  email: string
  name?: string
  is_admin: boolean
  email_verified: boolean
  subscription_tier?: 'personal' | 'team'
}

interface BillingStatus {
  user_id: string
  subscription_tier: string
  subscription_status: string
  trial_ends_at?: string
  subscription_started_at?: string
  stripe_customer_id?: string
  wallet_count: number
  contact_count: number
  limits: {
    max_wallets: number
    max_contacts_per_wallet: number
    sync_interval_seconds: number
  }
}

interface AuthContextType {
  user: User | null
  token: string | null
  billingStatus: BillingStatus | null
  isLoading: boolean
  isAuthenticated: boolean
  isSaasMode: boolean
  isFossMode: boolean
  register: (email: string, password: string, name: string, marketingEmails?: boolean) => Promise<void>
  login: (email: string, password: string) => Promise<void>
  setAuth: (token: string, user: User) => Promise<void>
  forgotPassword: (email: string) => Promise<void>
  resetPassword: (token: string, password: string) => Promise<void>
  verifyEmail: (token: string) => Promise<void>
  logout: () => void
  refreshBillingStatus: () => Promise<void>
}

const AuthContext = createContext<AuthContextType | undefined>(undefined)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [billingStatus, setBillingStatus] = useState<BillingStatus | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const router = useRouter()

  // Check operating mode
  const mode = process.env.NEXT_PUBLIC_CANARY_MODE || 'saas'
  const isSaasMode = mode === 'saas'
  const isFossMode = mode === 'foss'
  

  // Check for existing session on mount
  useEffect(() => {
    // In FOSS mode, set a default user and skip auth
    if (isFossMode) {
      setUser({
        id: 1,
        email: 'admin@local',
        name: 'Admin',
        is_admin: true,
        email_verified: true,
        subscription_tier: 'team' as const
      })
      setToken('foss-mode')
      setIsLoading(false)
      return
    }

    const storedToken = localStorage.getItem('auth_token')
    if (storedToken) {
      setToken(storedToken)
      // Set token in API client
      api.setAuthToken(storedToken)
      // Fetch user info
      fetchUser()
    } else {
      setIsLoading(false)
    }
  }, [isFossMode])

  const fetchUser = useCallback(async () => {
    try {
      const { user: userData } = await api.getMe()
      setUser(userData)
      // Also fetch billing status if user is authenticated (but don't let it cause logout)
      try {
        await fetchBillingStatus()
      } catch (billingError) {
        console.error('Failed to fetch billing status (non-critical):', billingError)
        // Don't throw - billing status failure shouldn't cause logout
      }
    } catch (error) {
      console.error('Failed to fetch user:', error)
      // Only clear auth on 401 Unauthorized - other errors might be temporary
      if (error instanceof Error && error.message.includes('401')) {
        console.log('Token appears invalid, logging out')
        localStorage.removeItem('auth_token')
        setToken(null)
        api.setAuthToken(null)
      } else {
        console.log('Non-auth error, keeping user logged in:', error instanceof Error ? error.message : String(error))
        // Keep user data but mark as potentially stale
      }
    } finally {
      setIsLoading(false)
    }
  }, [])

  const fetchBillingStatus = useCallback(async () => {
    try {
      const status = await api.getBillingStatus()
      setBillingStatus(status)
      // Update user subscription tier if it differs
      if (user && status.subscription_tier !== user.subscription_tier) {
        setUser(prev => prev ? { ...prev, subscription_tier: status.subscription_tier as 'personal' | 'team' } : null)
      }
    } catch (error) {
      console.error('Failed to fetch billing status:', error)
      // Don't throw - billing status is optional
    }
  }, [user])

  const refreshBillingStatus = useCallback(async () => {
    if (!token) return
    await fetchBillingStatus()
  }, [token, fetchBillingStatus])

  const register = async (email: string, password: string, name: string, marketingEmails: boolean = false) => {
    try {
      await api.register(email, password, name, marketingEmails)
    } catch (error) {
      throw error
    }
  }

  const login = async (email: string, password: string) => {
    try {
      const data = await api.login(email, password)
      setToken(data.token)
      setUser(data.user)
      localStorage.setItem('auth_token', data.token)
      api.setAuthToken(data.token)
      router.push('/')
    } catch (error) {
      throw error
    }
  }

  const forgotPassword = async (email: string) => {
    try {
      await api.forgotPassword(email)
    } catch (error) {
      throw error
    }
  }

  const resetPassword = async (token: string, password: string) => {
    try {
      await api.resetPassword(token, password)
    } catch (error) {
      throw error
    }
  }

  const verifyEmail = async (token: string) => {
    try {
      await api.verifyEmail(token)
    } catch (error) {
      throw error
    }
  }

  const setAuth = async (token: string, user: User) => {
    setToken(token)
    setUser(user)
    localStorage.setItem('auth_token', token)
    api.setAuthToken(token)
    // Small delay to ensure state is propagated before navigation
    await new Promise(resolve => setTimeout(resolve, 100))
    await router.push('/')
  }

  const logout = async () => {
    if (token) {
      try {
        await api.logout()
      } catch (error) {
        console.error('Logout error:', error)
      }
    }
    
    setUser(null)
    setToken(null)
    setBillingStatus(null)
    localStorage.removeItem('auth_token')
    api.setAuthToken(null)
    router.push('/sign-in')
  }

  return (
    <AuthContext.Provider
      value={{
        user,
        token,
        billingStatus,
        isLoading,
        isAuthenticated: !!token,
        isSaasMode,
        isFossMode,
        register,
        login,
        setAuth,
        forgotPassword,
        resetPassword,
        verifyEmail,
        logout,
        refreshBillingStatus,
      }}
    >
      {children}
    </AuthContext.Provider>
  )
}

export function useAuth() {
  const context = useContext(AuthContext)
  if (context === undefined) {
    throw new Error('useAuth must be used within an AuthProvider')
  }
  return context
}