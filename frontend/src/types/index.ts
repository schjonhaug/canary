// Shared type definitions for the Canary frontend application

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
  status: string // 'pending' | 'ready' | 'deleted'
  contact_count: number
  is_active: boolean
}


export interface NotificationStatus {
  contact_name: string
  provider_name: string
  status: string
  error_message: string | null
  notification_target?: string  // Phone number, email, or ntfy topic
  provider_type?: string        // 'sms', 'email', 'ntfy'
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
  notification_status: NotificationStatus[]
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
  notification_status: NotificationStatus[]
}

export interface NotificationMethod {
  id: string
  contact_id: string
  provider_type: 'sms' | 'ntfy' | 'email'
  notification_target: string
  display_target?: string
  created_at: string
}

export interface Contact {
  id: string
  wallet_checksum: string
  name: string
  language: 'en' | 'no'
  notification_methods: NotificationMethod[]
  created_at: string
  is_active: boolean
}


export interface BlockHeader {
  height: number
  timestamp: number
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
}

// Balance Alert Types
export interface BalanceAlert {
  id: string
  wallet_checksum: string
  threshold_sats: number
  alert_type: 'above' | 'below' | 'equals'
  is_active: boolean
  last_triggered_at?: number // Unix timestamp
  created_at: string
}

export interface CreateBalanceAlertRequest {
  threshold_sats: number
  alert_type: 'above' | 'below' | 'equals'
}