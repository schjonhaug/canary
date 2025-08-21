// Shared type definitions for the Canary frontend application

export interface Wallet {
  checksum: string
  name: string
  descriptor: string
  wallet_filename: string
  hex_color: string
  created_at: string
  balance_total: number
  last_activity: string | null
  sync_status: string // 'pending' | 'ready'
  contact_count: number
  is_active: boolean
}


export interface NotificationStatus {
  contact_name: string
  provider_name: string
  status: string
  error_message: string | null
}

export interface TransactionEvent {
  id: number
  wallet_checksum: string
  wallet_name: string
  event_type: 'send' | 'receive'
  amount_sats: number
  is_confirmed: boolean
  is_rbf: boolean
  is_cpfp: boolean
  balance_total: number | null
  transaction_time: number
  notification_status: NotificationStatus[]
}

export interface NotificationMethod {
  id: number
  contact_id: number
  provider_type: 'sms' | 'ntfy' | 'email'
  notification_target: string
  display_target?: string
  created_at: string
}

export interface Contact {
  id: number
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
  events: TransactionEvent[]
  contacts: Contact[]
}