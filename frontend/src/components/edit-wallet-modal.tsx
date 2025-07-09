"use client"

import { useState, useEffect, useCallback } from "react"
import { Edit, Trash2, Plus, X, Phone, Users } from "lucide-react"
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
import { formatNumber, parsePhoneNumber } from "libphonenumber-js"

interface Wallet {
  id: number
  name: string
  descriptor: string
  wallet_filename: string
  created_at: string
  balance_total?: number
  last_activity?: string
}

interface Contact {
  id: number
  wallet_id: number
  name: string
  phone_number: string
  created_at: string
}

interface EditWalletModalProps {
  wallet: Wallet | null
  isOpen: boolean
  onClose: () => void
  onWalletUpdated: () => void
  onDeleteWallet: (wallet: Wallet) => void
}

export function EditWalletModal({
  wallet,
  isOpen,
  onClose,
  onWalletUpdated,
  onDeleteWallet,
}: EditWalletModalProps) {
  const [walletName, setWalletName] = useState("")
  const [isUpdating, setIsUpdating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [walletContacts, setWalletContacts] = useState<Contact[]>([])
  const [contactsLoading, setContactsLoading] = useState(false)
  const [isCreatingContact, setIsCreatingContact] = useState(false)
  const [newContactName, setNewContactName] = useState("")
  const [newContactPhone, setNewContactPhone] = useState("")
  const [newContactError, setNewContactError] = useState<string | null>(null)

  // Format phone number for display
  const formatPhoneForDisplay = (phoneNumber: string): string => {
    try {
      const formatted = formatNumber(phoneNumber, 'INTERNATIONAL')
      return formatted || phoneNumber
    } catch {
      return phoneNumber
    }
  }

  // Global contacts endpoint removed - contacts are now wallet-specific

  const fetchWalletContacts = useCallback(async (walletId: number) => {
    try {
      const baseUrl = process.env.NEXT_PUBLIC_API_URL || ''
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

  const handleSave = async () => {
    if (!wallet || !walletName.trim()) return

    setIsUpdating(true)
    setError(null)

    try {
      const baseUrl = process.env.NEXT_PUBLIC_API_URL || ''
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

      onWalletUpdated()
      onClose()
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
    if (!wallet || !newContactName.trim() || !newContactPhone.trim()) return

    setIsCreatingContact(true)
    setNewContactError(null)

    try {
      // Validate phone number
      const phoneNumber = parsePhoneNumber(newContactPhone, 'NO')
      if (!phoneNumber || !phoneNumber.isValid()) {
        throw new Error('Invalid phone number format. Please include country code (e.g., +4712345678)')
      }

      const baseUrl = process.env.NEXT_PUBLIC_API_URL || ''
      const response = await fetch(`${baseUrl}/api/wallets/${wallet.id}/contacts`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          name: newContactName.trim(),
          phone_number: phoneNumber.format('E.164'),
        }),
      })

      if (!response.ok) {
        const errorData = await response.json().catch(() => null)
        throw new Error(errorData?.error || `HTTP error! status: ${response.status}`)
      }

      // Clear form and refresh contacts
      setNewContactName('')
      setNewContactPhone('')
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
      const baseUrl = process.env.NEXT_PUBLIC_API_URL || ''
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

  // Helper function removed - contacts are now wallet-specific

  const handleClose = () => {
    if (!isUpdating && !isCreatingContact) {
      setError(null)
      setNewContactError(null)
      setWalletName(wallet?.name || "")
      setWalletContacts([])
      setNewContactName('')
      setNewContactPhone('')
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
            Edit the wallet name and manage SMS notification contacts.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          <div>
            <Label htmlFor="wallet-name">Wallet Name</Label>
            <Input
              id="wallet-name"
              type="text"
              value={walletName}
              onChange={(e) => setWalletName(e.target.value)}
              placeholder="Enter wallet name"
              disabled={isUpdating}
            />
          </div>

          {/* Contact Management Section */}
          <div>
            <div className="flex items-center gap-2 mb-4">
              <Users className="h-4 w-4" />
              <h3 className="text-lg font-semibold">SMS Notifications</h3>
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
                      <div key={contact.id} className="flex items-center justify-between p-3 bg-green-50 rounded-lg">
                        <div className="flex items-center gap-3">
                          <Phone className="h-4 w-4 text-green-600" />
                          <div>
                            <p className="text-sm font-medium">{contact.name}</p>
                            <p className="text-xs text-muted-foreground">{formatPhoneForDisplay(contact.phone_number)}</p>
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

                    {walletContacts.length === 0 && (
                      <div className="text-center py-4 text-sm text-muted-foreground">
                        No contacts are receiving SMS notifications for this wallet.
                      </div>
                    )}

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
                            <Label htmlFor="contact-phone" className="text-xs">Phone Number</Label>
                            <Input
                              id="contact-phone"
                              value={newContactPhone}
                              onChange={(e) => setNewContactPhone(e.target.value)}
                              placeholder="+4712345678"
                              disabled={isCreatingContact}
                            />
                          </div>
                        </div>
                        <Button
                          onClick={handleCreateContact}
                          disabled={isCreatingContact || !newContactName.trim() || !newContactPhone.trim()}
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

        <DialogFooter className="flex justify-between">
          <Button
            variant="destructive"
            onClick={handleDelete}
            disabled={isUpdating}
            className="flex items-center gap-2"
          >
            <Trash2 className="h-4 w-4" />
            Delete Wallet
          </Button>
          
          <Button
            onClick={handleSave}
            disabled={isUpdating || !walletName.trim()}
          >
            {isUpdating ? "Updating..." : "Save Changes"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}