'use client'

import { useState } from 'react'
import { useAuth } from '@/contexts/auth-context'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Shield, User, ChevronDown, LogOut } from 'lucide-react'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'

const DEV_USERS = [
  { phone: '+4799999900', isAdmin: true },
  { phone: '+4799999901', isAdmin: false },
  { phone: '+4699999902', isAdmin: false },
  { phone: '+3399999903', isAdmin: false },
]

export function DevUserSwitcher() {
  const { isDevMode, user, devLogin, logout } = useAuth()
  const [isLoading, setIsLoading] = useState(false)

  if (!isDevMode) {
    return null
  }

  const handleSwitchUser = async (phone: string) => {
    setIsLoading(true)
    try {
      await devLogin(phone)
    } catch (error) {
      console.error('Failed to switch user:', error)
    } finally {
      setIsLoading(false)
    }
  }

  const currentUser = DEV_USERS.find(u => u.phone === user?.phone_number)

  return (
    <div className="fixed bottom-4 right-4 z-50">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="outline" size="sm" className="bg-blue-50 border-blue-200 text-blue-800 hover:bg-blue-100">
            {currentUser ? (
              <>
                {currentUser.isAdmin ? <Shield className="mr-2 h-4 w-4" /> : <User className="mr-2 h-4 w-4" />}
                {currentUser.phone}
                <ChevronDown className="ml-2 h-4 w-4" />
              </>
            ) : (
              <>
                <User className="mr-2 h-4 w-4" />
                Switch User
                <ChevronDown className="ml-2 h-4 w-4" />
              </>
            )}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-56">
          <div className="px-2 py-1.5 text-sm font-medium text-muted-foreground">
            Development Users
          </div>
          {DEV_USERS.map((devUser) => (
            <DropdownMenuItem
              key={devUser.phone}
              onClick={() => handleSwitchUser(devUser.phone)}
              disabled={isLoading || devUser.phone === user?.phone_number}
              className="flex items-center justify-between"
            >
              <div className="flex items-center">
                {devUser.isAdmin ? <Shield className="mr-2 h-3 w-3" /> : <User className="mr-2 h-3 w-3" />}
                <span>{devUser.phone}</span>
              </div>
              {devUser.phone === user?.phone_number && (
                <Badge variant="secondary" className="text-xs">
                  Current
                </Badge>
              )}
            </DropdownMenuItem>
          ))}
          <div className="border-t mt-1 pt-1">
            <DropdownMenuItem
              onClick={logout}
              className="text-red-600 hover:text-red-700"
            >
              <LogOut className="mr-2 h-4 w-4" />
              Logout
            </DropdownMenuItem>
          </div>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
} 