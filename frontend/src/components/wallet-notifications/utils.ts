import { parsePhoneNumberFromString } from "libphonenumber-js"

import type {
  BalanceAlert,
  Contact,
  NotificationContentFields,
} from "@/types"
import {
  DEFAULT_NOTIFICATION_CONTENT_FIELDS,
} from "@/components/notification-content-fields-control"
import type {
  BalanceDraft,
  ContactDraft,
  ContentPreset,
  MethodDraft,
} from "./types"

export const PRIVATE_CONTENT_FIELDS: NotificationContentFields = {
  wallet_name: false,
  event_type: false,
  transaction_amount: false,
  transaction_balance: false,
  balance_alert_condition: false,
  balance_alert_threshold: false,
  balance_alert_balance: false,
}

export const DETAILED_CONTENT_FIELDS: NotificationContentFields = {
  wallet_name: true,
  event_type: true,
  transaction_amount: true,
  transaction_balance: true,
  balance_alert_condition: true,
  balance_alert_threshold: true,
  balance_alert_balance: true,
}

export const DEFAULT_NEW_CONTACT_SETTINGS: Omit<ContactDraft, "name" | "methods"> = {
  notify_sending: true,
  notify_sent: true,
  notify_receiving: true,
  notify_received: true,
  notify_cpfp: false,
  notify_rbf: false,
  include_wallet_balance_in_tx_notifications: false,
}

export const CONTENT_FIELD_KEYS = Object.keys(
  DEFAULT_NOTIFICATION_CONTENT_FIELDS
) as Array<keyof NotificationContentFields>

export function fieldsForPreset(
  preset: Exclude<ContentPreset, "custom">
): NotificationContentFields {
  if (preset === "private") return { ...PRIVATE_CONTENT_FIELDS }
  if (preset === "detailed") return { ...DETAILED_CONTENT_FIELDS }
  return { ...DEFAULT_NOTIFICATION_CONTENT_FIELDS }
}

export function getContentPreset(fields: NotificationContentFields): ContentPreset {
  if (CONTENT_FIELD_KEYS.every((key) => fields[key] === DEFAULT_NOTIFICATION_CONTENT_FIELDS[key])) {
    return "useful"
  }
  if (CONTENT_FIELD_KEYS.every((key) => fields[key] === PRIVATE_CONTENT_FIELDS[key])) {
    return "private"
  }
  if (CONTENT_FIELD_KEYS.every((key) => fields[key] === DETAILED_CONTENT_FIELDS[key])) {
    return "detailed"
  }
  return "custom"
}

export function legacyContentFields(
  level: "minimal" | "standard" | "detailed" | undefined,
  includeTransactionBalance: boolean
): NotificationContentFields {
  if (level === "minimal") return { ...PRIVATE_CONTENT_FIELDS }
  if (level === "detailed") {
    return {
      ...DETAILED_CONTENT_FIELDS,
      transaction_balance: includeTransactionBalance,
    }
  }
  return { ...DEFAULT_NOTIFICATION_CONTENT_FIELDS }
}

export function contactToDraft(contact: Contact): ContactDraft {
  return {
    name: contact.name,
    methods: contact.notification_methods.map((method) => ({
      provider_type: method.provider_type,
      notification_target:
        method.provider_type === "nostr"
          ? method.display_target ?? method.notification_target
          : method.notification_target,
      is_enabled: method.is_enabled ?? true,
      content_fields:
        method.content_fields ??
        legacyContentFields(
          (method as typeof method & {
            content_privacy_level?: "minimal" | "standard" | "detailed"
          }).content_privacy_level,
          contact.include_wallet_balance_in_tx_notifications ?? false
        ),
    })),
    notify_sending: contact.notify_sending ?? true,
    notify_sent: contact.notify_sent ?? true,
    notify_receiving: contact.notify_receiving ?? true,
    notify_received: contact.notify_received ?? true,
    // Older contact payloads omit these fields. Preserve the pre-redesign fallback
    // while keeping both disabled in DEFAULT_NEW_CONTACT_SETTINGS for new contacts.
    notify_cpfp: contact.notify_cpfp ?? true,
    notify_rbf: contact.notify_rbf ?? true,
    include_wallet_balance_in_tx_notifications:
      contact.include_wallet_balance_in_tx_notifications ?? false,
  }
}

export function alertsToDrafts(alerts: BalanceAlert[]): BalanceDraft[] {
  return alerts.map((alert) => ({
    id: alert.id,
    persisted: true,
    alert_type: alert.alert_type,
    threshold_sats: alert.threshold_sats,
    threshold_currency: alert.threshold_currency,
    threshold_fiat_amount: alert.threshold_fiat_amount,
  }))
}

export function txSettingsFromDraft(draft: ContactDraft) {
  return {
    notify_sending: draft.notify_sending,
    notify_sent: draft.notify_sent,
    notify_receiving: draft.notify_receiving,
    notify_received: draft.notify_received,
    notify_cpfp: draft.notify_cpfp,
    notify_rbf: draft.notify_rbf,
    include_wallet_balance_in_tx_notifications:
      draft.include_wallet_balance_in_tx_notifications,
  }
}

export function generatePrivateNtfyTopic(): string {
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  return `canary-${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`
}

export function redactDeliveryTarget(method: MethodDraft): string {
  const target = method.notification_target.trim()
  if (!target) return ""

  if (method.provider_type === "email") {
    const [local, domain] = target.split("@")
    if (!domain) return "••••"
    return `${local?.slice(0, 1) || "•"}•••@${domain}`
  }
  if (method.provider_type === "sms") {
    const formatted = parsePhoneNumberFromString(target)?.formatInternational() ?? target
    return `•••• ${formatted.replace(/\s/g, "").slice(-4)}`
  }
  if (method.provider_type === "webhook") {
    try {
      return new URL(target).origin
    } catch {
      return "••••"
    }
  }
  if (target.length <= 12) return `${target.slice(0, 2)}••••${target.slice(-2)}`
  return `${target.slice(0, 7)}…${target.slice(-5)}`
}

export type AlertSummary = "recommended" | "advanced" | "none" | "custom"

export function getAlertSummary(contact: Pick<
  ContactDraft,
  | "notify_sending"
  | "notify_sent"
  | "notify_receiving"
  | "notify_received"
  | "notify_cpfp"
  | "notify_rbf"
>): AlertSummary {
  if (contact.notify_rbf || contact.notify_cpfp) return "advanced"
  const directional = [
    contact.notify_sending,
    contact.notify_receiving,
    contact.notify_sent,
    contact.notify_received,
  ]
  if (directional.every(Boolean)) return "recommended"
  if (directional.every((value) => !value)) return "none"
  return "custom"
}

export function isDraftDirty(
  draft: ContactDraft,
  initial: ContactDraft,
  balanceDrafts: BalanceDraft[],
  initialBalanceDrafts: BalanceDraft[]
): boolean {
  return JSON.stringify(draft) !== JSON.stringify(initial) ||
    JSON.stringify(balanceDrafts) !== JSON.stringify(initialBalanceDrafts)
}
