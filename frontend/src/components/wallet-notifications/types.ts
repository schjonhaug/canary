import type {
  BalanceAlert,
  CreateBalanceAlertRequest,
  NotificationContentFields,
  NotificationMethod,
} from "@/types"

export type WizardStep = "delivery" | "alerts" | "privacy"
export type ContentPreset = "useful" | "private" | "detailed" | "custom"
export type NotificationProvider = NotificationMethod["provider_type"]

export type MethodDraft = {
  provider_type: NotificationProvider
  notification_target: string
  is_enabled: boolean
  content_fields: NotificationContentFields
}

export type ContactDraft = {
  name: string
  methods: MethodDraft[]
  notify_sending: boolean
  notify_sent: boolean
  notify_receiving: boolean
  notify_received: boolean
  notify_cpfp: boolean
  notify_rbf: boolean
  include_wallet_balance_in_tx_notifications: boolean
}

export type BalanceDraft = Omit<CreateBalanceAlertRequest, "contact_id"> & {
  id: string
  persisted: boolean
}

export type TransactionDraftKey =
  | "notify_sending"
  | "notify_sent"
  | "notify_receiving"
  | "notify_received"
  | "notify_cpfp"
  | "notify_rbf"

export type BalanceOperationFailure = {
  operation: "create" | "delete"
  alert: BalanceDraft | BalanceAlert
}
