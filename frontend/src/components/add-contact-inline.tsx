"use client"

import { useState, useCallback, useEffect } from "react"
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
  }

  const handleCreate = async () => {
    if (!name.trim()) {
      setError("Contact name is required")
      return
    }

    // Build notification methods array
    const notificationMethods: Array<{ provider_type: 'sms' | 'ntfy', notification_target: string }> = []

    for (const [providerName, enabled] of Object.entries(enabledProviders)) {
      if (enabled) {
        if (providerName === 'ntfy') {
          // For ntfy, send empty string - backend will auto-generate
          notificationMethods.push({ provider_type: 'ntfy', notification_target: '' })
        } else if (providerName === 'twilio' && providerValues[providerName]?.trim()) {
          // For SMS, require phone number
          notificationMethods.push({ provider_type: 'sms', notification_target: providerValues[providerName].trim() })
        }
      }
    }

    if (notificationMethods.length === 0) {
      setError("Please enable at least one notification method")
      return
    }

    setIsCreating(true)
    setError(null)

    try {
      await api.createContact(
        walletChecksum,
        name.trim(),
        language,
        notificationMethods
      )

      // Reset form and collapse
      handleCancel()
      if (onContactAdded) {
        onContactAdded()
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create contact")
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

      <div className="flex gap-2">
        <Button
          size="sm"
          onClick={handleCreate}
          disabled={isCreating || !name.trim()}
          className="flex-1 h-7 text-xs"
        >
          {isCreating ? "Creating..." : "Create Contact"}
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
    </div>
  )
}