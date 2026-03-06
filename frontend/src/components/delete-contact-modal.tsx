"use client"

import { useState } from "react"
import { Trash2 } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"
import { ApiError } from "../lib/api"
import { Contact } from "../types"
import { useTranslations } from "next-intl"
import { getTranslatedApiError } from "@/lib/utils"

interface DeleteContactModalProps {
  contact: Contact | null
  isOpen: boolean
  onClose: () => void
  onConfirmDelete: () => Promise<void>
}

export function DeleteContactModal({
  contact,
  isOpen,
  onClose,
  onConfirmDelete,
}: DeleteContactModalProps) {
  const [isDeleting, setIsDeleting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const t = useTranslations('contacts')
  const tCommon = useTranslations('common')
  const tApiErrors = useTranslations('errors.api')

  const handleDelete = async () => {
    if (!contact) return

    setIsDeleting(true)
    setError(null)

    try {
      await onConfirmDelete()
      onClose()
    } catch (err) {
      setError(err instanceof ApiError ? getTranslatedApiError(err, tApiErrors) : t('delete.failed'))
    } finally {
      setIsDeleting(false)
    }
  }

  const handleClose = () => {
    if (!isDeleting) {
      setError(null)
      onClose()
    }
  }

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[425px]">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-destructive">
            <Trash2 className="h-5 w-5" />
            {t('delete.title')}
          </DialogTitle>
          <DialogDescription>
            {t.rich('delete.description', {
              name: contact?.name ?? '',
              bold: (chunks) => <strong>{chunks}</strong>
            })}
          </DialogDescription>
        </DialogHeader>

        {error && (
          <div className="p-3 bg-red-50 border border-red-200 rounded-lg">
            <p className="text-sm text-red-700">{error}</p>
          </div>
        )}

        <DialogFooter>
          <Button
            variant="outline"
            onClick={handleClose}
            disabled={isDeleting}
          >
            {tCommon('cancel')}
          </Button>
          <Button
            variant="destructive"
            onClick={handleDelete}
            disabled={isDeleting}
          >
            {isDeleting ? tCommon('deleting') : tCommon('delete')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}