import { getApiBaseUrl, handleApiResponse } from './utils'
import { Wallet, Contact, TwilioConfig } from '../types'

// Base API client
class ApiClient {
  private baseUrl: string

  constructor() {
    this.baseUrl = getApiBaseUrl()
  }

  private async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseUrl}${endpoint}`
    
    const response = await fetch(url, {
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
      ...options,
    })

    return handleApiResponse(response)
  }

  // Wallet API methods
  async createWallet(name: string, descriptor: string): Promise<Wallet> {
    return this.request<Wallet>('/api/wallets', {
      method: 'POST',
      body: JSON.stringify({ name, descriptor }),
    })
  }

  async updateWallet(id: number, name: string): Promise<Wallet> {
    return this.request<Wallet>(`/api/wallets/${id}`, {
      method: 'PUT',
      body: JSON.stringify({ name }),
    })
  }

  async deleteWallet(id: number): Promise<void> {
    return this.request<void>(`/api/wallets/${id}`, {
      method: 'DELETE',
    })
  }

  // Contact API methods
  async getWalletContacts(walletId: number): Promise<Contact[]> {
    return this.request<Contact[]>(`/api/wallets/${walletId}/contacts`)
  }

  async createContact(walletId: number, name: string, phoneNumber: string): Promise<Contact> {
    return this.request<Contact>(`/api/wallets/${walletId}/contacts`, {
      method: 'POST',
      body: JSON.stringify({
        name,
        phone_number: phoneNumber,
      }),
    })
  }

  async deleteContact(walletId: number, contactId: number): Promise<void> {
    return this.request<void>(`/api/wallets/${walletId}/contacts/${contactId}`, {
      method: 'DELETE',
    })
  }

  // Twilio API methods
  async getTwilioConfig(): Promise<TwilioConfig> {
    return this.request<TwilioConfig>('/api/twilio/config')
  }

  async saveTwilioConfig(config: TwilioConfig & { skip_validation?: boolean }): Promise<{ message: string }> {
    return this.request<{ message: string }>('/api/twilio/config', {
      method: 'POST',
      body: JSON.stringify(config),
    })
  }

  // Block header API methods
  async getCurrentBlockHeader(): Promise<unknown> {
    return this.request<unknown>('/api/block-headers/current')
  }
}

// Export a singleton instance
export const api = new ApiClient()