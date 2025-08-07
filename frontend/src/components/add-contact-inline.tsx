"use client"

import { useState, useCallback, useEffect, useRef } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Plus, X, Bell, Smartphone } from "lucide-react"
import { api, ProviderInfo } from "../lib/api"

const LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'no', label: 'Norwegian' },
] as const

interface AddContactInlineProps {
  walletChecksum: string
  onContactAdded?: () => void
}

export function AddContactInline({ walletChecksum, onContactAdded }: AddContactInlineProps) {
  const [isExpanded, setIsExpanded] = useState(false)
  const [name, setName] = useState("")
  const [language, setLanguage] = useState<'en' | 'no'>('en')
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [enabledProviders, setEnabledProviders] = useState<Record<string, boolean>>({})
  const [providerValues, setProviderValues] = useState<Record<string, string>>({})
  const [isCreating, setIsCreating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [verificationStep, setVerificationStep] = useState<'input' | 'verify'>('input')
  const [verificationCode, setVerificationCode] = useState("")
  const [pendingPhoneNumber, setPendingPhoneNumber] = useState<string | null>(null)
  const [timeRemaining, setTimeRemaining] = useState<number>(0)
  const timerRef = useRef<NodeJS.Timeout | null>(null)

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
          setVerificationStep('input')
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

  useEffect(() => {
    if (isExpanded && providers.length === 0) {
      fetchProviders()
    }
  }, [isExpanded, providers.length, fetchProviders])

  const handleCancel = () => {
    setIsExpanded(false)
    setName("")
    setLanguage('en')
    setEnabledProviders({})
    setProviderValues({})
    setError(null)
    setVerificationStep('input')
    setVerificationCode("")
    setPendingPhoneNumber(null)
    setTimeRemaining(0)
    if (timerRef.current) {
      clearInterval(timerRef.current)
    }
  }

  const handleCreate = async () => {
    if (!name.trim()) {
      setError("Contact name is required")
      return
    }

    // Check what's enabled
    const hasNtfy = enabledProviders['ntfy'] || false
    const hasSms = enabledProviders['twilio'] && providerValues['twilio']?.trim()

    if (!hasNtfy && !hasSms) {
      setError("Please enable at least one notification method")
      return
    }

    setIsCreating(true)
    setError(null)

    try {
      // If only ntfy is enabled, create directly
      if (hasNtfy && !hasSms) {
        await api.createContact(
          walletChecksum,
          name.trim(),
          language,
          [{ provider_type: 'ntfy', notification_target: '' }]
        )

        // Reset form and collapse
        handleCancel()
        if (onContactAdded) {
          onContactAdded()
        }
      } 
      // If SMS is enabled (with or without ntfy), start verification
      else if (hasSms) {
        const phoneNumber = providerValues['twilio'].trim()
        setPendingPhoneNumber(phoneNumber)
        
        // Send verification code
        await api.sendContactVerification(
          walletChecksum,
          name.trim(),
          language,
          phoneNumber
        )
        
        setVerificationStep('verify')
        setError(null)
        startTimer()
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create contact")
    } finally {
      setIsCreating(false)
    }
  }

  const handleVerify = async () => {
    if (!verificationCode.trim() || !pendingPhoneNumber) {
      setError("Verification code is required")
      return
    }

    setIsCreating(true)
    setError(null)

    try {
      await api.verifyContact(
        walletChecksum,
        pendingPhoneNumber,
        verificationCode.trim()
      )

      // If ntfy was also enabled, create that too
      if (enabledProviders['ntfy']) {
        try {
          await api.createContact(
            walletChecksum,
            name.trim(),
            language,
            [{ provider_type: 'ntfy', notification_target: '' }]
          )
        } catch {
          // Ignore error if contact already exists with ntfy
          console.log('Note: ntfy creation failed, likely already exists with SMS')
        }
      }

      // Reset form and collapse
      handleCancel()
      if (onContactAdded) {
        onContactAdded()
      }
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : "Invalid verification code"
      
      // Provide more specific error messages
      if (errorMessage.includes("verification not found") || errorMessage.includes("expired")) {
        setError("Verification code expired. Please request a new code.")
        setVerificationStep('input')
        if (timerRef.current) {
          clearInterval(timerRef.current)
        }
      } else if (errorMessage.includes("Invalid verification code") || errorMessage.includes("wrong") || errorMessage.includes("incorrect")) {
        setError("Invalid verification code. Please try again.")
        setVerificationCode("") // Clear the input
      } else {
        setError(errorMessage)
      }
    } finally {
      setIsCreating(false)
    }
  }

  const handleResendCode = async () => {
    if (!pendingPhoneNumber) return
    
    setIsCreating(true)
    setError(null)
    
    try {
      await api.sendContactVerification(
        walletChecksum,
        name.trim(),
        language,
        pendingPhoneNumber
      )
      
      setVerificationCode("")
      startTimer()
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to resend code")
    } finally {
      setIsCreating(false)
    }
  }

  if (!isExpanded) {
    return (
      <Button
        size="sm"
        variant="outline"
        onClick={() => setIsExpanded(true)}
        className="w-full mt-2"
      >
        <Plus size={14} className="mr-2" />
        Add Contact
      </Button>
    )
  }

  return (
    <div className="mt-2 p-3 border rounded-md bg-muted/30 space-y-3">
      <div className="flex items-center justify-between">
        <h4 className="text-sm font-medium">Add New Contact</h4>
        <Button
          size="sm"
          variant="ghost"
          onClick={handleCancel}
          disabled={isCreating}
          className="h-6 w-6 p-0"
        >
          <X size={14} />
        </Button>
      </div>

      {error && (
        <div className="text-sm text-red-600">{error}</div>
      )}

      <div className="space-y-2">
        <div>
          <Label htmlFor="contact-name" className="text-xs">Name</Label>
          <Input
            id="contact-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Contact name"
            disabled={isCreating}
            className="h-8 text-sm"
          />
        </div>

        <div>
          <Label htmlFor="contact-language" className="text-xs">Language</Label>
          <select
            id="contact-language"
            value={language}
            onChange={(e) => setLanguage(e.target.value as 'en' | 'no')}
            disabled={isCreating}
            className="w-full h-8 text-sm border border-input bg-background px-3 py-1 rounded-md"
          >
            {LANGUAGES.map((lang) => (
              <option key={lang.value} value={lang.value}>
                {lang.label}
              </option>
            ))}
          </select>
        </div>

        <div>
          <Label className="text-xs">Notification Methods</Label>
          <div className="space-y-2 mt-1">
            {providers.map((provider) => (
              <div key={provider.name} className="p-2 border rounded text-sm">
                <label className="flex items-start gap-2">
                  <input
                    type="checkbox"
                    checked={enabledProviders[provider.name] || false}
                    onChange={(e) => {
                      setEnabledProviders(prev => ({
                        ...prev,
                        [provider.name]: e.target.checked
                      }))
                    }}
                    disabled={isCreating}
                    className="mt-1 shrink-0"
                  />
                  <div className="flex-1">
                    <div className="flex items-center gap-1">
                      {provider.name === 'twilio' ? (
                        <Smartphone className="h-3 w-3" />
                      ) : (
                        <Bell className="h-3 w-3" />
                      )}
                      <span className="text-xs font-medium">{provider.display_name}</span>
                    </div>
                    {enabledProviders[provider.name] && provider.name === 'twilio' && (
                      <div className="mt-1">
                        <Input
                          value={providerValues[provider.name] || ''}
                          onChange={(e) => {
                            setProviderValues(prev => ({
                              ...prev,
                              [provider.name]: e.target.value
                            }))
                          }}
                          placeholder="+1234567890"
                          disabled={isCreating}
                          className="h-7 text-xs"
                        />
                        <p className="text-xs text-muted-foreground mt-1">
                          Include country code (e.g., +47 for Norway)
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

      {verificationStep === 'input' ? (
        <div className="flex gap-2">
          <Button
            size="sm"
            onClick={handleCreate}
            disabled={isCreating || !name.trim()}
            className="flex-1 h-7 text-xs"
          >
            {isCreating ? "Sending..." : enabledProviders['twilio'] ? "Send Verification" : "Create Contact"}
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={handleCancel}
            disabled={isCreating}
            className="h-7 text-xs"
          >
            Cancel
          </Button>
        </div>
      ) : (
        <div className="space-y-2">
          <div>
            <Label htmlFor="verification-code" className="text-xs">Verification Code</Label>
            <Input
              id="verification-code"
              value={verificationCode}
              onChange={(e) => setVerificationCode(e.target.value)}
              placeholder="Enter 6-digit code"
              disabled={isCreating}
              className="h-8 text-sm"
              maxLength={6}
            />
            <div className="flex justify-between items-center">
              <p className="text-xs text-muted-foreground">
                We sent a verification code to {pendingPhoneNumber}
                {timeRemaining > 0 && (
                  <span className="block">Code expires in {formatTime(timeRemaining)}</span>
                )}
              </p>
              <button
                type="button"
                onClick={handleResendCode}
                disabled={isCreating || timeRemaining > 540} // Allow resend after 1 minute
                className="text-xs text-blue-600 hover:text-blue-800 disabled:text-gray-400 underline"
              >
                Resend
              </button>
            </div>
          </div>
          <div className="flex gap-2">
            <Button
              size="sm"
              onClick={handleVerify}
              disabled={isCreating || !verificationCode.trim()}
              className="flex-1 h-7 text-xs"
            >
              {isCreating ? "Verifying..." : "Verify & Create"}
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                setVerificationStep('input')
                setVerificationCode("")
                setError(null)
              }}
              disabled={isCreating}
              className="h-7 text-xs"
            >
              Back
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}