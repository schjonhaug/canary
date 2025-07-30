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
  transaction_time: number
}

export interface Contact {
  id: number
  wallet_id: number
  name: string
  contact_address: string
  language: 'en' | 'no'
  created_at: string
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

export interface DashboardUpdate {
  timestamp: number
  wallets: Wallet[]
  events: TransactionEvent[]
}