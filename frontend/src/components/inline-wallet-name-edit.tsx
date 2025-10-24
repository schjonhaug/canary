"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Edit, Check, X } from "lucide-react"
import { api } from "@/lib/api"
import { useAuth } from "@/contexts/auth-context"

interface InlineWalletNameEditProps {
  walletChecksum: string
  currentName: string
  onNameUpdated?: (newName: string) => void
  size?: "default" | "small"
}

export function InlineWalletNameEdit({ walletChecksum, currentName, onNameUpdated, size = "default" }: InlineWalletNameEditProps) {
  const [isEditing, setIsEditing] = useState(false)
  const [name, setName] = useState(currentName)
  const [isUpdating, setIsUpdating] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const { user, isCloudMode } = useAuth()

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
      await api.updateWallet(walletChecksum, name.trim())
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

  const textClasses = size === "small" ? "text-sm font-medium" : "text-2xl font-semibold"
  const inputClasses = size === "small" ? "text-sm font-medium h-auto py-1 px-2" : "text-2xl font-semibold h-auto py-1 px-2"
  const editIconSize = size === "small" ? 12 : 16

  if (isEditing) {
    return (
      <div className="space-y-2">
        <div className="flex items-center gap-2 min-w-0">
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={handleKeyDown}
            className={`${inputClasses} min-w-0`}
            disabled={isUpdating}
            autoFocus
          />
          <Button
            size="sm"
            onClick={handleSave}
            disabled={isUpdating || !name.trim()}
            className="shrink-0"
          >
            <Check size={editIconSize} />
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={handleCancel}
            disabled={isUpdating}
            className="shrink-0"
          >
            <X size={editIconSize} />
          </Button>
        </div>
        {error && (
          <p className="text-sm text-red-600">{error}</p>
        )}
      </div>
    )
  }

  // For admin or demo users in SaaS mode, show only the name without edit button
  if (isCloudMode && (user?.is_admin || user?.is_demo)) {
    return (
      <div className="flex items-center gap-2 min-w-0">
        <span className={`${textClasses} truncate`} title={currentName}>{currentName}</span>
      </div>
    )
  }

  return (
    <div className="flex items-center gap-2 group min-w-0">
      <span className={`${textClasses} truncate`} title={currentName}>{currentName}</span>
      <Button
        size="sm"
        variant="ghost"
        onClick={handleEdit}
        className="opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
      >
        <Edit size={editIconSize} />
      </Button>
    </div>
  )
}