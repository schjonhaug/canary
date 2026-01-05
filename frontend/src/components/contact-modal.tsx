"use client"

import { useState, useCallback, useEffect, useRef } from "react"
import { 
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Bell, MessageCircle, Mail } from "lucide-react"
import { api, ProviderInfo, ApiError } from "../lib/api"
import { Contact } from "../types"
import { DeleteContactModal } from "./delete-contact-modal"
import { useTranslations } from "next-intl"

// Notification languages supported by backend (for contact notifications)
// Must match backend Language enum: English, Norwegian, Spanish, Portuguese, German, French, Japanese
const NOTIFICATION_LANGUAGE_VALUES = ['en', 'no', 'es', 'pt', 'de', 'fr', 'ja'] as const

interface ContactModalProps {
  isOpen: boolean
  onClose: () => void
  walletChecksum: string
  onContactSaved?: () => void
  editContact?: Contact
}

export function ContactModal({
  isOpen,
  onClose,
  walletChecksum,
  onContactSaved,
  editContact
}: ContactModalProps) {
  const t = useTranslations('contacts')
  const tCommon = useTranslations('common')
  const [name, setName] = useState("")
  const [language, setLanguage] = useState<typeof NOTIFICATION_LANGUAGE_VALUES[number]>('en')
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [enabledProviders, setEnabledProviders] = useState<Record<string, boolean>>({})
  const [providerValues, setProviderValues] = useState<Record<string, string>>({})
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [smsVerificationError, setSmsVerificationError] = useState<string | null>(null)
  const [phoneNumberError, setPhoneNumberError] = useState<string | null>(null)
  const [emailVerificationError, setEmailVerificationError] = useState<string | null>(null)
  const [emailAddressError, setEmailAddressError] = useState<string | null>(null)
  const [emailVerificationSent, setEmailVerificationSent] = useState(false)
  const [emailVerificationCode, setEmailVerificationCode] = useState("")
  const [emailVerificationAddress, setEmailVerificationAddress] = useState<string | null>(null)
  const [emailVerified, setEmailVerified] = useState(false)
  const [showEmailVerificationSuccess, setShowEmailVerificationSuccess] = useState(false)
  const [originalEmailAddress, setOriginalEmailAddress] = useState<string | null>(null)
  const [isSendingEmailVerification, setIsSendingEmailVerification] = useState(false)
  const [isVerifyingEmailCode, setIsVerifyingEmailCode] = useState(false)
  const [smsVerificationSent, setSmsVerificationSent] = useState(false)
  const [smsVerificationCode, setSmsVerificationCode] = useState("")
  const [smsVerificationPhone, setSmsVerificationPhone] = useState<string | null>(null)
  const [timeRemaining, setTimeRemaining] = useState<number>(0)
  const [isSendingVerification, setIsSendingVerification] = useState(false)
  const [isVerifyingCode, setIsVerifyingCode] = useState(false)
  const [smsVerified, setSmsVerified] = useState(false)
  const [showSmsVerificationSuccess, setShowSmsVerificationSuccess] = useState(false)
  const [originalPhoneNumber, setOriginalPhoneNumber] = useState<string | null>(null)
  const [hasChanges, setHasChanges] = useState(false)
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const timerRef = useRef<NodeJS.Timeout | null>(null)

  const isEditMode = !!editContact

  // Only calculate when modal is open to avoid unnecessary computation
  const phoneNumberChanged = isOpen ? (originalPhoneNumber !== null && 
    providerValues['twilio']?.trim() !== originalPhoneNumber) : false

  // Only calculate when modal is open to avoid unnecessary computation
  const emailAddressChanged = isOpen ? (originalEmailAddress !== null && 
    providerValues['email']?.trim() !== originalEmailAddress) : false

  // Check if SMS verification is required
  const smsVerificationRequired = isOpen ? (enabledProviders['twilio'] && 
    (phoneNumberChanged || (!isEditMode && !smsVerified) || (isEditMode && originalPhoneNumber === null && !smsVerified))) : false
  
  // Check if email verification is required
  const emailVerificationRequired = isOpen ? (enabledProviders['email'] && 
    (emailAddressChanged || (!isEditMode && !emailVerified) || (isEditMode && originalEmailAddress === null && !emailVerified))) : false
  

  // Clear timer on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current)
      }
    }
  }, [])

  // Start countdown timer
  const startTimer = useCallback(() => {
    setTimeRemaining(600) // 10 minutes in seconds
    if (timerRef.current) {
      clearInterval(timerRef.current)
    }
    timerRef.current = setInterval(() => {
      setTimeRemaining(prev => {
        if (prev <= 1) {
          if (timerRef.current) {
            clearInterval(timerRef.current)
          }
          // Auto-cancel verification when expired
          setSmsVerificationSent(false)
          setError(t('verification.expired'))
          return 0
        }
        return prev - 1
      })
    }, 1000)
  }, [])

  // Format time remaining as MM:SS
  const formatTime = (seconds: number) => {
    const mins = Math.floor(seconds / 60)
    const secs = seconds % 60
    return `${mins}:${secs.toString().padStart(2, '0')}`
  }

  // Fetch available providers
  const fetchProviders = useCallback(async () => {
    try {
      const response = await api.getProviders()
      setProviders(response.providers)
    } catch (err) {
      console.error('Failed to fetch providers:', err)
    }
  }, [])

  // Initialize form data when modal opens
  useEffect(() => {
    if (isOpen) {
      // Force clear all state first
      setError(null)
      setSmsVerificationError(null)
      setPhoneNumberError(null)
      setEmailVerificationError(null)
      setEmailAddressError(null)
      setSmsVerificationSent(false)
      setSmsVerificationCode("")
      setSmsVerificationPhone(null)
      setEmailVerificationSent(false)
      setEmailVerificationCode("")
      setEmailVerificationAddress(null)
      setTimeRemaining(0)
      setSmsVerified(false)
      setShowSmsVerificationSuccess(false)
      setEmailVerified(false)
      setShowEmailVerificationSuccess(false)
      setOriginalPhoneNumber(null)
      setOriginalEmailAddress(null)
      setHasChanges(false)
      setIsDeleteModalOpen(false)
      
      if (timerRef.current) {
        clearInterval(timerRef.current)
      }
      
      if (editContact) {
        // Populate form with existing contact data
        setName(editContact.name)
        setLanguage(editContact.language)
        
        // Set up providers based on existing notification methods
        const newEnabledProviders: Record<string, boolean> = {}
        const newProviderValues: Record<string, string> = {}
        
        editContact.notification_methods.forEach(method => {
          if (method.provider_type === 'sms') {
            const phoneNumber = method.display_target || method.notification_target
            newEnabledProviders['twilio'] = true
            newProviderValues['twilio'] = phoneNumber
            setOriginalPhoneNumber(phoneNumber)
            setSmsVerified(true) // SMS already exists on contact, so it's verified
          } else if (method.provider_type === 'ntfy') {
            newEnabledProviders['ntfy'] = true
          } else if (method.provider_type === 'email') {
            const emailAddress = method.display_target || method.notification_target
            newEnabledProviders['email'] = true
            newProviderValues['email'] = emailAddress
            setOriginalEmailAddress(emailAddress)
            setEmailVerified(true) // Email already exists on contact, so it's verified
          }
        })
        
        setEnabledProviders(newEnabledProviders)
        setProviderValues(newProviderValues)
      } else {
        // Reset form for new contact
        setName("")
        setLanguage('en')
        setEnabledProviders({})
        setProviderValues({})
      }

      if (providers.length === 0) {
        fetchProviders()
      }
    }
  }, [isOpen, editContact, fetchProviders, providers.length])

  const handleClose = () => {
    setError(null)
    setSmsVerificationError(null)
    setPhoneNumberError(null)
    setEmailVerificationError(null)
    setSmsVerificationSent(false)
    setSmsVerificationCode("")
    setSmsVerificationPhone(null)
    setEmailVerificationSent(false)
    setEmailVerificationCode("")
    setEmailVerificationAddress(null)
    setTimeRemaining(0)
    setSmsVerified(false)
    setShowSmsVerificationSuccess(false)
    setEmailVerified(false)
    setShowEmailVerificationSuccess(false)
    setOriginalPhoneNumber(null)
    setOriginalEmailAddress(null)
    setHasChanges(false)
    setIsDeleteModalOpen(false)
    if (timerRef.current) {
      clearInterval(timerRef.current)
    }
    onClose()
  }

  const handleSendSmsVerification = async () => {
    const phoneNumber = providerValues['twilio']?.trim()
    if (!phoneNumber) {
      setError(t('verification.smsRequired'))
      return
    }

    setIsSendingVerification(true)
    setError(null)

    try {
      await api.sendContactVerification(
        walletChecksum,
        name.trim() || `Contact-${phoneNumber.slice(-4)}`,
        language,
        phoneNumber,
        undefined // emailAddress
      )
      
      setSmsVerificationPhone(phoneNumber)
      setSmsVerificationSent(true)
      setSmsVerificationCode("")
      setError(null)
      startTimer()
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        // Use user-friendly message for network/server errors
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : "Failed to send verification code"
      }

      if (errorMessage.toLowerCase().includes("phone") || errorMessage.toLowerCase().includes("number")) {
        setPhoneNumberError(errorMessage)
      } else {
        setError(errorMessage)
      }
    } finally {
      setIsSendingVerification(false)
    }
  }

  const handleVerifySmsCode = async () => {
    if (!smsVerificationCode.trim() || !smsVerificationPhone) {
      setError(t('verification.enterCode'))
      return
    }

    setIsVerifyingCode(true)
    setError(null)

    try {
      // Use the new unified verify endpoint
      const result = await api.verifyContact(
        walletChecksum,
        smsVerificationCode.trim(),
        smsVerificationPhone,
        undefined // emailAddress
      )
      
      if (result.valid) {
        setSmsVerified(true)
        setShowSmsVerificationSuccess(true) // Only show success for fresh verification
        setError(null)
        setSmsVerificationError(null)
        
        // Clear the timer since verification is complete
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
        setTimeRemaining(0)
      } else {
        setSmsVerificationError(result.message || "Invalid verification code")
        setSmsVerificationCode("")
      }
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        // Use user-friendly message for network/server errors
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : "Invalid verification code"
      }

      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setSmsVerificationError(t('verification.expiredRequest'))
        setSmsVerificationSent(false)
        setSmsVerified(false)
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setSmsVerificationError(t('verification.invalid'))
        setSmsVerificationCode("") // Clear the input
      } else {
        setSmsVerificationError(errorMessage)
      }
    } finally {
      setIsVerifyingCode(false)
    }
  }

  const handleSendEmailVerification = async () => {
    const emailAddress = providerValues['email']?.trim()
    if (!emailAddress) {
      setEmailAddressError(t('verification.emailRequired'))
      return
    }

    setIsSendingEmailVerification(true)
    setEmailVerificationError(null)
    setEmailAddressError(null)

    try {
      const result = await api.sendContactVerification(
        walletChecksum,
        name.trim() || `Contact-${emailAddress.split('@')[0]}`,
        language,
        undefined, // phoneNumber
        emailAddress
      )
      
      // Check if email was auto-verified (user's own email)
      if (result.auto_verified) {
        setEmailVerified(true)
        setShowEmailVerificationSuccess(true)
        setError(null)
      } else {
        setEmailVerificationAddress(emailAddress)
        setEmailVerificationSent(true)
        setEmailVerificationCode("")
        setError(null)
        startTimer()
      }
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        // Use user-friendly message for network/server errors
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : "Failed to send verification code"
      }

      if (errorMessage.toLowerCase().includes("email") || errorMessage.toLowerCase().includes("address")) {
        setEmailAddressError(errorMessage)
      } else {
        setError(errorMessage)
      }
    } finally {
      setIsSendingEmailVerification(false)
    }
  }

  const handleVerifyEmailCode = async () => {
    if (!emailVerificationCode.trim() || !emailVerificationAddress) {
      setEmailVerificationError(t('verification.enterCode'))
      return
    }

    setIsVerifyingEmailCode(true)
    setEmailVerificationError(null)

    try {
      const result = await api.verifyContact(
        walletChecksum,
        emailVerificationCode.trim(),
        undefined, // phoneNumber
        emailVerificationAddress
      )
      
      if (result.valid) {
        setEmailVerified(true)
        setShowEmailVerificationSuccess(true)
        setError(null)
        setEmailVerificationError(null)
      setEmailAddressError(null)
        
        // Clear the timer since verification is complete
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
        setTimeRemaining(0)
      } else {
        setEmailVerificationError(result.message || "Invalid verification code")
        setEmailVerificationCode("")
      }
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        // Use user-friendly message for network/server errors
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : "Invalid verification code"
      }

      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setEmailVerificationError(t('verification.expiredRequest'))
        setEmailVerificationSent(false)
        setEmailVerified(false)
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setEmailVerificationError(t('verification.invalid'))
        setEmailVerificationCode("")
      } else {
        setEmailVerificationError(errorMessage)
      }
    } finally {
      setIsVerifyingEmailCode(false)
    }
  }

  const handleSubmit = async () => {
    if (!name.trim()) {
      setError(t('errors.nameRequired'))
      return
    }

    // Check what's enabled
    const hasNtfy = enabledProviders['ntfy'] || false
    const hasSms = enabledProviders['twilio'] && providerValues['twilio']?.trim()
    const hasEmail = enabledProviders['email'] && providerValues['email']?.trim()

    // Check if SMS verification is required but not completed
    if (smsVerificationRequired && !smsVerified) {
      if (phoneNumberChanged) {
        setError(t('verification.verifyNewSms'))
      } else {
        setError(t('verification.verifySmsFirst'))
      }
      return
    }

    // Check if email verification is required but not completed
    if (emailVerificationRequired && !emailVerified) {
      if (emailAddressChanged) {
        setError(t('verification.verifyNewEmail'))
      } else {
        setError(t('verification.verifyEmailFirst'))
      }
      return
    }

    setIsSubmitting(true)
    setError(null)

    try {
      // If verification requirements are met
      if ((!hasSms || (hasSms && smsVerified)) && (!hasEmail || (hasEmail && emailVerified))) {
        const notificationMethods: { provider_type: 'sms' | 'ntfy' | 'email'; notification_target: string }[] = []
        
        if (hasNtfy) {
          notificationMethods.push({ provider_type: 'ntfy', notification_target: '' })
        }
        
        if (hasEmail && emailVerified) {
          notificationMethods.push({ 
            provider_type: 'email', 
            notification_target: emailVerificationAddress || providerValues['email'].trim() 
          })
        }
        
        if (hasSms && smsVerified) {
          notificationMethods.push({ 
            provider_type: 'sms', 
            notification_target: smsVerificationPhone || providerValues['twilio'].trim()
          })
        }
        
        if (isEditMode && editContact) {
          // Use PUT for updates - atomic transaction
          await api.updateContact(
            walletChecksum,
            editContact.id,
            name.trim(),
            language,
            notificationMethods
          )
        } else {
          // Use POST for creation
          await api.createContact(
            walletChecksum,
            name.trim(),
            language,
            notificationMethods
          )
        }

        handleClose()
        if (onContactSaved) {
          onContactSaved()
        }
      }
    } catch (err) {
      let errorMessage: string
      if (err instanceof ApiError) {
        // Use user-friendly message for network/server errors
        errorMessage = err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message
      } else {
        errorMessage = err instanceof Error ? err.message : `Failed to ${isEditMode ? 'update' : 'create'} contact`
      }

      // Provide more specific error messages for SMS verification
      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setError(t('verification.expiredRequest'))
        setSmsVerificationSent(false)
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setError(t('verification.invalid'))
        setSmsVerificationCode("") // Clear the input
      } else {
        setError(errorMessage)
      }
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleDeleteContact = async () => {
    if (!editContact) return
    
    await api.deleteContact(walletChecksum, editContact.id)
    handleClose()
    if (onContactSaved) {
      onContactSaved()
    }
  }

  const handleResendCode = async () => {
    if (!smsVerificationPhone) return
    
    setIsSendingVerification(true)
    setError(null)
    
    try {
      await api.sendContactVerification(
        walletChecksum,
        name.trim(),
        language,
        smsVerificationPhone
      )
      
      setSmsVerificationCode("")
      startTimer()
      setError(null)
    } catch (err) {
      if (err instanceof ApiError) {
        // Use user-friendly message for network/server errors
        setError(err.isNetworkError() || err.isServerError()
          ? err.getUserFriendlyMessage()
          : err.message)
      } else {
        setError(err instanceof Error ? err.message : "Failed to resend code")
      }
    } finally {
      setIsSendingVerification(false)
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {isEditMode ? t('edit.title') : t('add.title')}
          </DialogTitle>
          <DialogDescription>
            {isEditMode ? t('edit.description') : t('add.description')}
          </DialogDescription>
        </DialogHeader>

        {error && (
          <div className="text-sm text-red-600 bg-red-50 p-3 rounded-md border border-red-200">
            {error}
          </div>
        )}

        <div className="space-y-4">
          <div>
            <Label htmlFor="contact-name">{t('add.nameLabel')}</Label>
            <Input
              id="contact-name"
              value={name}
              onChange={(e) => {
                setName(e.target.value)
                setHasChanges(true)
              }}
              placeholder={t('add.namePlaceholder')}
              disabled={isSubmitting}
            />
          </div>

          <div>
            <Label htmlFor="contact-language">{t('add.languageLabel')}</Label>
            <select
              id="contact-language"
              value={language}
              onChange={(e) => {
                setLanguage(e.target.value as typeof NOTIFICATION_LANGUAGE_VALUES[number])
                setHasChanges(true)
              }}
              disabled={isSubmitting}
              className="w-full h-10 border border-input bg-background px-3 py-2 rounded-md text-sm"
            >
              {NOTIFICATION_LANGUAGE_VALUES.map((langValue) => (
                <option key={langValue} value={langValue}>
                  {t(`languages.${langValue}`)}
                </option>
              ))}
            </select>
          </div>

          <div>
            <Label>{t('add.methodTitle')}</Label>
            <div className="space-y-3 mt-2">
              {providers.map((provider) => (
                <div key={provider.name} className="p-3 border rounded-lg">
                  <label className="flex items-start gap-3">
                    <input
                      type="checkbox"
                      checked={enabledProviders[provider.name] || false}
                      onChange={(e) => {
                        setEnabledProviders(prev => ({
                          ...prev,
                          [provider.name]: e.target.checked
                        }))
                        setHasChanges(true)
                      }}
                      disabled={isSubmitting}
                      className="mt-1"
                    />
                    <div className="flex-1">
                      <div className="flex items-center gap-2 mb-1">
                        {provider.name === 'twilio' ? (
                          <MessageCircle className="h-4 w-4" />
                        ) : provider.name === 'email' ? (
                          <Mail className="h-4 w-4" />
                        ) : (
                          <Bell className="h-4 w-4" />
                        )}
                        <span className="font-medium">{t(`add.providers.${provider.name}`)}</span>
                      </div>
                      {enabledProviders[provider.name] && provider.name === 'twilio' && (
                        <div className="mt-2 space-y-3">
                          <div>
                            <Input
                              value={providerValues[provider.name] || ''}
                              onChange={(e) => {
                                setProviderValues(prev => ({
                                  ...prev,
                                  [provider.name]: e.target.value
                                }))
                                setHasChanges(true)
                                // Clear phone number error when user starts typing
                                if (phoneNumberError) {
                                  setPhoneNumberError(null)
                                }
                                // Reset verification state when phone number changes from original
                                const newPhoneNumber = e.target.value.trim()
                                if (originalPhoneNumber !== null && newPhoneNumber !== originalPhoneNumber) {
                                  setSmsVerificationSent(false)
                                  setSmsVerificationCode("")
                                  setSmsVerificationPhone(null)
                                  setSmsVerified(false)
                                  setTimeRemaining(0)
                                  if (timerRef.current) {
                                    clearInterval(timerRef.current)
                                  }
                                } else if (originalPhoneNumber !== null && newPhoneNumber === originalPhoneNumber) {
                                  // Phone number reverted to original, mark as verified
                                  setSmsVerified(true)
                                  setSmsVerificationSent(false)
                                  setSmsVerificationCode("")
                                  setSmsVerificationPhone(null)
                                  setTimeRemaining(0)
                                  if (timerRef.current) {
                                    clearInterval(timerRef.current)
                                  }
                                }
                              }}
                              placeholder="+1234567890"
                              disabled={isSubmitting || isSendingVerification}
                              className={phoneNumberError ? 'border-red-500 focus:border-red-500' : ''}
                            />
                            {/* Phone number error under the input */}
                            {phoneNumberError && (
                              <div className="text-sm text-red-600 mt-1">
                                {phoneNumberError}
                              </div>
                            )}
                            {(!providerValues[provider.name] || !smsVerified) && !phoneNumberError && (
                              <p className="text-xs text-muted-foreground mt-1">
                                {t('add.sms.phoneHint')}
                              </p>
                            )}
                          </div>
                          
                          {/* Send Verification Button - only show when verification is required */}
                          {(smsVerificationRequired && !smsVerificationSent) && (
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={handleSendSmsVerification}
                              disabled={isSendingVerification || isSubmitting || !providerValues['twilio']?.trim()}
                              className="w-full"
                            >
                              {isSendingVerification ? t('verification.sendingCode') : t('verification.sendCode')}
                            </Button>
                          )}

                          {/* OTP Input Field */}
                          {smsVerificationSent && !smsVerified && (
                            <div className="space-y-3">
                              <div>
                                <Label htmlFor="sms-verification-code">{t('verification.codeLabel')}</Label>
                                <div className="flex gap-2">
                                  <Input
                                    id="sms-verification-code"
                                    value={smsVerificationCode}
                                    onChange={(e) => {
                                      setSmsVerificationCode(e.target.value)
                                      // Clear SMS verification error when user starts typing
                                      if (smsVerificationError) {
                                        setSmsVerificationError(null)
                                      }
                                    }}
                                    placeholder={t('verification.codePlaceholder')}
                                    disabled={isSubmitting || isVerifyingCode}
                                    maxLength={6}
                                    autoComplete="one-time-code"
                                    autoCorrect="off"
                                    autoCapitalize="off"
                                    spellCheck="false"
                                    inputMode="numeric"
                                    className={`flex-1 ${smsVerificationError ? 'border-red-500 focus:border-red-500' : ''}`}
                                  />
                                  <Button
                                    type="button"
                                    variant="outline"
                                    onClick={handleVerifySmsCode}
                                    disabled={!smsVerificationCode.trim() || isVerifyingCode || isSubmitting}
                                  >
                                    {isVerifyingCode ? t('verification.verifying') : t('verification.verify')}
                                  </Button>
                                </div>
                                {/* SMS verification error under the input */}
                                {smsVerificationError && (
                                  <div className="text-sm text-red-600 mt-1">
                                    {smsVerificationError}
                                  </div>
                                )}
                              </div>
                              <div className="flex justify-between items-center text-xs text-muted-foreground">
                                <span>
                                  {t('verification.codeSentTo', { target: smsVerificationPhone || '' })}
                                  {timeRemaining > 0 && (
                                    <span className="block">{t('verification.expiresIn', { time: formatTime(timeRemaining) })}</span>
                                  )}
                                </span>
                                <button
                                  type="button"
                                  onClick={handleResendCode}
                                  disabled={isSendingVerification || timeRemaining > 540} // Allow resend after 1 minute
                                  className="text-blue-600 hover:text-blue-800 disabled:text-gray-400 underline"
                                >
                                  {t('verification.resend')}
                                </button>
                              </div>
                            </div>
                          )}

                          {/* Verification Success - only show after fresh verification */}
                          {showSmsVerificationSuccess && (
                            <div className="flex items-center gap-2 text-green-600 text-sm">
                              <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                              </svg>
                              {t('verification.smsVerified')}
                            </div>
                          )}
                        </div>
                      )}
                      {enabledProviders[provider.name] && provider.name === 'email' && (
                        <div className="mt-2 space-y-3">
                          <div>
                            <Input
                              value={providerValues[provider.name] || ''}
                              onChange={(e) => {
                                setProviderValues(prev => ({
                                  ...prev,
                                  [provider.name]: e.target.value
                                }))
                                setHasChanges(true)
                                // Clear email errors when user starts typing
                                if (emailVerificationError) {
                                  setEmailVerificationError(null)
                                }
                                if (emailAddressError) {
                                  setEmailAddressError(null)
                                }
                                // Reset verification state when email address changes from original
                                const newEmailAddress = e.target.value.trim()
                                if (originalEmailAddress !== null && newEmailAddress !== originalEmailAddress) {
                                  setEmailVerificationSent(false)
                                  setEmailVerificationCode("")
                                  setEmailVerificationAddress(null)
                                  setEmailVerified(false)
                                  setTimeRemaining(0)
                                  if (timerRef.current) {
                                    clearInterval(timerRef.current)
                                  }
                                } else if (originalEmailAddress !== null && newEmailAddress === originalEmailAddress) {
                                  // Email address reverted to original, mark as verified
                                  setEmailVerified(true)
                                  setEmailVerificationSent(false)
                                  setEmailVerificationCode("")
                                  setEmailVerificationAddress(null)
                                  setTimeRemaining(0)
                                  if (timerRef.current) {
                                    clearInterval(timerRef.current)
                                  }
                                }
                              }}
                              placeholder={tCommon('emailPlaceholder')}
                              disabled={isSubmitting || isSendingEmailVerification}
                              type="email"
                              className={emailAddressError ? 'border-red-500 focus:border-red-500' : ''}
                            />
                            {/* Email address error under the input */}
                            {emailAddressError && (
                              <div className="text-sm text-red-600 mt-1">
                                {emailAddressError}
                              </div>
                            )}
                            {(!providerValues[provider.name] || !emailVerified) && !emailAddressError && (
                              <p className="text-xs text-muted-foreground mt-1">
                                {t('add.email.emailHint')}
                              </p>
                            )}
                          </div>

                          {/* Send Verification Button - only show when verification is required */}
                          {(emailVerificationRequired && !emailVerificationSent) && (
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={handleSendEmailVerification}
                              disabled={isSendingEmailVerification || isSubmitting || !providerValues['email']?.trim()}
                              className="w-full"
                            >
                              {isSendingEmailVerification ? t('verification.sendingCode') : t('verification.sendCode')}
                            </Button>
                          )}

                          {/* OTP Input Field */}
                          {emailVerificationSent && !emailVerified && (
                            <div className="space-y-3">
                              <div>
                                <Label htmlFor="email-verification-code">{t('verification.codeLabel')}</Label>
                                <div className="flex gap-2">
                                  <Input
                                    id="email-verification-code"
                                    value={emailVerificationCode}
                                    onChange={(e) => {
                                      setEmailVerificationCode(e.target.value)
                                      // Clear email verification error when user starts typing
                                      if (emailVerificationError) {
                                        setEmailVerificationError(null)
                                        setEmailAddressError(null)
                                      }
                                    }}
                                    placeholder={t('verification.codePlaceholder')}
                                    disabled={isSubmitting || isVerifyingEmailCode}
                                    maxLength={6}
                                    autoComplete="one-time-code"
                                    autoCorrect="off"
                                    autoCapitalize="off"
                                    spellCheck="false"
                                    inputMode="numeric"
                                    className={`flex-1 ${emailVerificationError ? 'border-red-500 focus:border-red-500' : ''}`}
                                  />
                                  <Button
                                    type="button"
                                    variant="outline"
                                    onClick={handleVerifyEmailCode}
                                    disabled={!emailVerificationCode.trim() || isVerifyingEmailCode || isSubmitting}
                                  >
                                    {isVerifyingEmailCode ? t('verification.verifying') : t('verification.verify')}
                                  </Button>
                                </div>
                                {/* Email verification error under the input */}
                                {emailVerificationError && (
                                  <div className="text-sm text-red-600 mt-1">
                                    {emailVerificationError}
                                  </div>
                                )}
                              </div>
                              <div className="flex justify-between items-center text-xs text-muted-foreground">
                                <span>
                                  {t('verification.codeSentTo', { target: emailVerificationAddress || '' })}
                                  {timeRemaining > 0 && (
                                    <span className="block">{t('verification.expiresIn', { time: formatTime(timeRemaining) })}</span>
                                  )}
                                </span>
                                <button
                                  type="button"
                                  onClick={() => handleSendEmailVerification()}
                                  disabled={isSendingEmailVerification || timeRemaining > 540} // Allow resend after 1 minute
                                  className="text-blue-600 hover:text-blue-800 disabled:text-gray-400 underline"
                                >
                                  {t('verification.resend')}
                                </button>
                              </div>
                            </div>
                          )}

                          {/* Verification Success - only show after fresh verification */}
                          {showEmailVerificationSuccess && (
                            <div className="flex items-center gap-2 text-green-600 text-sm">
                              <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                              </svg>
                              {t('verification.emailVerified')}
                            </div>
                          )}
                        </div>
                      )}
                      {enabledProviders[provider.name] && provider.name === 'ntfy' && (
                        <p className="text-xs text-muted-foreground mt-1">
                          {t('add.ntfy.topicHint')}
                        </p>
                      )}
                    </div>
                  </label>
                </div>
              ))}
            </div>
          </div>

        </div>

        <DialogFooter className={isEditMode ? "sm:justify-between" : ""}>
          {isEditMode && (
            <Button
              variant="destructive"
              onClick={() => setIsDeleteModalOpen(true)}
              disabled={isSubmitting}
            >
              {t('delete.confirm')}
            </Button>
          )}
          <Button onClick={handleSubmit} disabled={isSubmitting || (isEditMode && !hasChanges)}>
            {isSubmitting ? t('add.submitting') : (isEditMode ? t('edit.submit') : t('add.submit'))}
          </Button>
        </DialogFooter>
      </DialogContent>

      <DeleteContactModal
        contact={editContact || null}
        isOpen={isDeleteModalOpen}
        onClose={() => setIsDeleteModalOpen(false)}
        onConfirmDelete={handleDeleteContact}
      />
    </Dialog>
  )
}