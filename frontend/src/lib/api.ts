import { getApiBaseUrl, handleApiResponse, createNetworkError } from './utils'

// Re-export ApiError for convenience
export { ApiError } from './utils'
import {
  Wallet,
  Contact,
  BalanceAlert,
  CreateBalanceAlertRequest,
  WalletDetailResponse,
  NotificationStatus,
} from '../types'

export interface ProviderInfo {
  name: string
  display_name: string
  config_schema: Record<string, unknown>
}

export interface TxExplorerConfig {
  id: string
  name: string
  base_url: string | null
  base_urls?: string[]
  port: number | null
  platform: string | null
}

export interface NtfyServerConfig {
  id: string
  name: string
  base_url: string
  platform: string | null
  default_topic: string | null
  managed_auth: boolean
}

export interface AppConfigResponse {
  tx_explorers: TxExplorerConfig[]
  default_tx_explorer_id: string
  ntfy_servers: NtfyServerConfig[]
  default_ntfy_server_id: string
}

export interface UserPreferencesResponse {
  preferred_fiat_currency: string
  preferred_tx_explorer_id: string | null
  ntfy_server_url: string | null
  ntfy_has_access_token: boolean
  ntfy_has_credentials: boolean
  ntfy_username: string | null
}

// Base API client
class ApiClient {
  private baseUrl: string

  constructor() {
    this.baseUrl = getApiBaseUrl()
  }

  // Kept for backwards compatibility during migration, but no longer stores tokens
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  setAuthToken(_token: string | null) {
    // No-op: tokens are now managed via HttpOnly cookies
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

    let response: Response
    try {
      response = await fetch(url, {
        ...options,
        headers,
        // Include credentials (cookies) with all requests for HttpOnly cookie auth
        credentials: 'include',
      })
    } catch (err) {
      // Network error (fetch failed entirely - no response)
      throw createNetworkError(err instanceof Error ? err : undefined)
    }

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
    const browserLanguage = typeof window !== 'undefined'
      ? navigator.language
      : 'en-US'

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

  async getWalletDetail(
    checksum: string,
    params?: {
      cursor?: string | null
      sinceTimestamp?: number | null
      pageSize?: number
    }
  ): Promise<WalletDetailResponse> {
    const searchParams = new URLSearchParams({
      page_size: (params?.pageSize ?? 100).toString(),
    })

    if (params?.cursor) {
      searchParams.set('cursor', params.cursor)
    }
    if (params?.sinceTimestamp !== null && params?.sinceTimestamp !== undefined) {
      searchParams.set('since_timestamp', params.sinceTimestamp.toString())
    }

    return this.request<WalletDetailResponse>(
      `/api/wallets/${checksum}/detail?${searchParams.toString()}`
    )
  }

  async getTransactionNotifications(
    walletChecksum: string,
    txid: string
  ): Promise<NotificationStatus[]> {
    return this.request<NotificationStatus[]>(
      `/api/wallets/${walletChecksum}/transactions/${txid}/notifications`
    )
  }

  // Contact API methods
  async getWalletContacts(walletChecksum: string): Promise<Contact[]> {
    return this.request<Contact[]>(`/api/wallets/${walletChecksum}/contacts`)
  }

  async createContact(
    walletChecksum: string,
    name: string,
    notificationMethods: Array<{ provider_type: 'sms' | 'ntfy' | 'email', notification_target: string }>
  ): Promise<Contact> {
    return this.request<Contact>(`/api/wallets/${walletChecksum}/contacts`, {
      method: 'POST',
      body: JSON.stringify({
        name,
        notification_methods: notificationMethods,
      }),
    })
  }

  async sendContactVerification(
    walletChecksum: string,
    name: string,
    phoneNumber?: string,
    emailAddress?: string
  ): Promise<{ message: string; auto_verified?: boolean }> {
    return this.request<{ message: string; auto_verified?: boolean }>(`/api/wallets/${walletChecksum}/contacts/send-verification`, {
      method: 'POST',
      body: JSON.stringify({
        name,
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
    notificationMethods: Array<{ provider_type: 'sms' | 'ntfy' | 'email', notification_target: string }>
  ): Promise<Contact> {
    return this.request<Contact>(`/api/wallets/${walletChecksum}/contacts/${contactId}`, {
      method: 'PUT',
      body: JSON.stringify({
        name,
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

  async login(email: string, password: string): Promise<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean; preferred_language?: string } }> {
    return this.request<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean; preferred_language?: string } }>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    })
  }

  async demoLogin(): Promise<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean; preferred_language?: string } }> {
    const browserLocale = typeof window !== 'undefined' ? navigator.language : 'en-US'

    return this.request<{ token: string; user: { id: number; email: string; name?: string; is_admin: boolean; is_demo: boolean; email_verified: boolean; preferred_language?: string } }>('/api/auth/demo-login', {
      method: 'POST',
      body: JSON.stringify({ browser_locale: browserLocale }),
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

  async getMe(): Promise<{ user: { id: number, email: string, name?: string, is_admin: boolean, is_demo: boolean, email_verified: boolean, preferred_language?: string } }> {
    return this.request<{ user: { id: number, email: string, name?: string, is_admin: boolean, is_demo: boolean, email_verified: boolean, preferred_language?: string } }>('/api/auth/me')
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

  async createCheckoutSession(tier: string, isYearly: boolean = false): Promise<{ url: string }> {
    return this.request<{ url: string }>('/api/stripe/checkout', {
      method: 'POST',
      body: JSON.stringify({ tier, is_yearly: isYearly }),
    })
  }

  async getCheckoutSessionDetails(sessionId: string): Promise<{
    status: string,
    tier?: string,
    billing_period?: string,
    amount_total?: number,
    currency?: string
  }> {
    return this.request<{
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
  async getUserPreferences(): Promise<UserPreferencesResponse> {
    return this.request<UserPreferencesResponse>('/api/user/preferences')
  }

  async updateUserPreferences(preferences: {
    preferred_fiat_currency?: string;
    preferred_language?: string;
    preferred_tx_explorer_id?: string | null;
    ntfy_server_url?: string;
    ntfy_access_token?: string;
    ntfy_username?: string;
    ntfy_password?: string;
  }): Promise<UserPreferencesResponse> {
    return this.request<UserPreferencesResponse>('/api/user/preferences', {
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

  // Config API methods
  async getConfig(): Promise<AppConfigResponse> {
    return this.request<AppConfigResponse>('/api/config')
  }

  async sendTestNtfyNotification(topic: string): Promise<{ success: boolean; error?: string }> {
    return this.request<{ success: boolean; error?: string }>('/api/ntfy/test', {
      method: 'POST',
      body: JSON.stringify({ topic }),
    })
  }
}

export const api = new ApiClient()
