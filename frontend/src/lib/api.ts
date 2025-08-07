import { getApiBaseUrl, handleApiResponse } from './utils'
import { Wallet, Contact, TransactionEvent } from '../types'

export interface ProviderInfo {
  name: string
  display_name: string
  config_schema: Record<string, unknown>
}

// Base API client
class ApiClient {
  private baseUrl: string
  private authToken: string | null = null

  constructor() {
    this.baseUrl = getApiBaseUrl()
    // Check for stored token on initialization
    if (typeof window !== 'undefined') {
      this.authToken = localStorage.getItem('auth_token')
    }
  }

  setAuthToken(token: string | null) {
    this.authToken = token
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`
    
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...(options.headers as Record<string, string> || {}),
    }

    // Add auth token if available
    if (this.authToken) {
      headers['Authorization'] = `Bearer ${this.authToken}`
    }
    
    const response = await fetch(url, {
      headers,
      ...options,
    })

    return handleApiResponse(response) as T
  }

  // Wallet API methods
  async createWallet(name: string, descriptor: string): Promise<Wallet> {
    // Send raw browser language - backend will map to supported languages
    const browserLanguage = typeof window !== 'undefined' 
      ? navigator.language
      : 'en'
    
    return this.request<Wallet>('/api/wallets', {
      method: 'POST',
      body: JSON.stringify({ name, descriptor, preferred_language: browserLanguage }),
    })
  }

  async updateWallet(checksum: string, name: string): Promise<Wallet> {
    return this.request<Wallet>(`/api/wallets/${checksum}`, {
      method: 'PUT',
      body: JSON.stringify({ name }),
    })
  }

  async deleteWallet(checksum: string): Promise<void> {
    return this.request<void>(`/api/wallets/${checksum}`, {
      method: 'DELETE',
    })
  }

  async getWallets(): Promise<{ timestamp: number; wallets: Wallet[] }> {
    return this.request<{ timestamp: number; wallets: Wallet[] }>('/api/wallets')
  }

  async getWalletDetail(checksum: string): Promise<{ timestamp: number; wallet: Wallet; events: TransactionEvent[] }> {
    return this.request<{ timestamp: number; wallet: Wallet; events: TransactionEvent[] }>(`/api/wallets/${checksum}/detail`)
  }

  // Contact API methods
  async getWalletContacts(walletChecksum: string): Promise<Contact[]> {
    return this.request<Contact[]>(`/api/wallets/${walletChecksum}/contacts`)
  }

  async createContact(
    walletChecksum: string, 
    name: string,
    language: 'en' | 'no',
    notificationMethods: Array<{ provider_type: 'sms' | 'ntfy', notification_target: string }>
  ): Promise<Contact> {
    return this.request<Contact>(`/api/wallets/${walletChecksum}/contacts`, {
      method: 'POST',
      body: JSON.stringify({
        name,
        language,
        notification_methods: notificationMethods,
      }),
    })
  }

  async sendContactVerification(
    walletChecksum: string,
    name: string,
    language: string,
    phoneNumber: string
  ): Promise<{ message: string }> {
    return this.request<{ message: string }>(`/api/wallets/${walletChecksum}/contacts/send-verification`, {
      method: 'POST',
      body: JSON.stringify({
        name,
        language,
        phone_number: phoneNumber,
      }),
    })
  }

  async verifyContact(
    walletChecksum: string,
    phoneNumber: string,
    code: string
  ): Promise<{ message: string; contact_id: number }> {
    return this.request<{ message: string; contact_id: number }>(`/api/wallets/${walletChecksum}/contacts/verify`, {
      method: 'POST',
      body: JSON.stringify({
        phone_number: phoneNumber,
        code,
      }),
    })
  }

  async deleteContact(walletChecksum: string, contactId: number): Promise<void> {
    return this.request<void>(`/api/wallets/${walletChecksum}/contacts/${contactId}`, {
      method: 'DELETE',
    })
  }


  // Provider API methods
  async getProviders(): Promise<{ providers: ProviderInfo[] }> {
    return this.request<{ providers: ProviderInfo[] }>('/api/providers')
  }

  // Block header API methods
  async getCurrentBlockHeader(): Promise<unknown> {
    return this.request<unknown>('/api/block-headers/current')
  }

  // Auth API methods
  async sendOtp(phoneNumber: string): Promise<{ message: string }> {
    return this.request<{ message: string }>('/api/auth/send-otp', {
      method: 'POST',
      body: JSON.stringify({ phone_number: phoneNumber }),
    })
  }

  async verifyOtp(phoneNumber: string, code: string, name?: string): Promise<{ token: string; user: { id: number; phone_number: string; name?: string; is_admin: boolean }; requires_name?: boolean }> {
    return this.request<{ token: string; user: { id: number; phone_number: string; name?: string; is_admin: boolean }; requires_name?: boolean }>('/api/auth/verify-otp', {
      method: 'POST',
      body: JSON.stringify({ phone_number: phoneNumber, code, name }),
    })
  }

  async logout(): Promise<void> {
    return this.request<void>('/api/auth/logout', {
      method: 'POST',
    })
  }

  async getMe(): Promise<{ user: { id: number, phone_number: string, name?: string, is_admin: boolean } }> {
    return this.request<{ user: { id: number, phone_number: string, name?: string, is_admin: boolean } }>('/api/auth/me')
  }

  async updateUserProfile(name: string): Promise<{ user: { id: number, phone_number: string, name?: string, is_admin: boolean } }> {
    return this.request<{ user: { id: number, phone_number: string, name?: string, is_admin: boolean } }>('/api/auth/user', {
      method: 'PUT',
      body: JSON.stringify({ name }),
    })
  }
}

// Export a singleton instance
export const api = new ApiClient()