"use client"

import { useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Bell, Mail, MessageCircle, AlertTriangle } from "lucide-react"
import { Contact } from "../types"
import { ContactModal } from "./contact-modal"
import { useAuth } from "@/contexts/auth-context"

interface WalletContactsListProps {
  walletChecksum: string
  contacts: Contact[]
  onContactsUpdated?: () => void
  isWalletActive?: boolean
}

export function WalletContactsList({ walletChecksum, contacts, onContactsUpdated, isWalletActive = true }: WalletContactsListProps) {
  const [isEditModalOpen, setIsEditModalOpen] = useState(false)
  const [editingContact, setEditingContact] = useState<Contact | null>(null)
  const { user, isSaasMode, billingStatus } = useAuth()

  // All notification methods are available for all tiers - no need to check provider type


  const handleEditContact = (contact: Contact) => {
    // Don't allow admin users in SaaS mode to edit contacts
    if (isSaasMode && user?.is_admin) {
      return
    }
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
      {contacts.length > 0 ? (
        <div className="space-y-2">
          {contacts.sort((a, b) => {
            const nameComparison = a.name.localeCompare(b.name);
            if (nameComparison !== 0) {
              return nameComparison;
            }
            // Secondary sort by created_at for contacts with the same name
            return a.created_at.localeCompare(b.created_at);
          }).map((contact) => {
            const isInactive = contact.is_active === false
            const shouldShowInactiveState = isInactive && isWalletActive
            const isAdminInSaas = isSaasMode && user?.is_admin
            
            return (
              <button
                key={contact.id} 
                onClick={() => handleEditContact(contact)}
                disabled={isAdminInSaas}
                className={`w-full p-2 rounded-md transition-colors text-left ${
                  isAdminInSaas 
                    ? 'bg-muted/20 cursor-not-allowed' 
                    : shouldShowInactiveState 
                      ? 'bg-orange-50/50 border border-orange-200 hover:bg-muted/50' 
                      : 'bg-muted/30 hover:bg-muted/50'
                }`}
              >
                <div className="flex items-center justify-between mb-1">
                  <div className="flex items-center gap-2">
                    <span className={`text-sm font-medium truncate ${shouldShowInactiveState ? 'text-muted-foreground line-through' : ''}`}>
                      {contact.name}
                    </span>
                    {shouldShowInactiveState && (
                      <Badge variant="outline" className="text-xs text-orange-600 border-orange-600 bg-orange-50">
                        <AlertTriangle className="h-3 w-3 mr-1" />
                        Inactive
                      </Badge>
                    )}
                  </div>
                </div>
                {shouldShowInactiveState && (
                  <div className="text-xs text-orange-600 mb-1">
                    {billingStatus?.subscription_status === 'expired'
                      ? "Your subscription has expired - contact won't receive notifications"
                      : "This contact exceeds your subscription tier limits and won't receive notifications"}
                  </div>
                )}
                <div className="space-y-1">
                  {contact.notification_methods?.map((method) => (
                    <div key={method.id} className="flex items-center gap-1 text-xs text-muted-foreground">
                      <div className="flex items-center gap-1">
                        {method.provider_type === 'sms' ? (
                          <MessageCircle className="h-3 w-3 shrink-0" />
                        ) : method.provider_type === 'email' ? (
                          <Mail className="h-3 w-3 shrink-0" />
                        ) : (
                          <Bell className="h-3 w-3 shrink-0" />
                        )}
                        {method.provider_type === 'sms' ? (
                          <a
                            href={`tel:${method.notification_target}`}
                            className="truncate text-blue-600 hover:text-blue-800 underline"
                            onClick={(e) => e.stopPropagation()}
                          >
                            {method.display_target || method.notification_target}
                          </a>
                        ) : method.provider_type === 'email' ? (
                          <a
                            href={`mailto:${method.notification_target}`}
                            className="truncate text-blue-600 hover:text-blue-800 underline"
                            onClick={(e) => e.stopPropagation()}
                          >
                            {method.display_target || method.notification_target}
                          </a>
                        ) : method.provider_type === 'ntfy' ? (
                          <a
                            href={`https://ntfy.sh/${method.notification_target}`}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="truncate text-blue-600 hover:text-blue-800 underline"
                            onClick={(e) => e.stopPropagation()}
                          >
                            {method.display_target || method.notification_target}
                          </a>
                        ) : (
                          <span className="truncate">
                            {method.display_target || method.notification_target}
                          </span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </button>
            )
          })}
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