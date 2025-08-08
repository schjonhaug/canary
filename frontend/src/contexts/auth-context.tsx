'use client'

import React, { createContext, useContext, useState, useEffect, ReactNode } from 'react'
import { useRouter } from 'next/navigation'
import { api } from '@/lib/api'

interface User {
  id: number
  email: string
  name?: string
  is_admin: boolean
  email_verified: boolean
}

interface AuthContextType {
  user: User | null
  token: string | null
  isLoading: boolean
  isAuthenticated: boolean
  register: (email: string, password: string, name: string) => Promise<void>
  login: (email: string, password: string) => Promise<void>
  setAuth: (token: string, user: User) => Promise<void>
  forgotPassword: (email: string) => Promise<void>
  resetPassword: (token: string, password: string) => Promise<void>
  verifyEmail: (token: string) => Promise<void>
  logout: () => void
}

const AuthContext = createContext<AuthContextType | undefined>(undefined)

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null)
  const [token, setToken] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(true)
  const router = useRouter()

  // Check if auth is enabled
  const authEnabled = process.env.NEXT_PUBLIC_AUTH_ENABLED === 'true'
  

  // Check for existing session on mount
  useEffect(() => {
    // In FOSS mode, set a default user and skip auth
    if (!authEnabled) {
      setUser({
        id: 1,
        email: 'admin@foss.mode',
        name: 'Admin',
        is_admin: true,
        email_verified: true
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
  }, [authEnabled])

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

  const register = async (email: string, password: string, name: string) => {
    try {
      await api.register(email, password, name)
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
        register,
        login,
        setAuth,
        forgotPassword,
        resetPassword,
        verifyEmail,
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