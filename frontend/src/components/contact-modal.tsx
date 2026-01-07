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
import { SmsProviderFields, EmailProviderFields, NtfyProviderFields } from "./contact-modal/index"
import { useTranslations } from "next-intl"
import { usePhonePlaceholder } from "@/hooks/usePhonePlaceholder"
import { useSmsVerification } from "@/hooks/useSmsVerification"
import { useEmailVerification } from "@/hooks/useEmailVerification"

// Helper to sanitize name for ntfy topic
function sanitizeForNtfyTopic(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9-]/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
    .substring(0, 30) // Leave room for checksum suffix
}

// Generate default ntfy topic from name and wallet checksum
function generateDefaultNtfyTopic(name: string, walletChecksum: string): string {
  const sanitizedName = sanitizeForNtfyTopic(name)
  if (!sanitizedName) {
    return walletChecksum.substring(0, 8)
  }
  return `${sanitizedName}-${walletChecksum.substring(0, 8)}`
}

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
  const phonePlaceholder = usePhonePlaceholder()
  const [name, setName] = useState("")
  const [ntfyTopic, setNtfyTopic] = useState("")
  const [userEditedNtfyTopic, setUserEditedNtfyTopic] = useState(false)
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [enabledProviders, setEnabledProviders] = useState<Record<string, boolean>>({})
  const [providerValues, setProviderValues] = useState<Record<string, string>>({})
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [originalPhoneNumber, setOriginalPhoneNumber] = useState<string | null>(null)
  const [originalEmailAddress, setOriginalEmailAddress] = useState<string | null>(null)
  const [originalName, setOriginalName] = useState<string | null>(null)
  const [originalNtfyTopic, setOriginalNtfyTopic] = useState<string | null>(null)
  const [originalNtfyEnabled, setOriginalNtfyEnabled] = useState<boolean>(false)
  const [originalSmsEnabled, setOriginalSmsEnabled] = useState<boolean>(false)
  const [originalEmailEnabled, setOriginalEmailEnabled] = useState<boolean>(false)
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)
  const nameInputRef = useRef<HTMLInputElement>(null)

  const isEditMode = !!editContact

  // Use verification hooks for SMS and email
  const smsVerification = useSmsVerification({
    walletChecksum,
    contactName: name,
    originalPhoneNumber,
    onError: setError
  })

  const emailVerification = useEmailVerification({
    walletChecksum,
    contactName: name,
    originalEmailAddress,
    onError: setError
  })

  // Only calculate when modal is open to avoid unnecessary computation
  const phoneNumberChanged = isOpen ? (originalPhoneNumber !== null &&
    providerValues['twilio']?.trim() !== originalPhoneNumber) : false

  // Only calculate when modal is open to avoid unnecessary computation
  const emailAddressChanged = isOpen ? (originalEmailAddress !== null &&
    providerValues['email']?.trim() !== originalEmailAddress) : false

  // Check if SMS verification is required
  const smsVerificationRequired = isOpen ? (enabledProviders['twilio'] &&
    (phoneNumberChanged || (!isEditMode && !smsVerification.isVerified) || (isEditMode && originalPhoneNumber === null && !smsVerification.isVerified))) : false

  // Check if email verification is required
  const emailVerificationRequired = isOpen ? (enabledProviders['email'] &&
    (emailAddressChanged || (!isEditMode && !emailVerification.isVerified) || (isEditMode && originalEmailAddress === null && !emailVerification.isVerified))) : false

  // Compute hasChanges by comparing current values to original values
  // In edit mode, if originalName is null, the initialization effect hasn't run yet
  const hasChanges = isOpen && isEditMode ? (
    originalName === null ? false : (
      name.trim() !== originalName ||
      ntfyTopic !== (originalNtfyTopic || '') ||
      (enabledProviders['ntfy'] || false) !== originalNtfyEnabled ||
      (enabledProviders['twilio'] || false) !== originalSmsEnabled ||
      (enabledProviders['email'] || false) !== originalEmailEnabled ||
      (providerValues['twilio']?.trim() || '') !== (originalPhoneNumber || '') ||
      (providerValues['email']?.trim() || '') !== (originalEmailAddress || '')
    )
  ) : true  // Always allow submit for new contacts

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
      setOriginalPhoneNumber(null)
      setOriginalEmailAddress(null)
      setOriginalName(null)
      setOriginalNtfyTopic(null)
      setOriginalNtfyEnabled(false)
      setOriginalSmsEnabled(false)
      setOriginalEmailEnabled(false)
      setIsDeleteModalOpen(false)
      setNtfyTopic("")
      setUserEditedNtfyTopic(false)

      // Reset verification hooks
      smsVerification.reset()
      emailVerification.reset()

      if (editContact) {
        // Populate form with existing contact data
        setName(editContact.name)
        setOriginalName(editContact.name)

        // Set up providers based on existing notification methods
        const newEnabledProviders: Record<string, boolean> = {}
        const newProviderValues: Record<string, string> = {}

        editContact.notification_methods.forEach(method => {
          if (method.provider_type === 'sms') {
            const phoneNumber = method.display_target || method.notification_target
            newEnabledProviders['twilio'] = true
            newProviderValues['twilio'] = phoneNumber
            setOriginalPhoneNumber(phoneNumber)
            setOriginalSmsEnabled(true)
            smsVerification.setVerified(true) // SMS already exists on contact, so it's verified
          } else if (method.provider_type === 'ntfy') {
            newEnabledProviders['ntfy'] = true
            // Pre-populate ntfy topic from existing notification method
            const existingTopic = method.notification_target
            setNtfyTopic(existingTopic)
            setOriginalNtfyTopic(existingTopic)
            setOriginalNtfyEnabled(true)
            setUserEditedNtfyTopic(true) // Mark as edited so it doesn't auto-update
          } else if (method.provider_type === 'email') {
            const emailAddress = method.display_target || method.notification_target
            newEnabledProviders['email'] = true
            newProviderValues['email'] = emailAddress
            setOriginalEmailAddress(emailAddress)
            setOriginalEmailEnabled(true)
            emailVerification.setVerified(true) // Email already exists on contact, so it's verified
          }
        })

        setEnabledProviders(newEnabledProviders)
        setProviderValues(newProviderValues)
      } else {
        // Reset form for new contact
        setName("")
        setEnabledProviders({})
        setProviderValues({})
      }

      if (providers.length === 0) {
        fetchProviders()
      }
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, editContact, fetchProviders, providers.length])

  const handleClose = () => {
    setError(null)
    setOriginalPhoneNumber(null)
    setOriginalEmailAddress(null)
    setOriginalName(null)
    setOriginalNtfyTopic(null)
    setOriginalNtfyEnabled(false)
    setOriginalSmsEnabled(false)
    setOriginalEmailEnabled(false)
    setIsDeleteModalOpen(false)
    // Reset form values
    setName("")
    setNtfyTopic("")
    setUserEditedNtfyTopic(false)
    setEnabledProviders({})
    setProviderValues({})
    // Reset verification hooks
    smsVerification.reset()
    emailVerification.reset()
    onClose()
  }

  // SMS verification handlers - delegate to hook
  const handleSendSmsVerification = () => {
    const phoneNumber = providerValues['twilio']?.trim()
    if (phoneNumber) {
      smsVerification.sendVerification(phoneNumber)
    }
  }

  const handleVerifySmsCode = () => {
    smsVerification.verifyCode()
  }

  // Email verification handlers - delegate to hook
  const handleSendEmailVerification = () => {
    const emailAddress = providerValues['email']?.trim()
    if (emailAddress) {
      emailVerification.sendVerification(emailAddress)
    }
  }

  const handleVerifyEmailCode = () => {
    emailVerification.verifyCode()
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

    // Validate ntfy topic if ntfy is enabled
    if (hasNtfy && !ntfyTopic.trim()) {
      setError(t('errors.ntfyTopicRequired'))
      return
    }

    // Check if SMS verification is required but not completed
    if (smsVerificationRequired && !smsVerification.isVerified) {
      if (phoneNumberChanged) {
        setError(t('verification.verifyNewSms'))
      } else {
        setError(t('verification.verifySmsFirst'))
      }
      return
    }

    // Check if email verification is required but not completed
    if (emailVerificationRequired && !emailVerification.isVerified) {
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
      if ((!hasSms || (hasSms && smsVerification.isVerified)) && (!hasEmail || (hasEmail && emailVerification.isVerified))) {
        const notificationMethods: { provider_type: 'sms' | 'ntfy' | 'email'; notification_target: string }[] = []

        if (hasNtfy) {
          notificationMethods.push({ provider_type: 'ntfy', notification_target: ntfyTopic.trim() })
        }

        if (hasEmail && emailVerification.isVerified) {
          notificationMethods.push({
            provider_type: 'email',
            notification_target: emailVerification.verificationAddress || providerValues['email'].trim()
          })
        }

        if (hasSms && smsVerification.isVerified) {
          notificationMethods.push({
            provider_type: 'sms',
            notification_target: smsVerification.verificationPhone || providerValues['twilio'].trim()
          })
        }

        if (isEditMode && editContact) {
          // Use PUT for updates - atomic transaction
          await api.updateContact(
            walletChecksum,
            editContact.id,
            name.trim(),
            notificationMethods
          )
        } else {
          // Use POST for creation
          await api.createContact(
            walletChecksum,
            name.trim(),
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

      // Provide more specific error messages for verification issues
      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setError(t('verification.expiredRequest'))
        smsVerification.reset()
        emailVerification.reset()
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setError(t('verification.invalid'))
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

  // Resend handlers - delegate to hooks
  const handleResendSmsCode = () => {
    smsVerification.resendCode()
  }

  const handleResendEmailCode = () => {
    emailVerification.resendCode()
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent
        className="sm:max-w-md"
        onOpenAutoFocus={(e) => {
          // Only auto-focus name input when creating new contact, not when editing
          if (isEditMode) {
            e.preventDefault()
          } else {
            // Focus name input for new contacts
            nameInputRef.current?.focus()
          }
        }}
      >
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
                const newName = e.target.value
                setName(newName)
                // Auto-update ntfy topic when name changes (if user hasn't manually edited it)
                if (enabledProviders['ntfy'] && !userEditedNtfyTopic) {
                  setNtfyTopic(generateDefaultNtfyTopic(newName, walletChecksum))
                }
              }}
              ref={nameInputRef}
              placeholder={t('add.namePlaceholder')}
              disabled={isSubmitting}
            />
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
                        // Generate default ntfy topic when ntfy is enabled and no topic set
                        if (provider.name === 'ntfy' && e.target.checked && !ntfyTopic && !userEditedNtfyTopic) {
                          setNtfyTopic(generateDefaultNtfyTopic(name, walletChecksum))
                        }
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
                        <SmsProviderFields
                          phoneNumber={providerValues[provider.name] || ''}
                          onPhoneNumberChange={(value) => {
                            setProviderValues(prev => ({
                              ...prev,
                              [provider.name]: value
                            }))
                            smsVerification.clearPhoneError()
                            const newPhoneNumber = value.trim()
                            if (originalPhoneNumber !== null && newPhoneNumber !== originalPhoneNumber) {
                              smsVerification.resetForPhoneChange(newPhoneNumber)
                            } else if (originalPhoneNumber !== null && newPhoneNumber === originalPhoneNumber) {
                              smsVerification.revertToOriginal()
                            }
                          }}
                          phonePlaceholder={phonePlaceholder}
                          phoneError={smsVerification.phoneError}
                          disabled={isSubmitting}
                          verificationRequired={smsVerificationRequired}
                          verificationSent={smsVerification.verificationSent}
                          verificationCode={smsVerification.verificationCode}
                          onVerificationCodeChange={(code) => {
                            smsVerification.setVerificationCode(code)
                            smsVerification.clearVerificationError()
                          }}
                          verificationPhone={smsVerification.verificationPhone}
                          verificationError={smsVerification.verificationError}
                          isVerified={smsVerification.isVerified}
                          showSuccess={smsVerification.showSuccess}
                          isSending={smsVerification.isSending}
                          isVerifying={smsVerification.isVerifying}
                          timeRemaining={smsVerification.timeRemaining}
                          formatTime={smsVerification.formatTime}
                          onSendVerification={handleSendSmsVerification}
                          onVerifyCode={handleVerifySmsCode}
                          onResendCode={handleResendSmsCode}
                        />
                      )}
                      {enabledProviders[provider.name] && provider.name === 'email' && (
                        <EmailProviderFields
                          emailAddress={providerValues[provider.name] || ''}
                          onEmailAddressChange={(value) => {
                            setProviderValues(prev => ({
                              ...prev,
                              [provider.name]: value
                            }))
                            emailVerification.clearVerificationError()
                            emailVerification.clearEmailError()
                            const newEmailAddress = value.trim()
                            if (originalEmailAddress !== null && newEmailAddress !== originalEmailAddress) {
                              emailVerification.resetForEmailChange(newEmailAddress)
                            } else if (originalEmailAddress !== null && newEmailAddress === originalEmailAddress) {
                              emailVerification.revertToOriginal()
                            }
                          }}
                          emailPlaceholder={tCommon('emailPlaceholder')}
                          emailError={emailVerification.emailError}
                          disabled={isSubmitting}
                          verificationRequired={emailVerificationRequired}
                          verificationSent={emailVerification.verificationSent}
                          verificationCode={emailVerification.verificationCode}
                          onVerificationCodeChange={(code) => {
                            emailVerification.setVerificationCode(code)
                            emailVerification.clearVerificationError()
                          }}
                          verificationAddress={emailVerification.verificationAddress}
                          verificationError={emailVerification.verificationError}
                          isVerified={emailVerification.isVerified}
                          showSuccess={emailVerification.showSuccess}
                          isSending={emailVerification.isSending}
                          isVerifying={emailVerification.isVerifying}
                          timeRemaining={emailVerification.timeRemaining}
                          formatTime={emailVerification.formatTime}
                          onSendVerification={handleSendEmailVerification}
                          onVerifyCode={handleVerifyEmailCode}
                          onResendCode={handleResendEmailCode}
                        />
                      )}
                      {enabledProviders[provider.name] && provider.name === 'ntfy' && (
                        <NtfyProviderFields
                          topic={ntfyTopic}
                          onTopicChange={(value) => {
                            setNtfyTopic(value)
                            setUserEditedNtfyTopic(true)
                          }}
                          defaultTopicPlaceholder={generateDefaultNtfyTopic(name || 'contact', walletChecksum)}
                          disabled={isSubmitting}
                        />
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
              {tCommon('delete')}
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