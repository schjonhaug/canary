"use client"

import { useEffect, useState } from "react"
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Plus, Trash2, Edit2, Users, Phone, X } from "lucide-react"
import { extractChecksum } from "@/lib/utils"
import { isValidPhoneNumber, formatNumber } from "libphonenumber-js"

interface Contact {
  id: number
  name: string
  phone_number: string
}

interface Wallet {
  id: number
  name: string
  descriptor: string
  wallet_filename: string
  created_at: string
  balance_total?: number
  last_activity?: string
}

interface ContactsModalProps {
  isOpen: boolean
  onClose: () => void
}

export function ContactsModal({ isOpen, onClose }: ContactsModalProps) {
  const [contacts, setContacts] = useState<Contact[]>([])
  const [wallets, setWallets] = useState<Wallet[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [editingContact, setEditingContact] = useState<Contact | null>(null)
  const [isCreating, setIsCreating] = useState(false)
  const [selectedWallet, setSelectedWallet] = useState<Wallet | null>(null)
  const [walletContacts, setWalletContacts] = useState<Contact[]>([])
  const [formData, setFormData] = useState({
    name: "",
    phone_number: ""
  })
  const [formErrors, setFormErrors] = useState({
    name: "",
    phone_number: ""
  })

  const apiUrl = `http://${typeof window !== 'undefined' ? window.location.hostname : 'localhost'}:3000`

  // Format phone number for display using libphonenumber-js
  const formatPhoneForDisplay = (phoneNumber: string): string => {
    try {
      const formatted = formatNumber(phoneNumber, 'INTERNATIONAL')
      return formatted || phoneNumber
    } catch {
      return phoneNumber
    }
  }

  // Validate phone number
  const validatePhoneNumber = (phoneNumber: string): string | null => {
    if (!phoneNumber.trim()) {
      return "Phone number is required"
    }
    
    if (!phoneNumber.startsWith('+')) {
      return "Phone number must include country code (e.g., +4712345678)"
    }
    
    if (!isValidPhoneNumber(phoneNumber)) {
      return "Invalid phone number format"
    }
    
    return null
  }

  const fetchContacts = async () => {
    try {
      const response = await fetch(`${apiUrl}/contacts`)
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`)
      const data = await response.json()
      setContacts(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch contacts")
    }
  }

  const fetchWallets = async () => {
    try {
      const response = await fetch(`${apiUrl}/wallets`)
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`)
      const data = await response.json()
      setWallets(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch wallets")
    }
  }

  const fetchWalletContacts = async (walletId: number) => {
    try {
      const response = await fetch(`${apiUrl}/wallets/${walletId}/contacts`)
      if (!response.ok) throw new Error(`HTTP error! status: ${response.status}`)
      const data = await response.json()
      setWalletContacts(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch wallet contacts")
    }
  }

  useEffect(() => {
    if (isOpen) {
      Promise.all([fetchContacts(), fetchWallets()])
        .finally(() => setLoading(false))
    }
  }, [isOpen])

  useEffect(() => {
    if (selectedWallet) {
      fetchWalletContacts(selectedWallet.id)
    }
  }, [selectedWallet])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    
    // Clear previous errors
    setFormErrors({ name: "", phone_number: "" })
    
    // Validate form data
    const nameError = formData.name.trim() ? "" : "Name is required"
    const phoneError = validatePhoneNumber(formData.phone_number) || ""
    
    if (nameError || phoneError) {
      setFormErrors({ name: nameError, phone_number: phoneError })
      return
    }
    
    try {
      const url = editingContact 
        ? `${apiUrl}/contacts/${editingContact.id}` 
        : `${apiUrl}/contacts`
      
      const method = editingContact ? 'PUT' : 'POST'
      
      const response = await fetch(url, {
        method,
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(formData),
      })

      if (!response.ok) {
        const errorData = await response.json().catch(() => ({}))
        throw new Error(errorData.error || `HTTP error! status: ${response.status}`)
      }

      await fetchContacts()
      resetForm()
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save contact")
    }
  }

  const handleDelete = async (contactId: number) => {
    if (!confirm("Are you sure you want to delete this contact?")) return

    try {
      const response = await fetch(`${apiUrl}/contacts/${contactId}`, {
        method: 'DELETE',
      })

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`)
      }

      await fetchContacts()
      if (selectedWallet) {
        await fetchWalletContacts(selectedWallet.id)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete contact")
    }
  }

  const handleEdit = (contact: Contact) => {
    setEditingContact(contact)
    setFormData({
      name: contact.name,
      phone_number: contact.phone_number
    })
    setIsCreating(true)
  }

  const resetForm = () => {
    setFormData({ name: "", phone_number: "" })
    setFormErrors({ name: "", phone_number: "" })
    setEditingContact(null)
    setIsCreating(false)
  }

  const handleAddContactToWallet = async (contactId: number) => {
    if (!selectedWallet) return

    try {
      const response = await fetch(`${apiUrl}/wallets/${selectedWallet.id}/contacts`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ contact_id: contactId }),
      })

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`)
      }

      await fetchWalletContacts(selectedWallet.id)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add contact to wallet")
    }
  }

  const handleRemoveContactFromWallet = async (contactId: number) => {
    if (!selectedWallet) return

    try {
      const response = await fetch(`${apiUrl}/wallets/${selectedWallet.id}/contacts/${contactId}`, {
        method: 'DELETE',
      })

      if (!response.ok) {
        throw new Error(`HTTP error! status: ${response.status}`)
      }

      await fetchWalletContacts(selectedWallet.id)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove contact from wallet")
    }
  }

  const isContactInWallet = (contactId: number) => {
    return walletContacts.some(wc => wc.id === contactId)
  }


  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Users className="h-5 w-5" />
            Contact Management
          </DialogTitle>
        </DialogHeader>

        {error && (
          <div className="bg-red-50 border border-red-200 rounded-md p-3 mb-4">
            <p className="text-sm text-red-600">{error}</p>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setError(null)}
              className="mt-2"
            >
              Dismiss
            </Button>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Left Column - Contacts Management */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-lg font-semibold">Contacts</h3>
              <Button
                onClick={() => setIsCreating(true)}
                size="sm"
                className="gap-2"
              >
                <Plus className="h-4 w-4" />
                Add Contact
              </Button>
            </div>

            {isCreating && (
              <Card>
                <CardHeader>
                  <CardTitle className="text-base">
                    {editingContact ? "Edit Contact" : "Add New Contact"}
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <form onSubmit={handleSubmit} className="space-y-4">
                    <div>
                      <Label htmlFor="name">Name</Label>
                      <Input
                        id="name"
                        value={formData.name}
                        onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
                        placeholder="Enter contact name"
                        required
                        className={formErrors.name ? "border-red-500" : ""}
                      />
                      {formErrors.name && (
                        <p className="text-sm text-red-600 mt-1">{formErrors.name}</p>
                      )}
                    </div>
                    <div>
                      <Label htmlFor="phone_number">Phone Number</Label>
                      <Input
                        id="phone_number"
                        value={formData.phone_number}
                        onChange={(e) => setFormData(prev => ({ ...prev, phone_number: e.target.value }))}
                        placeholder="+4712345678"
                        required
                        className={formErrors.phone_number ? "border-red-500" : ""}
                      />
                      {formErrors.phone_number && (
                        <p className="text-sm text-red-600 mt-1">{formErrors.phone_number}</p>
                      )}
                      <p className="text-sm text-gray-500 mt-1">Include country code (e.g., +47 for Norway)</p>
                    </div>
                    <div className="flex gap-2">
                      <Button type="submit" size="sm">
                        {editingContact ? "Update" : "Create"}
                      </Button>
                      <Button type="button" variant="outline" size="sm" onClick={resetForm}>
                        Cancel
                      </Button>
                    </div>
                  </form>
                </CardContent>
              </Card>
            )}

            {loading ? (
              <Card>
                <CardContent className="p-4">
                  <p className="text-sm text-muted-foreground">Loading contacts...</p>
                </CardContent>
              </Card>
            ) : (
              <div className="space-y-2">
                {contacts.map((contact) => (
                  <Card key={contact.id} className="p-3">
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <Phone className="h-4 w-4 text-muted-foreground" />
                        <div>
                          <p className="font-medium">{contact.name}</p>
                          <p className="text-sm text-muted-foreground">{formatPhoneForDisplay(contact.phone_number)}</p>
                        </div>
                      </div>
                      <div className="flex gap-2">
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleEdit(contact)}
                        >
                          <Edit2 className="h-4 w-4" />
                        </Button>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleDelete(contact.id)}
                          className="text-red-600 hover:text-red-700"
                        >
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </div>
                  </Card>
                ))}
                {contacts.length === 0 && (
                  <Card>
                    <CardContent className="p-4 text-center">
                      <p className="text-sm text-muted-foreground">No contacts yet. Add your first contact!</p>
                    </CardContent>
                  </Card>
                )}
              </div>
            )}
          </div>

          {/* Right Column - Wallet Contact Assignment */}
          <div className="space-y-4">
            <h3 className="text-lg font-semibold">Wallet Notifications</h3>
            
            <div>
              <Label htmlFor="wallet-select">Select Wallet</Label>
              <div className="grid grid-cols-1 gap-2 mt-2">
                {wallets.map((wallet) => (
                  <Button
                    key={wallet.id}
                    variant={selectedWallet?.id === wallet.id ? "default" : "outline"}
                    onClick={() => setSelectedWallet(wallet)}
                    className="justify-start h-auto p-3"
                  >
                    <div className="text-left">
                      <div className="font-medium">{wallet.name}</div>
                      <div className="text-xs text-muted-foreground">
                        #{extractChecksum(wallet.descriptor)}
                      </div>
                    </div>
                  </Button>
                ))}
              </div>
            </div>

            {selectedWallet && (
              <Card>
                <CardHeader>
                  <CardTitle className="text-base">
                    SMS Notifications for &quot;{selectedWallet.name}&quot;
                  </CardTitle>
                </CardHeader>
                <CardContent>
                  <div className="space-y-3">
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-medium">Active Contacts</span>
                      <Badge variant="secondary">
                        {walletContacts.length} contact{walletContacts.length !== 1 ? 's' : ''}
                      </Badge>
                    </div>
                    
                    {walletContacts.map((contact) => (
                      <div key={contact.id} className="flex items-center justify-between p-2 bg-green-50 rounded">
                        <div className="flex items-center gap-2">
                          <Phone className="h-4 w-4 text-green-600" />
                          <div>
                            <p className="text-sm font-medium">{contact.name}</p>
                            <p className="text-xs text-muted-foreground">{formatPhoneForDisplay(contact.phone_number)}</p>
                          </div>
                        </div>
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleRemoveContactFromWallet(contact.id)}
                          className="text-red-600 hover:text-red-700"
                        >
                          <X className="h-4 w-4" />
                        </Button>
                      </div>
                    ))}

                    <div className="border-t pt-3">
                      <p className="text-sm font-medium mb-2">Add Contact to Wallet</p>
                      <div className="space-y-2">
                        {contacts.filter(c => !isContactInWallet(c.id)).map((contact) => (
                          <div key={contact.id} className="flex items-center justify-between p-2 bg-gray-50 rounded">
                            <div className="flex items-center gap-2">
                              <Phone className="h-4 w-4 text-muted-foreground" />
                              <div>
                                <p className="text-sm font-medium">{contact.name}</p>
                                <p className="text-xs text-muted-foreground">{formatPhoneForDisplay(contact.phone_number)}</p>
                              </div>
                            </div>
                            <Button
                              variant="ghost"
                              size="sm"
                              onClick={() => handleAddContactToWallet(contact.id)}
                              className="text-green-600 hover:text-green-700"
                            >
                              <Plus className="h-4 w-4" />
                            </Button>
                          </div>
                        ))}
                        {contacts.filter(c => !isContactInWallet(c.id)).length === 0 && (
                          <p className="text-xs text-muted-foreground text-center py-2">
                            All contacts are already added to this wallet
                          </p>
                        )}
                      </div>
                    </div>
                  </div>
                </CardContent>
              </Card>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}