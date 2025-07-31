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
  const [newContactAddress, setNewContactAddress] = useState("")
  const [autoGenerateTopic, setAutoGenerateTopic] = useState(true)
  const [newContactLanguage, setNewContactLanguage] = useState<'en' | 'no'>('en')
  const [newContactError, setNewContactError] = useState<string | null>(null)

  // Extract checksum from Bitcoin descriptor
  const extractChecksum = (descriptor: string): string => {
    const hashIndex = descriptor.lastIndexOf('#')
    if (hashIndex !== -1) {
      return descriptor.substring(hashIndex + 1)
    }
    return 'unknown'
  }

  // Generate ntfy topic from contact name, language, and wallet checksum
  const generateNtfyTopic = useCallback((contactName: string, language: string, descriptor: string): string => {
    const sanitizedName = contactName.toLowerCase()
      .replace(/[^a-z0-9]/g, '-')
      .replace(/-+/g, '-')
      .replace(/^-|-$/g, '')
    const checksum = extractChecksum(descriptor)
    return `${sanitizedName}-${language}-${checksum}`.substring(0, 64) // Max 64 chars
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

  // Auto-generate topic when contact name changes
  useEffect(() => {
    if (autoGenerateTopic && newContactName && wallet) {
      const generatedTopic = generateNtfyTopic(newContactName, newContactLanguage, wallet.descriptor)
      setNewContactAddress(generatedTopic)
    }
  }, [newContactName, newContactLanguage, autoGenerateTopic, wallet, generateNtfyTopic])

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
    if (!wallet || !newContactName.trim() || !newContactAddress.trim()) return

    setIsCreatingContact(true)
    setNewContactError(null)

    try {

      const baseUrl = getApiBaseUrl()
      const response = await fetch(`${baseUrl}/api/wallets/${wallet.id}/contacts`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          name: newContactName.trim(),
          contact_address: newContactAddress.trim(),
          language: newContactLanguage,
        }),
      })

      if (!response.ok) {
        const errorData = await response.json().catch(() => null)
        throw new Error(errorData?.error || `HTTP error! status: ${response.status}`)
      }

      setNewContactName('')
      setNewContactAddress('')
      setNewContactLanguage('en')
      setAutoGenerateTopic(true)
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
      setNewContactAddress('')
      setNewContactLanguage('en')
      setAutoGenerateTopic(true)
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
                            <p className="text-xs text-muted-foreground">{contact.contact_address}</p>
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
                            <div className="flex items-center justify-between mb-1">
                              <Label htmlFor="contact-address" className="text-xs">ntfy Topic</Label>
                              <label className="flex items-center gap-1 text-xs text-muted-foreground">
                                <input
                                  type="checkbox"
                                  checked={autoGenerateTopic}
                                  onChange={(e) => setAutoGenerateTopic(e.target.checked)}
                                  className="h-3 w-3"
                                />
                                Auto-generate
                              </label>
                            </div>
                            <Input
                              id="contact-address"
                              value={newContactAddress}
                              onChange={(e) => {
                                setNewContactAddress(e.target.value)
                                setAutoGenerateTopic(false)
                              }}
                              placeholder={autoGenerateTopic ? "Will be auto-generated" : "my-bitcoin-alerts"}
                              disabled={isCreatingContact || (autoGenerateTopic && !newContactName)}
                              className={autoGenerateTopic ? "bg-muted" : ""}
                            />
                          </div>
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
                        <Button
                          onClick={handleCreateContact}
                          disabled={isCreatingContact || !newContactName.trim() || !newContactAddress.trim()}
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