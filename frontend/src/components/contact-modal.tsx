"use client"

import { useState, useCallback, useEffect, useRef } from "react"
import { 
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Bell, Smartphone, Mail } from "lucide-react"
import { api, ProviderInfo } from "../lib/api"
import { Contact } from "../types"

const LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'no', label: 'Norwegian' },
] as const

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
  const [name, setName] = useState("")
  const [language, setLanguage] = useState<'en' | 'no'>('en')
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [enabledProviders, setEnabledProviders] = useState<Record<string, boolean>>({})
  const [providerValues, setProviderValues] = useState<Record<string, string>>({})
  const [isSubmitting, setIsSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [smsVerificationSent, setSmsVerificationSent] = useState(false)
  const [smsVerificationCode, setSmsVerificationCode] = useState("")
  const [smsVerificationPhone, setSmsVerificationPhone] = useState<string | null>(null)
  const [timeRemaining, setTimeRemaining] = useState<number>(0)
  const [isSendingVerification, setIsSendingVerification] = useState(false)
  const [isVerifyingCode, setIsVerifyingCode] = useState(false)
  const [smsVerified, setSmsVerified] = useState(false)
  const timerRef = useRef<NodeJS.Timeout | null>(null)

  const isEditMode = !!editContact

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
          setError("Verification code expired. Please try again.")
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
      if (editContact) {
        // Populate form with existing contact data
        setName(editContact.name)
        setLanguage(editContact.language)
        
        // Set up providers based on existing notification methods
        const newEnabledProviders: Record<string, boolean> = {}
        const newProviderValues: Record<string, string> = {}
        
        editContact.notification_methods.forEach(method => {
          if (method.provider_type === 'sms') {
            newEnabledProviders['twilio'] = true
            newProviderValues['twilio'] = method.display_target || method.notification_target
          } else if (method.provider_type === 'ntfy') {
            newEnabledProviders['ntfy'] = true
          } else if (method.provider_type === 'email') {
            newEnabledProviders['email'] = true
            newProviderValues['email'] = method.display_target || method.notification_target
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
      
      setError(null)
      setSmsVerificationSent(false)
      setSmsVerificationCode("")
      setSmsVerificationPhone(null)
      setTimeRemaining(0)
      setSmsVerified(false)
      
      if (timerRef.current) {
        clearInterval(timerRef.current)
      }

      if (providers.length === 0) {
        fetchProviders()
      }
    }
  }, [isOpen, editContact, providers.length, fetchProviders])

  const handleClose = () => {
    setError(null)
    setSmsVerificationSent(false)
    setSmsVerificationCode("")
    setSmsVerificationPhone(null)
    setTimeRemaining(0)
    setSmsVerified(false)
    if (timerRef.current) {
      clearInterval(timerRef.current)
    }
    onClose()
  }

  const handleSendSmsVerification = async () => {
    const phoneNumber = providerValues['twilio']?.trim()
    if (!phoneNumber || !name.trim()) {
      setError("Contact name and phone number are required for SMS verification")
      return
    }

    setIsSendingVerification(true)
    setError(null)

    try {
      await api.sendContactVerification(
        walletChecksum,
        name.trim(),
        language,
        phoneNumber
      )
      
      setSmsVerificationPhone(phoneNumber)
      setSmsVerificationSent(true)
      setSmsVerificationCode("")
      setError(null)
      startTimer()
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to send verification code")
    } finally {
      setIsSendingVerification(false)
    }
  }

  const handleVerifySmsCode = async () => {
    if (!smsVerificationCode.trim() || !smsVerificationPhone) {
      setError("Please enter the verification code")
      return
    }

    setIsVerifyingCode(true)
    setError(null)

    try {
      // Use the new verify-phone-only endpoint that doesn't create contacts
      const result = await api.verifyPhoneOnly(
        walletChecksum,
        smsVerificationPhone,
        smsVerificationCode.trim()
      )
      
      if (result.valid) {
        setSmsVerified(true)
        setError(null)
        
        // Clear the timer since verification is complete
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
        setTimeRemaining(0)
      } else {
        setError(result.message || "Invalid verification code")
        setSmsVerificationCode("")
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Invalid verification code"
      
      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setError("Verification code expired. Please request a new code.")
        setSmsVerificationSent(false)
        setSmsVerified(false)
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setError("Invalid verification code. Please try again.")
        setSmsVerificationCode("") // Clear the input
      } else {
        setError(errorMessage)
      }
    } finally {
      setIsVerifyingCode(false)
    }
  }

  const handleSubmit = async () => {
    if (!name.trim()) {
      setError("Contact name is required")
      return
    }

    // Check what's enabled
    const hasNtfy = enabledProviders['ntfy'] || false
    const hasSms = enabledProviders['twilio'] && providerValues['twilio']?.trim()
    const hasEmail = enabledProviders['email'] && providerValues['email']?.trim()

    if (!hasNtfy && !hasSms && !hasEmail) {
      setError("Please enable at least one notification method")
      return
    }

    // Check if SMS is enabled but not verified yet
    if (hasSms && !smsVerified) {
      setError("Please verify the SMS code before saving the contact")
      return
    }

    setIsSubmitting(true)
    setError(null)

    try {
      // For edit mode, we need to delete and recreate since there's no update endpoint
      if (isEditMode && editContact) {
        await api.deleteContact(walletChecksum, editContact.id)
      }

      // If only ntfy is enabled, or email is enabled without SMS, create directly
      if ((hasNtfy && !hasSms && !hasEmail) || (hasEmail && !hasSms)) {
        const notificationMethods = []
        
        if (hasNtfy) {
          notificationMethods.push({ provider_type: 'ntfy', notification_target: '' })
        }
        
        if (hasEmail) {
          notificationMethods.push({ 
            provider_type: 'email', 
            notification_target: providerValues['email'].trim() 
          })
        }
        
        await api.createContact(
          walletChecksum,
          name.trim(),
          language,
          notificationMethods
        )

        handleClose()
        if (onContactSaved) {
          onContactSaved()
        }
      } 
      // If SMS is enabled and verified
      else if (hasSms && smsVerified) {
        // With the new verify-phone-only endpoint, no contact was created during verification
        // We need to create the contact with ALL methods including SMS
        const allMethods = []
        
        // Add the verified SMS method
        allMethods.push({ 
          provider_type: 'sms', 
          notification_target: smsVerificationPhone! 
        })
        
        // Add other methods if enabled
        if (enabledProviders['ntfy']) {
          allMethods.push({ provider_type: 'ntfy', notification_target: '' })
        }
        
        if (enabledProviders['email'] && providerValues['email']?.trim()) {
          allMethods.push({ 
            provider_type: 'email', 
            notification_target: providerValues['email'].trim() 
          })
        }
        
        // Create contact with all methods at once
        await api.createContact(
          walletChecksum,
          name.trim(),
          language,
          allMethods
        )

        handleClose()
        if (onContactSaved) {
          onContactSaved()
        }
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : `Failed to ${isEditMode ? 'update' : 'create'} contact`
      
      // Provide more specific error messages for SMS verification
      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setError("Verification code expired. Please request a new code.")
        setSmsVerificationSent(false)
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setError("Invalid verification code. Please try again.")
        setSmsVerificationCode("") // Clear the input
      } else {
        setError(errorMessage)
      }
    } finally {
      setIsSubmitting(false)
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
      setError(err instanceof Error ? err.message : "Failed to resend code")
    } finally {
      setIsSendingVerification(false)
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {isEditMode ? 'Edit Contact' : 'Add New Contact'}
          </DialogTitle>
        </DialogHeader>

        {error && (
          <div className="text-sm text-red-600 bg-red-50 p-3 rounded-md border border-red-200">
            {error}
          </div>
        )}

        <div className="space-y-4">
          <div>
            <Label htmlFor="contact-name">Name</Label>
            <Input
              id="contact-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Contact name"
              disabled={isSubmitting}
            />
          </div>

          <div>
            <Label htmlFor="contact-language">Language</Label>
            <select
              id="contact-language"
              value={language}
              onChange={(e) => setLanguage(e.target.value as 'en' | 'no')}
              disabled={isSubmitting}
              className="w-full h-10 border border-input bg-background px-3 py-2 rounded-md text-sm"
            >
              {LANGUAGES.map((lang) => (
                <option key={lang.value} value={lang.value}>
                  {lang.label}
                </option>
              ))}
            </select>
          </div>

          <div>
            <Label>Notification Methods</Label>
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
                      }}
                      disabled={isSubmitting}
                      className="mt-1"
                    />
                    <div className="flex-1">
                      <div className="flex items-center gap-2 mb-1">
                        {provider.name === 'twilio' ? (
                          <Smartphone className="h-4 w-4" />
                        ) : provider.name === 'email' ? (
                          <Mail className="h-4 w-4" />
                        ) : (
                          <Bell className="h-4 w-4" />
                        )}
                        <span className="font-medium">{provider.display_name}</span>
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
                                // Reset verification state when phone number changes
                                if (smsVerificationSent && smsVerificationPhone !== e.target.value.trim()) {
                                  setSmsVerificationSent(false)
                                  setSmsVerificationCode("")
                                  setSmsVerificationPhone(null)
                                  setSmsVerified(false)
                                  setTimeRemaining(0)
                                  if (timerRef.current) {
                                    clearInterval(timerRef.current)
                                  }
                                }
                              }}
                              placeholder="+1234567890"
                              disabled={isSubmitting || isSendingVerification}
                            />
                            <p className="text-xs text-muted-foreground mt-1">
                              Include country code (e.g., +47 for Norway)
                            </p>
                          </div>
                          
                          {/* Send Verification Button */}
                          {providerValues[provider.name]?.trim() && !smsVerificationSent && (
                            <Button
                              type="button"
                              variant="outline"
                              size="sm"
                              onClick={handleSendSmsVerification}
                              disabled={isSendingVerification || isSubmitting || !name.trim()}
                              className="w-full"
                            >
                              {isSendingVerification ? "Sending..." : "Send Verification Code"}
                            </Button>
                          )}

                          {/* OTP Input Field */}
                          {smsVerificationSent && !smsVerified && (
                            <div className="space-y-3">
                              <div>
                                <Label htmlFor="sms-verification-code">Verification Code</Label>
                                <div className="flex gap-2">
                                  <Input
                                    id="sms-verification-code"
                                    value={smsVerificationCode}
                                    onChange={(e) => setSmsVerificationCode(e.target.value)}
                                    placeholder="Enter 6-digit code"
                                    disabled={isSubmitting || isVerifyingCode}
                                    maxLength={6}
                                    className="flex-1"
                                  />
                                  <Button
                                    type="button"
                                    variant="outline"
                                    onClick={handleVerifySmsCode}
                                    disabled={!smsVerificationCode.trim() || isVerifyingCode || isSubmitting}
                                  >
                                    {isVerifyingCode ? "Verifying..." : "Verify"}
                                  </Button>
                                </div>
                              </div>
                              <div className="flex justify-between items-center text-xs text-muted-foreground">
                                <span>
                                  Code sent to {smsVerificationPhone}
                                  {timeRemaining > 0 && (
                                    <span className="block">Expires in {formatTime(timeRemaining)}</span>
                                  )}
                                </span>
                                <button
                                  type="button"
                                  onClick={handleResendCode}
                                  disabled={isSendingVerification || timeRemaining > 540} // Allow resend after 1 minute
                                  className="text-blue-600 hover:text-blue-800 disabled:text-gray-400 underline"
                                >
                                  Resend
                                </button>
                              </div>
                            </div>
                          )}

                          {/* Verification Success */}
                          {smsVerified && (
                            <div className="flex items-center gap-2 text-green-600 text-sm">
                              <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
                              </svg>
                              SMS verified successfully
                            </div>
                          )}
                        </div>
                      )}
                      {enabledProviders[provider.name] && provider.name === 'email' && (
                        <div className="mt-2">
                          <Input
                            value={providerValues[provider.name] || ''}
                            onChange={(e) => {
                              setProviderValues(prev => ({
                                ...prev,
                                [provider.name]: e.target.value
                              }))
                            }}
                            placeholder="user@example.com"
                            disabled={isSubmitting}
                            type="email"
                          />
                          <p className="text-xs text-muted-foreground mt-1">
                            Enter valid email address
                          </p>
                        </div>
                      )}
                      {enabledProviders[provider.name] && provider.name === 'ntfy' && (
                        <p className="text-xs text-muted-foreground mt-1">
                          Topic will be auto-generated based on contact name
                        </p>
                      )}
                    </div>
                  </label>
                </div>
              ))}
            </div>
          </div>

        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={isSubmitting}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={isSubmitting || !name.trim()}>
            {isSubmitting ? "Processing..." : (isEditMode ? "Update Contact" : "Create Contact")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}