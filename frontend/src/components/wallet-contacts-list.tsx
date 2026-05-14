"use client"

import { useMemo, useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Bell, Mail, MessageCircle, AlertTriangle, Edit } from "lucide-react"
import { Contact } from "../types"
import { ContactModal } from "./contact-modal"
import { useAuth } from "@/contexts/auth-context"
import { useTranslations } from "next-intl"
import { useNtfyServerTarget } from "@/hooks/useNtfyServerUrl"

interface WalletContactsListProps {
  walletChecksum: string
  contacts: Contact[]
  onContactsUpdated?: () => void
  isWalletActive?: boolean
}

export function WalletContactsList({ walletChecksum, contacts, onContactsUpdated, isWalletActive = true }: WalletContactsListProps) {
  const [isEditModalOpen, setIsEditModalOpen] = useState(false)
  const [editingContact, setEditingContact] = useState<Contact | null>(null)
  const ntfyServerTarget = useNtfyServerTarget()
  const { user, isCloudMode, billingStatus } = useAuth()
  const t = useTranslations('contacts')

  // All notification methods are available for all tiers - no need to check provider type

  const sortedContacts = useMemo(() => [...contacts].sort((a, b) => {
    const nameComparison = a.name.localeCompare(b.name);
    if (nameComparison !== 0) {
      return nameComparison;
    }
    // Secondary sort by created_at for contacts with the same name
    return new Date(a.created_at).getTime() - new Date(b.created_at).getTime();
  }), [contacts])
  const isAdminInSaas = isCloudMode && (user?.is_admin || user?.is_demo)

  const handleEditContact = (contact: Contact) => {
    // Don't allow admin or demo users in cloud mode to edit contacts
    if (isAdminInSaas) {
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
      {sortedContacts.length > 0 ? (
        <ul className="space-y-2" aria-label={t('title')}>
          {sortedContacts.map((contact) => {
            const isInactive = contact.is_active === false
            const shouldShowInactiveState = isInactive && isWalletActive

            return (
              <li
                key={contact.id}
                className={`p-2 rounded-md ${
                  isAdminInSaas
                    ? 'bg-muted/20'
                    : shouldShowInactiveState
                      ? 'bg-orange-50/50 border border-orange-200'
                      : 'bg-muted/30'
                }`}
              >
                <div className="flex items-start justify-between gap-2 mb-1">
                  <div className="flex items-center gap-2">
                    <span className={`text-sm font-medium truncate ${shouldShowInactiveState ? 'text-muted-foreground line-through' : ''}`}>
                      {contact.name}
                    </span>
                    {shouldShowInactiveState && (
                      <Badge variant="outline" className="text-xs text-orange-600 border-orange-600 bg-orange-50">
                        <AlertTriangle className="h-3 w-3 mr-1" aria-hidden="true" />
                        {t('inactive.badge')}
                      </Badge>
                    )}
                  </div>
                  <button
                    type="button"
                    onClick={() => handleEditContact(contact)}
                    disabled={isAdminInSaas}
                    aria-label={t('edit.actionLabel', { name: contact.name })}
                    className="shrink-0 rounded-md p-1.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    <Edit className="h-4 w-4" aria-hidden="true" />
                  </button>
                </div>
                {shouldShowInactiveState && (
                  <div className="text-xs text-orange-600 mb-1">
                    {billingStatus?.subscription_status === 'expired'
                      ? t('inactive.expired')
                      : t('inactive.tierLimit')}
                  </div>
                )}
                <div className="space-y-1">
                  {contact.notification_methods?.map((method) => (
                    <div key={method.id} className="flex items-center gap-1 text-xs text-muted-foreground">
                      <div className="flex items-center gap-1">
                        {method.provider_type === 'sms' ? (
                          <MessageCircle className="h-3 w-3 shrink-0" aria-hidden="true" />
                        ) : method.provider_type === 'email' ? (
                          <Mail className="h-3 w-3 shrink-0" aria-hidden="true" />
                        ) : (
                          <Bell className="h-3 w-3 shrink-0" aria-hidden="true" />
                        )}
                        {method.provider_type === 'sms' ? (
                          <a
                            href={`tel:${method.notification_target}`}
                            className="truncate text-blue-600 hover:text-blue-800 underline"
                          >
                            {method.display_target || method.notification_target}
                          </a>
                        ) : method.provider_type === 'email' ? (
                          <a
                            href={`mailto:${method.notification_target}`}
                            className="truncate text-blue-600 hover:text-blue-800 underline"
                          >
                            {method.display_target || method.notification_target}
                          </a>
                        ) : method.provider_type === 'ntfy' && ntfyServerTarget.isBrowserSafe ? (
                          <a
                            href={`${ntfyServerTarget.url}/${method.notification_target}`}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="truncate text-blue-600 hover:text-blue-800 underline"
                          >
                            {method.display_target || method.notification_target}
                          </a>
                        ) : method.provider_type === 'ntfy' ? (
                          <span className="truncate">
                            {method.display_target || method.notification_target}
                          </span>
                        ) : (
                          <span className="truncate">
                            {method.display_target || method.notification_target}
                          </span>
                        )}
                      </div>
                    </div>
                  ))}
                </div>
              </li>
            )
          })}
        </ul>
      ) : (
        <div className="text-sm text-muted-foreground text-center py-4">
          {t('empty.list')}
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
