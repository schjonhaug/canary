'use client'

import React, { createContext, useContext, useState, useEffect, useCallback, useRef, ReactNode } from 'react'
import { useRouter } from 'next/navigation'
import { api } from '@/lib/api'
import { ApiError } from '@/lib/utils'
import { setStoredLocale, clearStoredLocale } from '@/lib/locale'
import { type Locale, locales } from '@/i18n/config'

interface User {
  id: number
  email: string
  name?: string
  is_admin: boolean
  is_demo: boolean
  email_verified: boolean
  subscription_tier?: 'personal' | 'team'
  preferred_language?: string
}

interface BillingStatus {
  user_id: string
  subscription_tier: string
  subscription_status: string
  trial_ends_at?: string
  subscription_started_at?: string
  subscription_ends_at?: string
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
  billingStatus: BillingStatus | null
  isLoading: boolean
  isAuthenticated: boolean
  isCloudMode: boolean
  isSelfHostedMode: boolean
  register: (email: string, password: string, name: string, marketingEmails?: boolean) => Promise<void>
  login: (email: string, password: string) => Promise<void>
  demoLogin: () => Promise<void>
  setAuth: (user: User) => Promise<void>
  forgotPassword: (email: string) => Promise<void>
  resetPassword: (token: string, password: string) => Promise<void>
  verifyEmail: (token: string) => Promise<void>
  logout: () => void
  refreshBillingStatus: () => Promise<void>
}

const AuthContext = createContext<AuthContextType | undefined>(undefined)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [billingStatus, setBillingStatus] = useState<BillingStatus | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const userRef = useRef<User | null>(null)
  const router = useRouter()

  // Check operating mode - REQUIRED configuration
  const mode = process.env.NEXT_PUBLIC_CANARY_MODE
  if (!mode) {
    throw new Error(
      'NEXT_PUBLIC_CANARY_MODE is required. Set it in your .env.local file.\n' +
      'Valid values: cloud, self-hosted\n\n' +
      'To get started:\n' +
      '  - For self-hosted mode: cp .env.example.self-hosted .env.local\n' +
      '  - For cloud mode: cp .env.example.cloud .env.local'
    )
  }
  if (mode !== 'cloud' && mode !== 'self-hosted') {
    throw new Error(
      `Invalid NEXT_PUBLIC_CANARY_MODE: '${mode}'. Valid values: cloud, self-hosted`
    )
  }
  const isCloudMode = mode === 'cloud'
  const isSelfHostedMode = mode === 'self-hosted'


  // Keep userRef in sync for use in stable callbacks
  useEffect(() => {
    userRef.current = user
  }, [user])

  // Sync locale cookie from user's stored preference (used on page refresh when already logged in)
  const syncLocaleFromUser = useCallback((userData: User) => {
    if (userData.preferred_language && locales.includes(userData.preferred_language as Locale)) {
      setStoredLocale(userData.preferred_language as Locale)
    }
  }, [])

  const fetchUser = useCallback(async (): Promise<boolean> => {
    try {
      const { user: userData } = await api.getMe()
      setUser(userData)
      syncLocaleFromUser(userData)
      return true
    } catch (error) {
      // Clear user state on 401/403 authentication errors - session is invalid or expired
      // This is expected when not logged in, so don't log as error
      if (error instanceof ApiError && error.isAuthError()) {
        setUser(null)
      } else {
        // Only log unexpected errors
        console.error('Failed to fetch user:', error)
      }
      return false
    } finally {
      setIsLoading(false)
    }
  }, [syncLocaleFromUser])

  const fetchBillingStatus = useCallback(async () => {
    try {
      const status = await api.getBillingStatus()
      setBillingStatus(status)
      // Update user subscription tier if it differs - use functional updater
      // to avoid depending on `user` in the dependency array, which would cause
      // fetchBillingStatus to get a new reference on every user change
      setUser(prev => {
        if (prev && status.subscription_tier !== prev.subscription_tier) {
          return { ...prev, subscription_tier: status.subscription_tier as 'personal' | 'team' }
        }
        return prev
      })
    } catch (error) {
      // Auth errors are expected when session expires - don't log as error
      if (!(error instanceof ApiError && error.isAuthError())) {
        console.error('Failed to fetch billing status:', error)
      }
      // Don't throw - billing status is optional
    }
  }, [])

  // Check for existing session on mount by calling /api/auth/me
  // The HttpOnly cookie will be sent automatically with the request
  useEffect(() => {
    // In self-hosted mode, set a default user and skip auth
    if (isSelfHostedMode) {
      setUser({
        id: 1,
        email: 'admin@local',
        name: 'Admin',
        is_admin: true,
        is_demo: false,
        email_verified: true,
        subscription_tier: 'team' as const
      })
      setIsLoading(false)
      return
    }

    // In cloud mode, check if we have a valid session by fetching user info
    // The HttpOnly auth cookie will be sent automatically
    fetchUser()
  }, [isSelfHostedMode, fetchUser])

  // Fetch billing status after user is loaded
  useEffect(() => {
    if (user && isCloudMode) {
      fetchBillingStatus().catch(error => {
        console.error('Failed to fetch billing status (non-critical):', error)
      })
    }
  }, [user, isCloudMode, fetchBillingStatus])

  const refreshBillingStatus = useCallback(async () => {
    if (!userRef.current) return
    await fetchBillingStatus()
  }, [fetchBillingStatus])

  const register = async (email: string, password: string, name: string, marketingEmails: boolean = false) => {
    await api.register(email, password, name, marketingEmails)
  }

  const login = async (email: string, password: string) => {
    // The login API will set an HttpOnly cookie with the JWT
    const data = await api.login(email, password)
    setUser(data.user)

    // Set locale cookie and force full page reload to apply new locale
    if (data.user.preferred_language && locales.includes(data.user.preferred_language as Locale)) {
      setStoredLocale(data.user.preferred_language as Locale)
      // Force hard navigation to re-run server-side locale detection
      window.location.href = '/'
    } else {
      router.push('/')
    }
  }

  const demoLogin = async () => {
    // The demo login API will set an HttpOnly cookie with the JWT
    const data = await api.demoLogin()
    setUser(data.user)

    // Set locale cookie and force full page reload to apply new locale
    if (data.user.preferred_language && locales.includes(data.user.preferred_language as Locale)) {
      setStoredLocale(data.user.preferred_language as Locale)
      window.location.href = '/'
    } else {
      router.push('/')
    }
  }

  const forgotPassword = async (email: string) => {
    await api.forgotPassword(email)
  }

  const resetPassword = async (token: string, password: string) => {
    await api.resetPassword(token, password)
  }

  const verifyEmail = async (token: string) => {
    await api.verifyEmail(token)
  }

  // setAuth is used after successful OAuth flows or similar
  // The cookie should already be set by the backend
  const setAuth = async (user: User) => {
    setUser(user)
    // Small delay to ensure state is propagated before navigation
    await new Promise(resolve => setTimeout(resolve, 100))
    await router.push('/')
  }

  const logout = async () => {
    if (user) {
      try {
        // The logout API will clear the HttpOnly cookie
        await api.logout()
      } catch (error) {
        console.error('Logout error:', error)
      }
    }

    // Clear locale cookie so next user doesn't inherit this user's language
    clearStoredLocale()

    setUser(null)
    setBillingStatus(null)
    // Don't redirect here - let the calling component handle navigation
  }

  return (
    <AuthContext.Provider
      value={{
        user,
        billingStatus,
        isLoading,
        // Derive isAuthenticated from validated user state, not token presence
        isAuthenticated: !!user,
        isCloudMode,
        isSelfHostedMode,
        register,
        login,
        demoLogin,
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
