"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Edit, Check, X } from "lucide-react"
import { api } from "@/lib/api"

interface InlineWalletNameEditProps {
  walletId: number
  currentName: string
  onNameUpdated?: (newName: string) => void
}

export function InlineWalletNameEdit({ walletId, currentName, onNameUpdated }: InlineWalletNameEditProps) {
  const [isEditing, setIsEditing] = useState(false)
  const [name, setName] = useState(currentName)
  const [isUpdating, setIsUpdating] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleEdit = () => {
    setIsEditing(true)
    setName(currentName)
    setError(null)
  }

  const handleCancel = () => {
    setIsEditing(false)
    setName(currentName)
    setError(null)
  }

  const handleSave = async () => {
    if (!name.trim()) {
      setError("Wallet name cannot be empty")
      return
    }

    if (name.trim() === currentName) {
      setIsEditing(false)
      return
    }

    setIsUpdating(true)
    setError(null)

    try {
      await api.updateWallet(walletId, name.trim())
      setIsEditing(false)
      if (onNameUpdated) {
        onNameUpdated(name.trim())
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update wallet name')
    } finally {
      setIsUpdating(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleSave()
    } else if (e.key === 'Escape') {
      handleCancel()
    }
  }

  if (isEditing) {
    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={handleKeyDown}
            className="text-3xl font-bold tracking-wide h-auto py-1 px-2"
            disabled={isUpdating}
            autoFocus
          />
          <Button
            size="sm"
            onClick={handleSave}
            disabled={isUpdating || !name.trim()}
            className="shrink-0"
          >
            <Check size={16} />
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={handleCancel}
            disabled={isUpdating}
            className="shrink-0"
          >
            <X size={16} />
          </Button>
        </div>
        {error && (
          <p className="text-sm text-red-600">{error}</p>
        )}
      </div>
    )
  }

  return (
    <div className="flex items-center gap-2 group">
      <h1 className="text-3xl font-bold tracking-wide">{currentName}</h1>
      <Button
        size="sm"
        variant="ghost"
        onClick={handleEdit}
        className="opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
      >
        <Edit size={16} />
      </Button>
    </div>
  )
}