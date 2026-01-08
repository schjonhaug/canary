import { useState, useCallback } from "react"
import { Contact } from "@/types"
import {
  OriginalContactState,
  ExtractedProviderData,
  extractProviderDataFromContact,
  createEmptyOriginalState
} from "@/components/contact-modal/types"

interface UseOriginalContactStateReturn {
  /** The current original state */
  originalState: OriginalContactState
  /** Initialize state from an existing contact (edit mode) */
  initializeFromContact: (contact: Contact) => Omit<ExtractedProviderData, 'originalState'>
  /** Reset to empty state (new contact mode) */
  reset: () => void
}

/**
 * Hook to manage the original state of a contact being edited.
 * This state is used for change detection to determine if the
 * submit button should be enabled and if verification is required.
 *
 * Consolidates 7 separate useState calls into a single state object.
 */
export function useOriginalContactState(): UseOriginalContactStateReturn {
  const [originalState, setOriginalState] = useState<OriginalContactState>(
    createEmptyOriginalState()
  )

  const initializeFromContact = useCallback((contact: Contact) => {
    const extracted = extractProviderDataFromContact(contact)
    setOriginalState(extracted.originalState)
    return {
      enabledProviders: extracted.enabledProviders,
      providerValues: extracted.providerValues,
      ntfyTopic: extracted.ntfyTopic,
    }
  }, [])

  const reset = useCallback(() => {
    setOriginalState(createEmptyOriginalState())
  }, [])

  return {
    originalState,
    initializeFromContact,
    reset,
  }
}
