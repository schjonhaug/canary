'use client'

import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react'
import { useRouter } from 'next/navigation'
import { api } from '@/lib/api'

interface User {
  id: number
  phone_number: string
  is_admin: boolean
}

interface AuthContextType {
  user: User | null
  token: string | null
  isLoading: boolean
  isAuthenticated: boolean
  login: (phone: string, code: string) => Promise<void>
  sendOtp: (phone: string) => Promise<void>
  logout: () => void
  // Development mode functions
  devLogin: (phone: string) => Promise<void>
  isDevMode: boolean
}

const AuthContext = createContext<AuthContextType | undefined>(undefined)

// Development mode configuration
// NOTE: This is a custom dev mode implementation, NOT Twilio's official test patterns.
// These phone numbers bypass our backend's Twilio Verify integration in development.
const DEV_MODE = process.env.NODE_ENV === 'development'
const DEV_ADMIN_PHONE = '+4799999900' // Custom admin number for dev mode (Norway)

// Dev mode test phone numbers - these bypass Twilio Verify in development
// Using clearly non-standard numbers with different country codes to avoid confusion
const DEV_TEST_PHONES = [
  '+4799999901', // Norway country code
  '+4699999902', // Sweden country code
  '+3399999903'  // France country code
]

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const router = useRouter()

  // Check for existing session on mount
  useEffect(() => {
    const storedToken = localStorage.getItem('auth_token')
    if (storedToken) {
      setToken(storedToken)
      // Set token in API client
      api.setAuthToken(storedToken)
      // Fetch user info
      fetchUser(storedToken)
    } else {
      setIsLoading(false)
    }
  }, [])

  const fetchUser = async (authToken: string) => {
    try {
      const { user: userData } = await api.getMe()
      setUser(userData)
    } catch (error) {
      console.error('Failed to fetch user:', error)
      // Token invalid, clear it
      localStorage.removeItem('auth_token')
      setToken(null)
      api.setAuthToken(null)
    } finally {
      setIsLoading(false)
    }
  }

  const sendOtp = async (phone: string) => {
    try {
      await api.sendOtp(phone)
    } catch (error) {
      throw error
    }
  }

  const login = async (phone: string, code: string) => {
    try {
      const data = await api.verifyOtp(phone, code)
      setToken(data.token)
      setUser(data.user)
      localStorage.setItem('auth_token', data.token)
      api.setAuthToken(data.token)
      router.push('/')
    } catch (error) {
      throw error
    }
  }

  // Development mode login - bypasses Twilio Verify using hardcoded test code
  const devLogin = async (phone: string) => {
    if (!DEV_MODE) {
      throw new Error('Development mode login only available in development')
    }

    try {
      // For dev test phones, backend will bypass Twilio and accept any code
      await sendOtp(phone)
      
      // Use "123456" as the hardcoded dev verification code
      const data = await api.verifyOtp(phone, '123456')
      
      setToken(data.token)
      setUser(data.user)
      localStorage.setItem('auth_token', data.token)
      api.setAuthToken(data.token)
      router.push('/')
    } catch (error) {
      throw error
    }
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
    localStorage.removeItem('auth_token')
    api.setAuthToken(null)
    router.push('/login')
  }

  return (
    <AuthContext.Provider
      value={{
        user,
        token,
        isLoading,
        isAuthenticated: !!token,
        login,
        sendOtp,
        logout,
        devLogin,
        isDevMode: DEV_MODE,
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