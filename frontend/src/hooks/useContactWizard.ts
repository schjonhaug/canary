import { useState, useCallback, useEffect, useMemo } from "react"

interface UseContactWizardProps {
  name: string
  enabledProviders: Record<string, boolean>
  providerValues: Record<string, string>
  ntfyTopic: string
  smsVerificationRequired: boolean
  emailVerificationRequired: boolean
  smsVerified: boolean
  emailVerified: boolean
}

interface UseContactWizardReturn {
  currentStep: number
  totalSteps: number
  canGoNext: boolean
  canGoBack: boolean
  goNext: () => void
  goBack: () => void
  reset: () => void
  isLastStep: boolean
  needsVerificationStep: boolean
  allVerificationsComplete: boolean
}

export function useContactWizard({
  name,
  enabledProviders,
  providerValues,
  ntfyTopic,
  smsVerificationRequired,
  emailVerificationRequired,
  smsVerified,
  emailVerified,
}: UseContactWizardProps): UseContactWizardReturn {
  const [currentStep, setCurrentStep] = useState(0)
  const [hasEnteredVerificationStep, setHasEnteredVerificationStep] = useState(false)

  const needsVerificationStep = smsVerificationRequired || emailVerificationRequired

  // Keep 3 steps if user is on or has visited verification step (prevents
  // jumping back when verification completes and requirements clear)
  const totalSteps = needsVerificationStep || hasEnteredVerificationStep ? 3 : 2

  // Track when user enters step 2
  useEffect(() => {
    if (currentStep === 2) {
      setHasEnteredVerificationStep(true)
    }
  }, [currentStep])

  // Clamp currentStep when totalSteps decreases and user hasn't entered verification
  useEffect(() => {
    if (!hasEnteredVerificationStep) {
      setCurrentStep(prev => Math.min(prev, totalSteps - 1))
    }
  }, [hasEnteredVerificationStep, totalSteps])

  const hasAtLeastOneProvider = useMemo(() => {
    return Object.entries(enabledProviders).some(([key, enabled]) => {
      if (!enabled) return false
      if (key === 'ntfy') return ntfyTopic.trim().length > 0
      return (providerValues[key]?.trim().length ?? 0) > 0
    })
  }, [enabledProviders, providerValues, ntfyTopic])

  const canGoNext = useMemo(() => {
    if (currentStep === 0) {
      return name.trim().length > 0
    }
    if (currentStep === 1) {
      return hasAtLeastOneProvider
    }
    return false
  }, [currentStep, name, hasAtLeastOneProvider])

  const canGoBack = currentStep > 0

  const allVerificationsComplete = useMemo(() => {
    const smsOk = !smsVerificationRequired || smsVerified
    const emailOk = !emailVerificationRequired || emailVerified
    return smsOk && emailOk
  }, [smsVerificationRequired, smsVerified, emailVerificationRequired, emailVerified])

  const isLastStep = useMemo(() => {
    if (currentStep === 1 && !needsVerificationStep && !hasEnteredVerificationStep) return true
    if (currentStep === 2) return true
    return false
  }, [currentStep, hasEnteredVerificationStep, needsVerificationStep])

  const goNext = useCallback(() => {
    if (canGoNext && currentStep < totalSteps - 1) {
      setCurrentStep(prev => prev + 1)
    }
  }, [canGoNext, currentStep, totalSteps])

  const goBack = useCallback(() => {
    if (canGoBack) {
      setCurrentStep(prev => prev - 1)
    }
  }, [canGoBack])

  const reset = useCallback(() => {
    setCurrentStep(0)
    setHasEnteredVerificationStep(false)
  }, [])

  return {
    currentStep,
    totalSteps,
    canGoNext,
    canGoBack,
    goNext,
    goBack,
    reset,
    isLastStep,
    needsVerificationStep,
    allVerificationsComplete,
  }
}
