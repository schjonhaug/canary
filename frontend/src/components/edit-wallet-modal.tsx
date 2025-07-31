"use client"

import { useState, useEffect, useCallback } from "react"
import { Edit, Trash2, Plus, X, Bell, Users } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Wallet, Contact } from "../types"
import { getApiBaseUrl } from "../lib/utils"
import { api, ProviderInfo } from "../lib/api"

const LANGUAGES = [
  { value: 'en', label: 'English' },
  { value: 'no', label: 'Norwegian' },
] as const

interface EditWalletModalProps {
  wallet: Wallet | null
  isOpen: boolean
  onClose: () => void
  onDeleteWallet: (wallet: Wallet) => void
}

export function EditWalletModal({
  wallet,
  isOpen,
  onClose,
  onDeleteWallet,
}: EditWalletModalProps) {
  const [walletName, setWalletName] = useState("")
  const [isUpdating, setIsUpdating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [walletContacts, setWalletContacts] = useState<Contact[]>([])
  const [contactsLoading, setContactsLoading] = useState(false)
  const [isCreatingContact, setIsCreatingContact] = useState(false)
  const [newContactName, setNewContactName] = useState("")
  const [newContactLanguage, setNewContactLanguage] = useState<'en' | 'no'>('en')
  const [newContactError, setNewContactError] = useState<string | null>(null)
  const [providers, setProviders] = useState<ProviderInfo[]>([])
  const [providersLoading, setProvidersLoading] = useState(false)
  const [enabledProviders, setEnabledProviders] = useState<Record<string, boolean>>({})
  const [providerValues, setProviderValues] = useState<Record<string, string>>({})

  // Fetch available providers
  const fetchProviders = useCallback(async () => {
    try {
      setProvidersLoading(true)
      const response = await api.getProviders()
      setProviders(response.providers)
    } catch (err) {
      console.error('Failed to fetch providers:', err)
    } finally {
      setProvidersLoading(false)
    }
  }, [])




  const fetchWalletContacts = useCallback(async (walletId: number) => {
    try {
      const baseUrl = getApiBaseUrl()
      const response = await fetch(`${baseUrl}/api/wallets/${walletId}/contacts`)
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`)
      const data = await response.json()
      setWalletContacts(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch wallet contacts")
    }
  }, [])

  useEffect(() => {
    if (wallet) {
      setWalletName(wallet.name)
    }
  }, [wallet])

  useEffect(() => {
    if (isOpen && wallet) {
      setContactsLoading(true)
      fetchWalletContacts(wallet.id)
        .finally(() => setContactsLoading(false))
    }
  }, [isOpen, wallet, fetchWalletContacts])

  // Fetch providers when modal opens
  useEffect(() => {
    if (isOpen) {
      fetchProviders()
    }
  }, [isOpen, fetchProviders])

  const handleSave = async () => {
    if (!wallet || !walletName.trim()) return

    setIsUpdating(true)
    setError(null)

    try {
      const baseUrl = getApiBaseUrl()
      const response = await fetch(`${baseUrl}/api/wallets/${wallet.id}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          name: walletName.trim(),
        }),
      })

      if (!response.ok) {
        if (response.status === 404) {
          throw new Error('Wallet not found')
        }
        throw new Error(`Update failed: ${response.status}`)
      }

    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update wallet")
    } finally {
      setIsUpdating(false)
    }
  }

  const handleDelete = () => {
    if (wallet) {
      onDeleteWallet(wallet)
    }
  }

  const handleCreateContact = async () => {
    if (!wallet || !newContactName.trim()) return

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
      setNewContactError("Please enable at least one notification method")
      return
    }

    setIsCreatingContact(true)
    setNewContactError(null)

    try {
      await api.createContact(
        wallet.id,
        newContactName.trim(),
        newContactLanguage,
        notificationMethods
      )

      // Reset form
      setNewContactName('')
      setNewContactLanguage('en')
      setEnabledProviders({})
      setProviderValues({})
      await fetchWalletContacts(wallet.id)
    } catch (err) {
      setNewContactError(err instanceof Error ? err.message : "Failed to create contact")
    } finally {
      setIsCreatingContact(false)
    }
  }

  const handleDeleteContact = async (contactId: number) => {
    if (!wallet) return

    try {
      const baseUrl = getApiBaseUrl()
      const response = await fetch(`${baseUrl}/api/wallets/${wallet.id}/contacts/${contactId}`, {
        method: 'DELETE',
      })

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`)
      }

      await fetchWalletContacts(wallet.id)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete contact")
    }
  }

  const handleClose = () => {
    if (!isUpdating && !isCreatingContact) {
      setError(null)
      setNewContactError(null)
      setWalletName(wallet?.name || "")
      setWalletContacts([])
      setNewContactName('')
      setNewContactLanguage('en')
      setEnabledProviders({})
      setProviderValues({})
      onClose()
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[600px] max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Edit className="h-5 w-5" />
            Edit Wallet
          </DialogTitle>
          <DialogDescription>
            Edit the wallet name and manage notification contacts.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          <div>
            <Label htmlFor="wallet-name">Wallet Name</Label>
            <div className="flex gap-2">
              <Input
                id="wallet-name"
                type="text"
                value={walletName}
                onChange={(e) => setWalletName(e.target.value)}
                placeholder="Enter wallet name"
                disabled={isUpdating}
                className="flex-1"
              />
              <Button
                onClick={handleSave}
                disabled={isUpdating || !walletName.trim()}
                size="sm"
              >
                {isUpdating ? "Updating..." : "Save Name"}
              </Button>
            </div>
          </div>

          {/* Contact Management Section */}
          <div>
            <div className="flex items-center gap-2 mb-4">
              <Users className="h-4 w-4" />
              <h3 className="text-lg font-semibold">Notifications</h3>
            </div>
            
            {contactsLoading ? (
              <div className="text-sm text-muted-foreground">Loading contacts...</div>
            ) : (
              <Card>
                <CardHeader>
                  <CardTitle className="text-base flex items-center justify-between">
                    <span>Active Contacts</span>
                    <Badge variant="secondary">
                      {walletContacts.length} contact{walletContacts.length !== 1 ? 's' : ''}
                    </Badge>
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    {walletContacts.map((contact) => (
                      <div key={contact.id} data-testid="contact-item" className="flex items-center justify-between p-3 bg-green-50 rounded-lg">
                        <div className="flex items-center gap-3">
                          <Bell className="h-4 w-4 text-green-600" />
                          <div>
                            <div className="flex items-center gap-2">
                              <p className="text-sm font-medium">{contact.name}</p>
                              <Badge variant="outline" className="text-xs">
                                {contact.language === 'no' ? 'Norwegian' : 'English'}
                              </Badge>
                            </div>
                            <div className="text-xs text-muted-foreground space-y-1">
                              {contact.notification_methods?.map((method) => (
                                <div key={method.id} className="flex items-center gap-1">
                                  {method.provider_type === 'sms' ? '📱' : '🔔'}
                                  <span>{method.display_target || method.notification_target}</span>
                                </div>
                              ))}
                            </div>
                          </div>
                        </div>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDeleteContact(contact.id)}
                          className="text-red-600 hover:text-red-700"
                          disabled={isUpdating}
                        >
                          <X className="h-4 w-4" />
                        </Button>
                      </div>
                    ))}


                    {/* Create New Contact Section */}
                    <div className="border-t pt-4">
                      <p className="text-sm font-medium mb-3">Create New Contact</p>
                      <div className="space-y-3">
                        {/* Name and Language */}
                        <div className="grid grid-cols-2 gap-3">
                          <div>
                            <Label htmlFor="contact-name" className="text-xs">Name</Label>
                            <Input
                              id="contact-name"
                              value={newContactName}
                              onChange={(e) => setNewContactName(e.target.value)}
                              placeholder="Contact name"
                              disabled={isCreatingContact}
                            />
                          </div>
                          <div>
                            <Label htmlFor="contact-language" className="text-xs">Language</Label>
                            <select
                              id="contact-language"
                              value={newContactLanguage}
                              onChange={(e) => setNewContactLanguage(e.target.value as 'en' | 'no')}
                              disabled={isCreatingContact}
                              className="w-full px-3 py-2 text-sm border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                            >
                              {LANGUAGES.map((lang) => (
                                <option key={lang.value} value={lang.value}>
                                  {lang.label}
                                </option>
                              ))}
                            </select>
                          </div>
                        </div>

                        {/* Notification Methods */}
                        <div className="space-y-2">
                          <Label className="text-xs">Notification Methods</Label>
                          {providersLoading ? (
                            <p className="text-xs text-muted-foreground">Loading providers...</p>
                          ) : (
                            <div className="space-y-2">
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
                                      disabled={isCreatingContact}
                                      className="mt-1"
                                    />
                                    <div className="flex-1">
                                      <div className="flex items-center gap-2">
                                        {provider.name === 'twilio' ? '📱' : '🔔'}
                                        <span className="text-sm font-medium">{provider.display_name}</span>
                                      </div>
                                      {enabledProviders[provider.name] && provider.name === 'twilio' && (
                                        <div className="mt-2">
                                          <Input
                                            value={providerValues[provider.name] || ''}
                                            onChange={(e) => {
                                              setProviderValues(prev => ({
                                                ...prev,
                                                [provider.name]: e.target.value
                                              }))
                                            }}
                                            placeholder="+1234567890"
                                            disabled={isCreatingContact}
                                            className="text-sm"
                                          />
                                          <p className="text-xs text-muted-foreground mt-1">
                                            Include country code (e.g., +1 for US, +47 for Norway)
                                          </p>
                                        </div>
                                      )}
                                      {enabledProviders[provider.name] && provider.name === 'ntfy' && (
                                        <p className="text-xs text-muted-foreground mt-2">
                                          Topic will be auto-generated based on contact name
                                        </p>
                                      )}
                                    </div>
                                  </label>
                                </div>
                              ))}
                            </div>
                          )}
                        </div>

                        <Button
                          onClick={handleCreateContact}
                          disabled={isCreatingContact || !newContactName.trim()}
                          className="w-full"
                          size="sm"
                        >
                          <Plus className="h-4 w-4 mr-2" />
                          {isCreatingContact ? "Creating..." : "Create Contact"}
                        </Button>
                        {newContactError && (
                          <div className="p-2 bg-red-50 border border-red-200 rounded text-xs text-red-700">
                            {newContactError}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>
            )}
          </div>

          {error && (
            <div className="p-3 bg-red-50 border border-red-200 rounded-lg">
              <p className="text-sm text-red-700">{error}</p>
            </div>
          )}
        </div>

        <DialogFooter className="flex justify-start">
          <Button
            variant="destructive"
            onClick={handleDelete}
            disabled={isUpdating}
            className="flex items-center gap-2"
          >
            <Trash2 className="h-4 w-4" />
            Delete Wallet
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}