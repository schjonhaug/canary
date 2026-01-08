import { Contact } from "@/types"

/**
 * Represents the original state of a contact being edited.
 * Used for change detection to enable/disable the submit button
 * and determine if verification is required.
 */
export interface OriginalContactState {
  name: string | null
  ntfyTopic: string | null
  phoneNumber: string | null
  emailAddress: string | null
  ntfyEnabled: boolean
  smsEnabled: boolean
  emailEnabled: boolean
}

/**
 * Result of change detection for contact form.
 * Indicates what has changed and what verification is required.
 */
export interface ChangeDetectionResult {
  /** Whether the phone number has changed from original */
  phoneNumberChanged: boolean
  /** Whether the email address has changed from original */
  emailAddressChanged: boolean
  /** Whether SMS verification is required before submit */
  smsVerificationRequired: boolean
  /** Whether email verification is required before submit */
  emailVerificationRequired: boolean
  /** Whether any changes have been made (for submit button state) */
  hasChanges: boolean
}

/**
 * Data extracted from a Contact for form initialization.
 */
export interface ExtractedProviderData {
  enabledProviders: Record<string, boolean>
  providerValues: Record<string, string>
  ntfyTopic: string
  originalState: OriginalContactState
}

/**
 * Creates an empty original state for new contacts.
 */
export function createEmptyOriginalState(): OriginalContactState {
  return {
    name: null,
    ntfyTopic: null,
    phoneNumber: null,
    emailAddress: null,
    ntfyEnabled: false,
    smsEnabled: false,
    emailEnabled: false,
  }
}

/**
 * Helper function to extract provider data from a Contact object.
 * Used when initializing the form in edit mode.
 */
export function extractProviderDataFromContact(
  contact: Contact
): ExtractedProviderData {
  const enabledProviders: Record<string, boolean> = {}
  const providerValues: Record<string, string> = {}
  let ntfyTopic = ""

  const originalState: OriginalContactState = {
    name: contact.name,
    ntfyTopic: null,
    phoneNumber: null,
    emailAddress: null,
    ntfyEnabled: false,
    smsEnabled: false,
    emailEnabled: false,
  }

  contact.notification_methods.forEach(method => {
    switch (method.provider_type) {
      case 'sms': {
        const phoneNumber = method.display_target || method.notification_target
        enabledProviders['twilio'] = true
        providerValues['twilio'] = phoneNumber
        originalState.phoneNumber = phoneNumber
        originalState.smsEnabled = true
        break
      }
      case 'ntfy': {
        enabledProviders['ntfy'] = true
        ntfyTopic = method.notification_target
        originalState.ntfyTopic = ntfyTopic
        originalState.ntfyEnabled = true
        break
      }
      case 'email': {
        const emailAddress = method.display_target || method.notification_target
        enabledProviders['email'] = true
        providerValues['email'] = emailAddress
        originalState.emailAddress = emailAddress
        originalState.emailEnabled = true
        break
      }
    }
  })

  return {
    enabledProviders,
    providerValues,
    ntfyTopic,
    originalState,
  }
}
