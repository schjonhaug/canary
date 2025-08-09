"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Bell, Mail, Smartphone } from "lucide-react"
import { Contact } from "../types"
import { api } from "../lib/api"
import { ContactModal } from "./contact-modal"

interface WalletContactsListProps {
  walletChecksum: string
  contacts: Contact[]
  onContactsUpdated?: () => void
}

export function WalletContactsList({ walletChecksum, contacts, onContactsUpdated }: WalletContactsListProps) {
  const [error, setError] = useState<string | null>(null)
  const [isEditModalOpen, setIsEditModalOpen] = useState(false)
  const [editingContact, setEditingContact] = useState<Contact | null>(null)


  const handleEditContact = (contact: Contact) => {
    setEditingContact(contact)
    setIsEditModalOpen(true)
  }

  const handleContactSaved = () => {
    setIsEditModalOpen(false)
    setEditingContact(null)
    if (onContactsUpdated) {
      onContactsUpdated()
    }
  }

  return (
    <div>
      {error && (
        <div className="mb-2 text-sm text-red-600">{error}</div>
      )}
      
      {contacts.length > 0 ? (
        <div className="space-y-2">
          {contacts.sort((a, b) => a.name.localeCompare(b.name)).map((contact) => (
            <div 
              key={contact.id} 
              onClick={() => handleEditContact(contact)}
              className="p-2 bg-muted/30 rounded-md cursor-pointer hover:bg-muted/50 transition-colors"
            >
              <div className="flex items-center gap-2 mb-1">
                <span className="text-sm font-medium truncate">{contact.name}</span>
              </div>
              <div className="space-y-1">
                {contact.notification_methods?.map((method) => (
                  <div key={method.id} className="flex items-center gap-1 text-xs text-muted-foreground">
                    {method.provider_type === 'sms' ? (
                      <Smartphone className="h-3 w-3 shrink-0" />
                    ) : method.provider_type === 'email' ? (
                      <Mail className="h-3 w-3 shrink-0" />
                    ) : (
                      <Bell className="h-3 w-3 shrink-0" />
                    )}
                    <span className="truncate">{method.display_target || method.notification_target}</span>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="text-sm text-muted-foreground text-center py-4">
          No contacts added yet
        </div>
      )}

      <ContactModal
        key={`edit-contact-${editingContact?.id || 'none'}`}
        isOpen={isEditModalOpen}
        onClose={() => {
          setIsEditModalOpen(false)
          setEditingContact(null)
        }}
        walletChecksum={walletChecksum}
        onContactSaved={handleContactSaved}
        editContact={editingContact || undefined}
      />
    </div>
  )
}