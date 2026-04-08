"use client"

import { useState, useCallback, useEffect, useRef } from "react"
import {
  ResponsiveModal,
  ResponsiveModalContent,
  ResponsiveModalHeader,
  ResponsiveModalTitle,
  ResponsiveModalDescription,
  ResponsiveModalFooter,
} from "@/components/ui/responsive-modal"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Bell, MessageCircle, Mail, ChevronLeft } from "lucide-react"
import { api, ProviderInfo, ApiError } from "../lib/api"
import { getTranslatedApiError } from "../lib/utils"
import { Contact } from "../types"
import { DeleteContactModal } from "./delete-contact-modal"
import { SmsProviderFields, EmailProviderFields, NtfyProviderFields } from "./contact-modal/index"
import { StepIndicator } from "./contact-modal/step-indicator"
import { useTranslations } from "next-intl"
import { usePhonePlaceholder } from "@/hooks/usePhonePlaceholder"
import { useSmsVerification } from "@/hooks/useSmsVerification"
import { useEmailVerification } from "@/hooks/useEmailVerification"
import { useOriginalContactState } from "@/hooks/useOriginalContactState"
import { useContactChangeDetection } from "@/hooks/useContactChangeDetection"
import { useNtfyServerUrl } from "@/hooks/useNtfyServerUrl"
import { useIsMobile } from "@/hooks/useIsMobile"
import { useContactWizard } from "@/hooks/useContactWizard"

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
  const tApiErrors = useTranslations('errors.api')
  const phonePlaceholder = usePhonePlaceholder()
  const ntfyServerUrl = useNtfyServerUrl()
  const [name, setName] = useState("")
  const [ntfyTopic, setNtfyTopic] = useState("")
  const [userEditedNtfyTopic, setUserEditedNtfyTopic] = useState(false)
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [enabledProviders, setEnabledProviders] = useState<Record<string, boolean>>({})
  const [providerValues, setProviderValues] = useState<Record<string, string>>({})
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false)

  // Consolidated original state management
  const { originalState, initializeFromContact, reset: resetOriginalState } = useOriginalContactState()
  const nameInputRef = useRef<HTMLInputElement>(null)

  const isEditMode = !!editContact

  // Use verification hooks for SMS and email
  const smsVerification = useSmsVerification({
    walletChecksum,
    contactName: name,
    originalPhoneNumber: originalState.phoneNumber,
    onError: setError
  })

  const emailVerification = useEmailVerification({
    walletChecksum,
    contactName: name,
    originalEmailAddress: originalState.emailAddress,
    onError: setError
  })

  // Consolidated change detection using custom hook
  const {
    phoneNumberChanged,
    emailAddressChanged,
    smsVerificationRequired,
    emailVerificationRequired,
    hasChanges,
  } = useContactChangeDetection({
    isOpen,
    isEditMode,
    originalState,
    currentName: name,
    currentNtfyTopic: ntfyTopic,
    enabledProviders,
    providerValues,
    smsVerified: smsVerification.isVerified,
    emailVerified: emailVerification.isVerified,
  })

  const isMobile = useIsMobile()
  const wizard = useContactWizard({
    name,
    enabledProviders,
    providerValues,
    ntfyTopic,
    smsVerificationRequired,
    emailVerificationRequired,
    smsVerified: smsVerification.isVerified,
    emailVerified: emailVerification.isVerified,
  })

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
      // Reset all state first
      setError(null)
      setIsDeleteModalOpen(false)
      setNtfyTopic("")
      setUserEditedNtfyTopic(false)
      resetOriginalState()

      // Reset verification hooks and wizard
      smsVerification.reset()
      emailVerification.reset()
      wizard.reset()

      if (editContact) {
        // Populate form with existing contact data using consolidated helper
        setName(editContact.name)
        const { enabledProviders: newEnabled, providerValues: newValues, ntfyTopic: existingTopic } =
          initializeFromContact(editContact)

        setEnabledProviders(newEnabled)
        setProviderValues(newValues)

        if (existingTopic) {
          setNtfyTopic(existingTopic)
          setUserEditedNtfyTopic(true) // Mark as edited so it doesn't auto-update
        }

        // Set verification status for existing providers
        if (newEnabled['twilio']) {
          smsVerification.setVerified(true)
        }
        if (newEnabled['email']) {
          emailVerification.setVerified(true)
        }
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
    setIsDeleteModalOpen(false)
    resetOriginalState()
    // Reset form values
    setName("")
    setNtfyTopic("")
    setUserEditedNtfyTopic(false)
    setEnabledProviders({})
    setProviderValues({})
    // Reset verification hooks and wizard
    smsVerification.reset()
    emailVerification.reset()
    wizard.reset()
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

    // Check if unchanged methods in edit mode (already verified when first added)
    const emailUnchangedInEditMode = isEditMode && !emailAddressChanged && originalState.emailEnabled
    const smsUnchangedInEditMode = isEditMode && !phoneNumberChanged && originalState.smsEnabled

    // SMS is ready if: not enabled, OR verified, OR unchanged in edit mode
    const smsReady = !hasSms || smsVerification.isVerified || smsUnchangedInEditMode
    // Email is ready if: not enabled, OR verified, OR unchanged in edit mode
    const emailReady = !hasEmail || emailVerification.isVerified || emailUnchangedInEditMode

    try {
      // If verification requirements are met
      if (smsReady && emailReady) {
        const notificationMethods: { provider_type: 'sms' | 'ntfy' | 'email'; notification_target: string }[] = []

        if (hasNtfy) {
          notificationMethods.push({ provider_type: 'ntfy', notification_target: ntfyTopic.trim() })
        }

        // Include email if verified OR if unchanged in edit mode
        if (hasEmail && (emailVerification.isVerified || emailUnchangedInEditMode)) {
          notificationMethods.push({
            provider_type: 'email',
            notification_target: emailVerification.verificationAddress || providerValues['email'].trim()
          })
        }

        // Include SMS if verified OR if unchanged in edit mode
        if (hasSms && (smsVerification.isVerified || smsUnchangedInEditMode)) {
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
      if (err instanceof ApiError) {
        // Use error codes for specific verification issues
        if (err.errorCode === 'no_pending_verification') {
          setError(t('verification.expiredRequest'))
          smsVerification.reset()
          emailVerification.reset()
        } else {
          setError(getTranslatedApiError(err, tApiErrors))
        }
      } else {
        setError(t('form.saveFailed'))
      }
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleDeleteContact = async () => {
    if (!editContact) return

    try {
      await api.deleteContact(walletChecksum, editContact.id)
      handleClose()
      if (onContactSaved) {
        onContactSaved()
      }
    } catch (err) {
      setIsDeleteModalOpen(false)
      if (err instanceof ApiError) {
        setError(getTranslatedApiError(err, tApiErrors))
      } else {
        setError(t('form.saveFailed'))
      }
    }
  }

  // Resend handlers - delegate to hooks
  const handleResendSmsCode = () => {
    smsVerification.resendCode()
  }

  const handleResendEmailCode = () => {
    emailVerification.resendCode()
  }

  // Shared form sections extracted for reuse in both layouts
  const nameSection = (
    <div>
      <Label htmlFor="contact-name">{t('add.nameLabel')}</Label>
      <Input
        id="contact-name"
        value={name}
        onChange={(e) => {
          const newName = e.target.value
          setName(newName)
          if (enabledProviders['ntfy'] && !userEditedNtfyTopic) {
            setNtfyTopic(generateDefaultNtfyTopic(newName, walletChecksum))
          }
        }}
        ref={nameInputRef}
        placeholder={t('add.namePlaceholder')}
        disabled={isSubmitting}
        enterKeyHint={isMobile ? "next" : undefined}
      />
    </div>
  )

  const methodSection = (
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
                      if (originalState.phoneNumber !== null && newPhoneNumber !== originalState.phoneNumber) {
                        smsVerification.resetForPhoneChange(newPhoneNumber)
                      } else if (originalState.phoneNumber !== null && newPhoneNumber === originalState.phoneNumber) {
                        smsVerification.revertToOriginal()
                      }
                    }}
                    phonePlaceholder={phonePlaceholder}
                    phoneError={smsVerification.phoneError}
                    disabled={isSubmitting}
                    verificationRequired={isMobile ? false : smsVerificationRequired}
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
                      if (originalState.emailAddress !== null && newEmailAddress !== originalState.emailAddress) {
                        emailVerification.resetForEmailChange(newEmailAddress)
                      } else if (originalState.emailAddress !== null && newEmailAddress === originalState.emailAddress) {
                        emailVerification.revertToOriginal()
                      }
                    }}
                    emailPlaceholder={tCommon('emailPlaceholder')}
                    emailError={emailVerification.emailError}
                    disabled={isSubmitting}
                    verificationRequired={isMobile ? false : emailVerificationRequired}
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
                    ntfyServerUrl={ntfyServerUrl}
                  />
                )}
              </div>
            </label>
          </div>
        ))}
      </div>
    </div>
  )

  const verificationSection = (
    <div className="space-y-4">
      {smsVerificationRequired && (
        <SmsProviderFields
          phoneNumber={providerValues['twilio'] || ''}
          onPhoneNumberChange={() => {}}
          phonePlaceholder={phonePlaceholder}
          phoneError={smsVerification.phoneError}
          disabled={isSubmitting}
          verificationRequired={true}
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
          hidePhoneInput
        />
      )}
      {emailVerificationRequired && (
        <EmailProviderFields
          emailAddress={providerValues['email'] || ''}
          onEmailAddressChange={() => {}}
          emailPlaceholder={tCommon('emailPlaceholder')}
          emailError={emailVerification.emailError}
          disabled={isSubmitting}
          verificationRequired={true}
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
          hideEmailInput
        />
      )}
    </div>
  )

  const errorDisplay = error && (
    <div role="alert" className="text-sm text-red-600 bg-red-50 p-3 rounded-md border border-red-200">
      {error}
    </div>
  )

  const submitButton = (
    <Button onClick={handleSubmit} disabled={isSubmitting || (isEditMode && !hasChanges)}>
      {isSubmitting ? t('add.submitting') : (isEditMode ? t('edit.submit') : t('add.submit'))}
    </Button>
  )

  const deleteButton = isEditMode && (
    <Button
      variant="destructive"
      onClick={() => setIsDeleteModalOpen(true)}
      disabled={isSubmitting}
    >
      {tCommon('delete')}
    </Button>
  )

  return (<>
    <ResponsiveModal open={isOpen} onOpenChange={handleClose}>
      <ResponsiveModalContent
        className="sm:max-w-md"
        onOpenAutoFocus={(e) => {
          if (isEditMode) {
            e.preventDefault()
          } else {
            nameInputRef.current?.focus()
          }
        }}
      >
        <ResponsiveModalHeader>
          {isMobile && (
            <StepIndicator currentStep={wizard.currentStep} totalSteps={wizard.totalSteps} />
          )}
          <ResponsiveModalTitle>
            {isMobile
              ? (wizard.currentStep === 0
                  ? t('wizard.nameStep')
                  : wizard.currentStep === 1
                    ? t('wizard.methodStep')
                    : t('wizard.verificationStep'))
              : (isEditMode ? t('edit.title') : t('add.title'))}
          </ResponsiveModalTitle>
          {!isMobile && (
            <ResponsiveModalDescription>
              {isEditMode ? t('edit.description') : t('add.description')}
            </ResponsiveModalDescription>
          )}
        </ResponsiveModalHeader>

        {errorDisplay}

        {isMobile ? (
          // Mobile wizard layout
          <div className="space-y-4">
            {wizard.currentStep === 0 && nameSection}
            {wizard.currentStep === 1 && methodSection}
            {wizard.currentStep === 2 && verificationSection}
          </div>
        ) : (
          // Desktop single-page layout
          <div className="space-y-4">
            {nameSection}
            {methodSection}
          </div>
        )}

        <ResponsiveModalFooter className={isEditMode && !isMobile ? "sm:justify-between" : ""}>
          {isMobile ? (
            // Mobile wizard footer
            <div className="flex w-full items-center justify-between">
              <div className="flex items-center gap-2">
                {wizard.canGoBack && (
                  <Button variant="ghost" size="sm" onClick={wizard.goBack}>
                    <ChevronLeft className="h-4 w-4 mr-1" />
                    {t('wizard.back')}
                  </Button>
                )}
                {isEditMode && wizard.currentStep === 0 && deleteButton}
              </div>
              <div>
                {wizard.isLastStep ? (
                  submitButton
                ) : (
                  <Button onClick={wizard.goNext} disabled={!wizard.canGoNext}>
                    {t('wizard.next')}
                  </Button>
                )}
              </div>
            </div>
          ) : (
            // Desktop footer
            <>
              {deleteButton}
              {submitButton}
            </>
          )}
        </ResponsiveModalFooter>
      </ResponsiveModalContent>
    </ResponsiveModal>

    <DeleteContactModal
      contact={editContact || null}
      isOpen={isDeleteModalOpen}
      onClose={() => setIsDeleteModalOpen(false)}
      onConfirmDelete={handleDeleteContact}
    />
  </>
  )
}