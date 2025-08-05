"use client"

import { useState, useEffect } from "react"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import { Bell, Smartphone, Trash2, Users } from "lucide-react"
import { Contact } from "../types"
import { api } from "../lib/api"

interface WalletContactsListProps {
  walletChecksum: string
  onContactsUpdated?: () => void
}

export function WalletContactsList({ walletChecksum, onContactsUpdated }: WalletContactsListProps) {
  const [contacts, setContacts] = useState<Contact[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const fetchContacts = async () => {
    setIsLoading(true)
    setError(null)
    
    try {
      const contactsData = await api.getWalletContacts(walletChecksum)
      setContacts(contactsData)
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load contacts")
    } finally {
      setIsLoading(false)
    }
  }

  const handleDeleteContact = async (contactId: number) => {
    try {
      await api.deleteContact(walletChecksum, contactId)
      await fetchContacts()
      if (onContactsUpdated) {
        onContactsUpdated()
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to delete contact")
    }
  }

  useEffect(() => {
    fetchContacts()
  }, [walletChecksum])

  if (isLoading && contacts.length === 0) {
    return (
      <div>
        <div className="text-sm text-muted-foreground">Contacts</div>
        <div className="mt-2 text-sm text-muted-foreground">Loading...</div>
      </div>
    )
  }

  return (
    <div>
      <div className="text-sm text-muted-foreground">Contacts</div>
      {error && (
        <div className="mt-2 text-sm text-red-600">{error}</div>
      )}
      
      {contacts.length > 0 && (
        <div className="mt-2 space-y-2">
          {contacts.map((contact) => (
            <div key={contact.id} className="flex items-start justify-between p-2 bg-muted/30 rounded-md">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span className="text-sm font-medium truncate">{contact.name}</span>
                  <Badge variant="outline" className="text-xs shrink-0">
                    {contact.language === 'no' ? 'NO' : 'EN'}
                  </Badge>
                </div>
                <div className="space-y-1">
                  {contact.notification_methods?.map((method) => (
                    <div key={method.id} className="flex items-center gap-1 text-xs text-muted-foreground">
                      {method.provider_type === 'sms' ? (
                        <Smartphone className="h-3 w-3 shrink-0" />
                      ) : (
                        <Bell className="h-3 w-3 shrink-0" />
                      )}
                      <span className="truncate">{method.display_target || method.notification_target}</span>
                    </div>
                  ))}
                </div>
              </div>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => handleDeleteContact(contact.id)}
                className="h-6 w-6 p-0 text-muted-foreground hover:text-red-600 shrink-0"
              >
                <Trash2 className="h-3 w-3" />
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}