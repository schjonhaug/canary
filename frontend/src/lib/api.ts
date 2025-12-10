import { getApiBaseUrl, handleApiResponse } from './utils'
import { Wallet, Contact, TransactionEvent, BalanceAlert, CreateBalanceAlertRequest } from '../types'

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
      ...options,
      headers,
    })

    return handleApiResponse(response) as T
  }

  // Wallet API methods
  async createWallet(params: {
    name: string;
    descriptor: string;
    isFreshWallet?: boolean;
    scriptType?: string;
    stopGap?: string;
  }): Promise<Wallet> {
    // Send raw browser language - backend will map to supported languages
    const browserLanguage = typeof window !== 'undefined' 
      ? navigator.language
      : 'en'
    
    const response = await this.request<{ message: string; wallet: Wallet }>('/api/wallets', {
      method: 'POST',
      body: JSON.stringify({ 
        name: params.name,
        descriptor: params.descriptor,
        preferred_language: browserLanguage,
        is_fresh_wallet: params.isFreshWallet,
        script_type: params.scriptType,
        stop_gap: params.stopGap,
      }),
    })
    
    return response.wallet
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
    notificationMethods: Array<{ provider_type: 'sms' | 'ntfy' | 'email', notification_target: string }>
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
    phoneNumber?: string,
    emailAddress?: string
  ): Promise<{ message: string; auto_verified?: boolean }> {
    return this.request<{ message: string; auto_verified?: boolean }>(`/api/wallets/${walletChecksum}/contacts/send-verification`, {
      method: 'POST',
      body: JSON.stringify({
        name,
        language,
        phone_number: phoneNumber,
        email_address: emailAddress,
      }),
    })
  }

  async verifyContact(
    walletChecksum: string,
    code: string,
    phoneNumber?: string,
    emailAddress?: string
  ): Promise<{ valid: boolean; message: string }> {
    return this.request<{ valid: boolean; message: string }>(`/api/wallets/${walletChecksum}/contacts/verify`, {
      method: 'POST',
      body: JSON.stringify({
        phone_number: phoneNumber,
        email_address: emailAddress,
        code,
      }),
    })
  }

  async updateContact(
    walletChecksum: string,
    contactId: string,
    name: string,
    language: 'en' | 'no',
    notificationMethods: Array<{ provider_type: 'sms' | 'ntfy' | 'email', notification_target: string }>
  ): Promise<Contact> {
    return this.request<Contact>(`/api/wallets/${walletChecksum}/contacts/${contactId}`, {
      method: 'PUT',
      body: JSON.stringify({
        name,
        language,
        notification_methods: notificationMethods,
      }),
    })
  }

  async deleteContact(walletChecksum: string, contactId: string): Promise<void> {
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
  async register(email: string, password: string, name: string, marketingEmails: boolean = false): Promise<{ message: string }> {
    // Include browser locale for smart currency selection
    const browserLocale = typeof window !== 'undefined' ? navigator.language : 'en-US'
    
    return this.request<{ message: string }>('/api/auth/register', {
      method: 'POST',
      body: JSON.stringify({ 
        email, 
        password, 
        name, 
        marketing_emails_opt_in: marketingEmails,
        browser_locale: browserLocale 
      }),
    })
  }

  async login(email: string, password: string): Promise<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean } }> {
    return this.request<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean } }>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    })
  }

  async demoLogin(): Promise<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean } }> {
    return this.request<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean } }>('/api/auth/demo-login', {
      method: 'POST',
    })
  }

  async verifyEmail(token: string): Promise<{ message: string }> {
    return this.request<{ message: string }>(`/api/auth/verify-email/${token}`, {
      method: 'GET',
    })
  }

  async forgotPassword(email: string): Promise<{ message: string }> {
    return this.request<{ message: string }>('/api/auth/forgot-password', {
      method: 'POST',
      body: JSON.stringify({ email }),
    })
  }

  async resetPassword(token: string, password: string): Promise<{ message: string }> {
    return this.request<{ message: string }>(`/api/auth/reset-password/${token}`, {
      method: 'POST',
      body: JSON.stringify({ password }),
    })
  }

  async logout(): Promise<void> {
    return this.request<void>('/api/auth/logout', {
      method: 'POST',
    })
  }

  async getMe(): Promise<{ user: { id: number, email: string, name?: string, is_admin: boolean, is_demo: boolean, email_verified: boolean } }> {
    return this.request<{ user: { id: number, email: string, name?: string, is_admin: boolean, is_demo: boolean, email_verified: boolean } }>('/api/auth/me')
  }

  async updateUserProfile(name: string): Promise<{ user: { id: number, email: string, name?: string, is_admin: boolean, email_verified: boolean } }> {
    return this.request<{ user: { id: number, email: string, name?: string, is_admin: boolean, email_verified: boolean } }>('/api/auth/user', {
      method: 'PUT',
      body: JSON.stringify({ name }),
    })
  }

  // Stripe billing API methods
  async getBillingPricing(): Promise<{ tiers: Array<{ 
    tier: string, 
    name: string, 
    description?: string, 
    monthly_price?: { price_id: string, amount: number, currency: string, interval: string },
    yearly_price?: { price_id: string, amount: number, currency: string, interval: string },
    features: Record<string, string>
  }> }> {
    return this.request<{ tiers: Array<{ 
      tier: string, 
      name: string, 
      description?: string, 
      monthly_price?: { price_id: string, amount: number, currency: string, interval: string },
      yearly_price?: { price_id: string, amount: number, currency: string, interval: string },
      features: Record<string, string>
    }> }>('/api/billing/pricing')
  }

  async createCheckoutSession(tier: string, isYearly: boolean = false): Promise<{ url: string, session_id: string }> {
    return this.request<{ url: string, session_id: string }>('/api/stripe/checkout', {
      method: 'POST',
      body: JSON.stringify({ tier, is_yearly: isYearly }),
    })
  }

  async getCheckoutSessionDetails(sessionId: string): Promise<{
    session_id: string,
    status: string,
    tier?: string,
    billing_period?: string,
    amount_total?: number,
    currency?: string
  }> {
    return this.request<{
      session_id: string,
      status: string,
      tier?: string,
      billing_period?: string,
      amount_total?: number,
      currency?: string
    }>(`/api/billing/session/${sessionId}`)
  }

  async createCustomerPortalSession(returnUrl: string): Promise<{ url: string }> {
    return this.request<{ url: string }>('/api/stripe/portal', {
      method: 'POST',
      body: JSON.stringify({ return_url: returnUrl }),
    })
  }

  async getBillingStatus(): Promise<{
    user_id: string,
    subscription_tier: string,
    subscription_status: string,
    trial_ends_at?: string,
    subscription_started_at?: string,
    subscription_ends_at?: string,
    stripe_customer_id?: string,
    wallet_count: number,
    contact_count: number,
    limits: {
      max_wallets: number,
      max_contacts_per_wallet: number,
      sync_interval_seconds: number
    }
  }> {
    return this.request<{
      user_id: string,
      subscription_tier: string,
      subscription_status: string,
      trial_ends_at?: string,
      subscription_started_at?: string,
      subscription_ends_at?: string,
      stripe_customer_id?: string,
      wallet_count: number,
      contact_count: number,
      limits: {
        max_wallets: number,
        max_contacts_per_wallet: number,
        sync_interval_seconds: number
      }
    }>('/api/billing/status')
  }

  // User preferences API methods
  async getUserPreferences(): Promise<{ preferred_fiat_currency: string; ntfy_server_url: string | null }> {
    return this.request<{ preferred_fiat_currency: string; ntfy_server_url: string | null }>('/api/user/preferences')
  }

  async updateUserPreferences(preferences: { preferred_fiat_currency?: string; ntfy_server_url?: string }): Promise<{ preferred_fiat_currency: string; ntfy_server_url: string | null }> {
    return this.request<{ preferred_fiat_currency: string; ntfy_server_url: string | null }>('/api/user/preferences', {
      method: 'PUT',
      body: JSON.stringify(preferences),
    })
  }

  // Exchange rates API methods
  async getExchangeRates(): Promise<{ rates: Record<string, { currency: string, rate_per_btc: number, last_updated: string }> }> {
    return this.request<{ rates: Record<string, { currency: string, rate_per_btc: number, last_updated: string }> }>('/api/exchange-rates')
  }

  // Balance Alert API methods
  async getBalanceAlerts(walletChecksum: string): Promise<BalanceAlert[]> {
    const response = await this.request<{ alerts: BalanceAlert[] }>(`/api/wallets/${walletChecksum}/balance-alerts`)
    return response.alerts
  }

  async createBalanceAlert(walletChecksum: string, alertData: CreateBalanceAlertRequest): Promise<BalanceAlert> {
    return this.request<BalanceAlert>(`/api/wallets/${walletChecksum}/balance-alerts`, {
      method: 'POST',
      body: JSON.stringify(alertData),
    })
  }

  async deleteBalanceAlert(alertId: string): Promise<void> {
    return this.request<void>(`/api/balance-alerts/${alertId}`, {
      method: 'DELETE',
    })
  }

  // Contact form API method
  async submitContactForm(email: string, message: string): Promise<{ message: string }> {
    return this.request<{ message: string }>('/api/contact', {
      method: 'POST',
      body: JSON.stringify({ email, message }),
    })
  }
}

// Export a singleton instance
export const api = new ApiClient()