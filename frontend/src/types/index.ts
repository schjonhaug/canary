// Shared type definitions for the Canary frontend application

export interface Wallet {
  id: number
  name: string
  descriptor: string
  wallet_filename: string
  hex_color: string
  created_at: string
  balance_total: number
  last_activity: string | null
  contact_count: number
}

export interface SmsRecipientStatus {
  name: string
  status: string // "sent", "failed", "pending"
  error_message?: string
}

export interface TransactionEvent {
  id: number
  wallet_id: number
  wallet_name: string
  event_type: 'send' | 'receive'
  amount_sats: number
  is_confirmed: boolean
  is_rbf: boolean
  is_cpfp: boolean
  balance_total: number | null
  sms_recipients: string[]
  sms_recipients_status: SmsRecipientStatus[]
  transaction_time: number
}

export interface Contact {
  id: number
  wallet_id: number
  name: string
  phone_number: string
  language: 'en' | 'no'
  created_at: string
}

export interface TwilioConfig {
  account_sid: string
  auth_token: string
  messaging_service_sid: string
}

export interface BlockHeader {
  height: number
  hash: string
  timestamp: number
}

export interface BlockHeaderState {
  blockHeader: BlockHeader | null
  connected: boolean
  reconnecting: boolean
  error: string | null
}

export interface DashboardUpdate {
  timestamp: number
  wallets: Wallet[]
  events: TransactionEvent[]
}

export interface CachedData {
  wallets: Wallet[]
  events: TransactionEvent[]
  lastUpdate: number
  timestamp: number
}