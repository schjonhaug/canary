'use client'

import { useAuth } from '@/contexts/auth-context'
import { Badge } from '@/components/ui/badge'
import { Shield } from 'lucide-react'

export function DevIndicator() {
  const { user } = useAuth()
  const isDevMode = process.env.NODE_ENV === 'development'

  if (!isDevMode) {
    return null
  }

  return (
    <div className="fixed top-4 right-4 z-50">
      <Badge variant="secondary" className="bg-blue-100 text-blue-800 border-blue-200">
        <Shield className="mr-1 h-3 w-3" />
        DEV MODE
        {user && (
          <span className="ml-2 text-xs">
            {user.is_admin ? 'ADMIN' : 'USER'} - {user.phone_number.slice(-4)}
          </span>
        )}
      </Badge>
    </div>
  )
} 