'use client'

import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react'
import { useRouter } from 'next/navigation'
import { api } from '@/lib/api'

interface User {
  id: number
  phone_number: string
  name?: string
  is_admin: boolean
}

interface AuthContextType {
  user: User | null
  token: string | null
  isLoading: boolean
  isAuthenticated: boolean
  login: (phone: string, code: string, name?: string) => Promise<void>
  setAuth: (token: string, user: User) => Promise<void>
  sendOtp: (phone: string) => Promise<void>
  logout: () => void
}

const AuthContext = createContext<AuthContextType | undefined>(undefined)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const router = useRouter()

  // Check if auth is enabled
  const authEnabled = process.env.NEXT_PUBLIC_AUTH_ENABLED !== 'false'

  // Check for existing session on mount
  useEffect(() => {
    // In FOSS mode, set a default user and skip auth
    if (!authEnabled) {
      setUser({
        id: 1,
        phone_number: 'FOSS',
        name: 'Admin',
        is_admin: true
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
  }, [])

  const fetchUser = async () => {
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

  const login = async (phone: string, code: string, name?: string) => {
    try {
      const data = await api.verifyOtp(phone, code, name)
      setToken(data.token)
      setUser(data.user)
      localStorage.setItem('auth_token', data.token)
      api.setAuthToken(data.token)
      router.push('/')
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
        setAuth,
        sendOtp,
        logout,
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