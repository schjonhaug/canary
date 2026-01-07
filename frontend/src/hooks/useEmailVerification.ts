import { useState, useCallback } from "react"
import { api, ApiError } from "@/lib/api"
import { useVerificationTimer } from "./useVerificationTimer"
import { useTranslations } from "next-intl"

interface UseEmailVerificationProps {
  walletChecksum: string
  contactName: string
  originalEmailAddress: string | null
  onError?: (error: string) => void
}

interface UseEmailVerificationReturn {
  // State
  verificationSent: boolean
  verificationCode: string
  verificationAddress: string | null
  isVerified: boolean
  showSuccess: boolean
  isSending: boolean
  isVerifying: boolean
  verificationError: string | null
  emailError: string | null

  // Timer
  timeRemaining: number
  formatTime: (seconds: number) => string

  // Actions
  setVerificationCode: (code: string) => void
  clearEmailError: () => void
  clearVerificationError: () => void
  sendVerification: (emailAddress: string) => Promise<void>
  verifyCode: () => Promise<void>
  resendCode: () => Promise<void>
  reset: () => void
  resetForEmailChange: (newEmailAddress: string) => void
  revertToOriginal: () => void
  setVerified: (verified: boolean) => void
}

export function useEmailVerification({
  walletChecksum,
  contactName,
  originalEmailAddress,
  onError
}: UseEmailVerificationProps): UseEmailVerificationReturn {
  const t = useTranslations('contacts')

  const [verificationSent, setVerificationSent] = useState(false)
  const [verificationCode, setVerificationCode] = useState("")
  const [verificationAddress, setVerificationAddress] = useState<string | null>(null)
  const [isVerified, setIsVerified] = useState(false)
  const [showSuccess, setShowSuccess] = useState(false)
  const [isSending, setIsSending] = useState(false)
  const [isVerifying, setIsVerifying] = useState(false)
  const [verificationError, setVerificationError] = useState<string | null>(null)
  const [emailError, setEmailError] = useState<string | null>(null)

  const { timeRemaining, startTimer, clearTimer, formatTime } = useVerificationTimer(
    600,
    () => {
      setVerificationSent(false)
      onError?.(t('verification.expired'))
    }
  )

  const reset = useCallback(() => {
    setVerificationSent(false)
    setVerificationCode("")
    setVerificationAddress(null)
    setIsVerified(false)
    setShowSuccess(false)
    setVerificationError(null)
    setEmailError(null)
    clearTimer()
  }, [clearTimer])

  const resetForEmailChange = useCallback((newEmailAddress: string) => {
    // Check if email changed from original
    if (originalEmailAddress !== null && newEmailAddress !== originalEmailAddress) {
      setVerificationSent(false)
      setVerificationCode("")
      setVerificationAddress(null)
      setIsVerified(false)
      clearTimer()
    }
  }, [originalEmailAddress, clearTimer])

  const revertToOriginal = useCallback(() => {
    // Email address reverted to original, mark as verified
    setIsVerified(true)
    setVerificationSent(false)
    setVerificationCode("")
    setVerificationAddress(null)
    clearTimer()
  }, [clearTimer])

  const sendVerification = useCallback(async (emailAddress: string) => {
    if (!emailAddress.trim()) {
      setEmailError(t('verification.emailRequired'))
      return
    }

    setIsSending(true)
    setVerificationError(null)
    setEmailError(null)

    try {
      const result = await api.sendContactVerification(
        walletChecksum,
        contactName || `Contact-${emailAddress.split('@')[0]}`,
        undefined,
        emailAddress
      )

      // Check if email was auto-verified (user's own email)
      if (result.auto_verified) {
        setIsVerified(true)
        setShowSuccess(true)
      } else {
        setVerificationAddress(emailAddress)
        setVerificationSent(true)
        setVerificationCode("")
        startTimer()
      }
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : "Failed to send verification code"
      }

      if (errorMessage.toLowerCase().includes("email") || errorMessage.toLowerCase().includes("address")) {
        setEmailError(errorMessage)
      } else {
        onError?.(errorMessage)
      }
    } finally {
      setIsSending(false)
    }
  }, [walletChecksum, contactName, startTimer, onError, t])

  const verifyCode = useCallback(async () => {
    if (!verificationCode.trim() || !verificationAddress) {
      setVerificationError(t('verification.enterCode'))
      return
    }

    setIsVerifying(true)
    setVerificationError(null)

    try {
      const result = await api.verifyContact(
        walletChecksum,
        verificationCode.trim(),
        undefined,
        verificationAddress
      )

      if (result.valid) {
        setIsVerified(true)
        setShowSuccess(true)
        setVerificationError(null)
        setEmailError(null)
        clearTimer()
      } else {
        setVerificationError(result.message || "Invalid verification code")
        setVerificationCode("")
      }
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : "Invalid verification code"
      }

      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setVerificationError(t('verification.expiredRequest'))
        setVerificationSent(false)
        setIsVerified(false)
        clearTimer()
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setVerificationError(t('verification.invalid'))
        setVerificationCode("")
      } else {
        setVerificationError(errorMessage)
      }
    } finally {
      setIsVerifying(false)
    }
  }, [verificationCode, verificationAddress, walletChecksum, clearTimer, t])

  const resendCode = useCallback(async () => {
    if (!verificationAddress) return

    setIsSending(true)
    setVerificationError(null)

    try {
      await api.sendContactVerification(
        walletChecksum,
        contactName,
        undefined,
        verificationAddress
      )

      setVerificationCode("")
      startTimer()
    } catch (err) {
      if (err instanceof ApiError) {
        onError?.(err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message)
      } else {
        onError?.(err instanceof Error ? err.message : "Failed to resend code")
      }
    } finally {
      setIsSending(false)
    }
  }, [verificationAddress, walletChecksum, contactName, startTimer, onError])

  return {
    verificationSent,
    verificationCode,
    verificationAddress,
    isVerified,
    showSuccess,
    isSending,
    isVerifying,
    verificationError,
    emailError,
    timeRemaining,
    formatTime,
    setVerificationCode,
    clearEmailError: () => setEmailError(null),
    clearVerificationError: () => { setVerificationError(null); setEmailError(null) },
    sendVerification,
    verifyCode,
    resendCode,
    reset,
    resetForEmailChange,
    revertToOriginal,
    setVerified: setIsVerified
  }
}
