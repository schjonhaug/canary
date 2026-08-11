import { useMemo } from "react"
import { OriginalContactState, ChangeDetectionResult } from "@/components/contact-modal/types"

interface UseContactChangeDetectionParams {
  /** Whether the modal is currently open */
  isOpen: boolean
  /** Whether we are editing an existing contact */
  isEditMode: boolean
  /** The original state of the contact being edited */
  originalState: OriginalContactState
  /** Current form values */
  currentName: string
  currentNtfyTopic: string
  enabledProviders: Record<string, boolean>
  providerValues: Record<string, string>
  /** Verification status */
  smsVerified: boolean
  emailVerified: boolean
}

/**
 * Hook to compute change detection for contact form.
 *
 * This centralizes the logic for determining:
 * - Whether fields have changed from their original values
 * - Whether verification is required for SMS/email
 * - Whether the form has any changes (for submit button state)
 *
 * All computations are memoized and only recalculated when inputs change.
 */
export function useContactChangeDetection({
  isOpen,
  isEditMode,
  originalState,
  currentName,
  currentNtfyTopic,
  enabledProviders,
  providerValues,
  smsVerified,
  emailVerified,
}: UseContactChangeDetectionParams): ChangeDetectionResult {

  return useMemo(() => {
    // When modal is closed, return all false to avoid unnecessary computation
    if (!isOpen) {
      return {
        phoneNumberChanged: false,
        emailAddressChanged: false,
        smsVerificationRequired: false,
        emailVerificationRequired: false,
        hasChanges: false,
      }
    }

    // Check if phone number has changed from original
    const phoneNumberChanged = originalState.phoneNumber !== null &&
      providerValues['twilio']?.trim() !== originalState.phoneNumber

    // Check if email address has changed from original
    const emailAddressChanged = originalState.emailAddress !== null &&
      providerValues['email']?.trim() !== originalState.emailAddress

    // SMS verification is required when:
    // 1. Twilio is enabled AND
    // 2. Either: phone changed, OR new contact without verification, OR adding SMS to existing contact
    const smsVerificationRequired = enabledProviders['twilio'] && (
      phoneNumberChanged ||
      (!isEditMode && !smsVerified) ||
      (isEditMode && originalState.phoneNumber === null && !smsVerified)
    )

    // Email verification is required when:
    // 1. Email is enabled AND
    // 2. Either: email changed, OR new contact without verification, OR adding email to existing contact
    const emailVerificationRequired = enabledProviders['email'] && (
      emailAddressChanged ||
      (!isEditMode && !emailVerified) ||
      (isEditMode && originalState.emailAddress === null && !emailVerified)
    )

    // Compute hasChanges for edit mode submit button
    // In edit mode, if originalState.name is null, initialization hasn't run yet
    const hasChanges = isEditMode ? (
      originalState.name === null ? false : (
        currentName.trim() !== originalState.name ||
        currentNtfyTopic !== (originalState.ntfyTopic || '') ||
        (enabledProviders['ntfy'] || false) !== originalState.ntfyEnabled ||
        (enabledProviders['twilio'] || false) !== originalState.smsEnabled ||
        (enabledProviders['email'] || false) !== originalState.emailEnabled ||
        (enabledProviders['nostr'] || false) !== originalState.nostrEnabled ||
        (enabledProviders['webhook'] || false) !== originalState.webhookEnabled ||
        (providerValues['twilio']?.trim() || '') !== (originalState.phoneNumber || '') ||
        (providerValues['email']?.trim() || '') !== (originalState.emailAddress || '') ||
        (providerValues['nostr']?.trim() || '') !== (originalState.nostrRecipient || '') ||
        (providerValues['webhook']?.trim() || '') !== (originalState.webhookUrl || '')
      )
    ) : true // Always allow submit for new contacts

    return {
      phoneNumberChanged,
      emailAddressChanged,
      smsVerificationRequired,
      emailVerificationRequired,
      hasChanges,
    }
  }, [
    isOpen,
    isEditMode,
    originalState,
    currentName,
    currentNtfyTopic,
    enabledProviders,
    providerValues,
    smsVerified,
    emailVerified,
  ])
}
