import { useState, useCallback } from "react"
import { api, ApiError } from "@/lib/api"
import { useVerificationTimer } from "./useVerificationTimer"
import { useTranslations } from "next-intl"

interface UseSmsVerificationProps {
  walletChecksum: string
  contactName: string
  originalPhoneNumber: string | null
  onError?: (error: string) => void
}

interface UseSmsVerificationReturn {
  // State
  verificationSent: boolean
  verificationCode: string
  verificationPhone: string | null
  isVerified: boolean
  showSuccess: boolean
  isSending: boolean
  isVerifying: boolean
  verificationError: string | null
  phoneError: string | null

  // Timer
  timeRemaining: number
  formatTime: (seconds: number) => string

  // Actions
  setVerificationCode: (code: string) => void
  clearPhoneError: () => void
  clearVerificationError: () => void
  sendVerification: (phoneNumber: string) => Promise<void>
  verifyCode: () => Promise<void>
  resendCode: () => Promise<void>
  reset: () => void
  resetForPhoneChange: (newPhoneNumber: string) => void
  revertToOriginal: () => void
  setVerified: (verified: boolean) => void
}

export function useSmsVerification({
  walletChecksum,
  contactName,
  originalPhoneNumber,
  onError
}: UseSmsVerificationProps): UseSmsVerificationReturn {
  const t = useTranslations('contacts')

  const [verificationSent, setVerificationSent] = useState(false)
  const [verificationCode, setVerificationCode] = useState("")
  const [verificationPhone, setVerificationPhone] = useState<string | null>(null)
  const [isVerified, setIsVerified] = useState(false)
  const [showSuccess, setShowSuccess] = useState(false)
  const [isSending, setIsSending] = useState(false)
  const [isVerifying, setIsVerifying] = useState(false)
  const [verificationError, setVerificationError] = useState<string | null>(null)
  const [phoneError, setPhoneError] = useState<string | null>(null)

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
    setVerificationPhone(null)
    setIsVerified(false)
    setShowSuccess(false)
    setVerificationError(null)
    setPhoneError(null)
    clearTimer()
  }, [clearTimer])

  const resetForPhoneChange = useCallback((newPhoneNumber: string) => {
    // Check if phone changed from original
    if (originalPhoneNumber !== null && newPhoneNumber !== originalPhoneNumber) {
      setVerificationSent(false)
      setVerificationCode("")
      setVerificationPhone(null)
      setIsVerified(false)
      clearTimer()
    }
  }, [originalPhoneNumber, clearTimer])

  const revertToOriginal = useCallback(() => {
    // Phone number reverted to original, mark as verified
    setIsVerified(true)
    setVerificationSent(false)
    setVerificationCode("")
    setVerificationPhone(null)
    clearTimer()
  }, [clearTimer])

  const sendVerification = useCallback(async (phoneNumber: string) => {
    if (!phoneNumber.trim()) {
      onError?.(t('verification.smsRequired'))
      return
    }

    setIsSending(true)
    setVerificationError(null)
    setPhoneError(null)

    try {
      await api.sendContactVerification(
        walletChecksum,
        contactName || `Contact-${phoneNumber.slice(-4)}`,
        phoneNumber,
        undefined
      )

      setVerificationPhone(phoneNumber)
      setVerificationSent(true)
      setVerificationCode("")
      startTimer()
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : "Failed to send verification code"
      }

      if (errorMessage.toLowerCase().includes("phone") || errorMessage.toLowerCase().includes("number")) {
        setPhoneError(errorMessage)
      } else {
        onError?.(errorMessage)
      }
    } finally {
      setIsSending(false)
    }
  }, [walletChecksum, contactName, startTimer, onError, t])

  const verifyCode = useCallback(async () => {
    if (!verificationCode.trim() || !verificationPhone) {
      onError?.(t('verification.enterCode'))
      return
    }

    setIsVerifying(true)
    setVerificationError(null)

    try {
      const result = await api.verifyContact(
        walletChecksum,
        verificationCode.trim(),
        verificationPhone,
        undefined
      )

      if (result.valid) {
        setIsVerified(true)
        setShowSuccess(true)
        setVerificationError(null)
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
  }, [verificationCode, verificationPhone, walletChecksum, clearTimer, onError, t])

  const resendCode = useCallback(async () => {
    if (!verificationPhone) return

    setIsSending(true)
    setVerificationError(null)

    try {
      await api.sendContactVerification(
        walletChecksum,
        contactName,
        verificationPhone
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
  }, [verificationPhone, walletChecksum, contactName, startTimer, onError])

  return {
    verificationSent,
    verificationCode,
    verificationPhone,
    isVerified,
    showSuccess,
    isSending,
    isVerifying,
    verificationError,
    phoneError,
    timeRemaining,
    formatTime,
    setVerificationCode,
    clearPhoneError: () => setPhoneError(null),
    clearVerificationError: () => setVerificationError(null),
    sendVerification,
    verifyCode,
    resendCode,
    reset,
    resetForPhoneChange,
    revertToOriginal,
    setVerified: setIsVerified
  }
}
