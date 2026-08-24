// Shared type definitions for the Canary Wallet frontend application

export interface Wallet {
  checksum: string
  name: string
  descriptor: string
  wallet_filename: string
  hex_color: string
  created_at: string
  balance_total: number
  balance_fiat?: number
  fiat_currency?: string
  last_activity: string | null
  status: 'pending' | 'ready' | 'failed' | 'deleted'
  contact_count: number
  is_active: boolean
  wallet_type: 'descriptor' | 'address'
  last_synced_at?: string | null
}


export interface NotificationStatus {
  contact_name: string
  provider_name: string
  status: string
  error_message: string | null
  notification_target?: string  // Phone number, email, or ntfy topic
  provider_type?: string        // 'sms', 'email', 'ntfy', 'nostr'
  created_at: string
  notification_type: string     // 'pending' | 'confirmed'
}

export interface Transaction {
  txid: string // Bitcoin transaction ID (hash) - primary key
  wallet_checksum: string
  wallet_name: string
  transaction_type: 'send' | 'receive'
  amount_sats: number
  fee_sats: number | null // Transaction fee (for send transactions)
  block_height: number | null // NULL = mempool, >0 = confirmed at this height
  first_seen_at: number // Unix timestamp when we first detected this transaction
  confirmed_at: number | null // Unix timestamp when transaction was confirmed
  parent_txid: string | null
  // RBF replacement tracking
  transaction_status: string // 'pending' | 'confirmed' | 'replaced'
  replaced_by_txid: string | null // Transaction ID that replaced this one (if any)
  replaced_at: number | null // Unix timestamp when this transaction was replaced
  notification_status?: NotificationStatus[]
}

// Keep old TransactionEvent interface for backward compatibility during transition
export interface TransactionEvent {
  id: number
  wallet_checksum: string
  wallet_name: string
  event_type: 'send' | 'receive'
  amount_sats: number
  is_confirmed: boolean
  is_rbf: boolean
  parent_txid: string | null
  balance_total: number | null
  transaction_time: number
  notification_status?: NotificationStatus[]
}

export interface NotificationMethod {
  id: string
  contact_id: string
  provider_type: 'sms' | 'ntfy' | 'email' | 'nostr' | 'webhook'
  notification_target: string
  display_target?: string
  created_at: string
  is_enabled: boolean
  content_fields: NotificationContentFields
}

export interface NotificationContentFields {
  wallet_name: boolean
  event_type: boolean
  transaction_amount: boolean
  transaction_balance: boolean
  balance_alert_condition: boolean
  balance_alert_threshold: boolean
  balance_alert_balance: boolean
}

// Supported notification languages (must match backend Language enum)
export type NotificationLanguage = 'en-US' | 'nb' | 'es-419' | 'pt-BR' | 'de-DE' | 'fr-FR' | 'ja' | 'da' | 'sv'

export interface Contact {
  id: string
  wallet_checksum: string
  name: string
  notification_methods: NotificationMethod[]
  created_at: string
  is_active: boolean
  notify_sending: boolean
  notify_sent: boolean
  notify_receiving: boolean
  notify_received: boolean
  notify_cpfp: boolean
  notify_rbf: boolean
  include_wallet_balance_in_tx_notifications: boolean
}


export interface BlockHeader {
  height: number
  timestamp: number
  network: 'mainnet' | 'testnet' | 'regtest'
}

export interface BlockHeaderState {
  blockHeader: BlockHeader | null
  connected: boolean
  reconnecting: boolean
  error: string | null
}

export interface WalletsListResponse {
  timestamp: number
  wallets: Wallet[]
}

export interface WalletDetailResponse {
  timestamp: number
  wallet: Wallet
  transactions: Transaction[]
  contacts: Contact[]
  balance_alerts: BalanceAlert[]
  pagination: WalletDetailPagination
}

export interface WalletNotificationsResponse {
  timestamp: number
  wallet: Wallet
  contacts: Contact[]
  balance_alerts: BalanceAlert[]
}

export interface WalletDetailPagination {
  page_size: number
  next_cursor: string | null
  has_more: boolean
  applied_since_timestamp: number | null
}

// Balance Alert Types
export interface BalanceAlert {
  id: string
  wallet_checksum: string
  contact_id?: string
  threshold_sats: number
  alert_type: 'above' | 'below' | 'equals'
  is_active: boolean
  last_triggered_at?: number // Unix timestamp
  created_at: string
  // Fiat threshold support
  threshold_currency?: string
  threshold_fiat_amount?: number
}

export interface CreateBalanceAlertRequest {
  contact_id?: string
  threshold_sats?: number // Option 1: BTC threshold
  alert_type: 'above' | 'below' | 'equals'
  // Option 2: Fiat threshold
  threshold_currency?: string
  threshold_fiat_amount?: number
}
