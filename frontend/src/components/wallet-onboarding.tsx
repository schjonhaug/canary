"use client"

import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Plus, Info, Clock, Sparkles } from "lucide-react"
import { useAuth } from "@/contexts/auth-context"

interface User {
  name?: string
  email: string
}

interface WalletOnboardingProps {
  onAddWallet: () => void
  user?: User | null
}

export function WalletOnboarding({ onAddWallet, user }: WalletOnboardingProps) {
  const { billingStatus } = useAuth()
  const isPending = billingStatus?.subscription_status === 'pending'
  
  // If user is in pending status, show trial activation message + wallet descriptor info
  if (isPending) {
    return (
      <div className="max-w-3xl mx-auto mt-16">
        <Alert className="mb-6 bg-gradient-to-r from-blue-50 to-indigo-50 border-blue-200">
          <Sparkles className="h-5 w-5 text-blue-600" />
          <AlertTitle className="text-blue-800">Start Your 30-Day Team Trial</AlertTitle>
          <AlertDescription className="text-blue-700">
            Your trial will begin when you add your first wallet. You&apos;ll get full access to all Team features for 30 days.
          </AlertDescription>
        </Alert>
        
        <Card className="border-muted">
          <CardContent className="pt-12 pb-10 px-8">
            <div className="space-y-8">
              {/* Trial Benefits Section */}
              <div className="text-center space-y-6">
                <div className="inline-flex items-center justify-center w-16 h-16 bg-blue-100 rounded-full mb-4">
                  <Clock className="w-8 h-8 text-blue-600" />
                </div>
                
                <h2 className="text-3xl font-bold">
                  Ready to Start Your Trial{user?.name ? `, ${user.name}` : ''}?
                </h2>
                
                <p className="text-lg text-muted-foreground max-w-2xl mx-auto">
                  Add your first wallet to activate your <span className="font-semibold text-blue-600">30-day Team trial</span>. 
                  You&apos;ll get instant access to all features including real-time sync, unlimited notifications, and priority support.
                </p>
                
                <div className="bg-gradient-to-br from-blue-50 to-indigo-50 rounded-lg p-6 max-w-2xl mx-auto">
                  <h3 className="font-semibold text-lg text-blue-800 mb-3">What you&apos;ll get:</h3>
                  <div className="grid md:grid-cols-2 gap-3 text-sm text-blue-700">
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      5 wallets with 1-minute sync
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      5 contacts per wallet
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      Email & SMS notifications
                    </div>
                    <div className="flex items-center gap-2">
                      <div className="w-2 h-2 bg-blue-500 rounded-full"></div>
                      Push notifications via ntfy
                    </div>
                  </div>
                </div>
              </div>

              {/* Wallet Descriptor Education Section */}
              <div className="space-y-4 text-left max-w-2xl mx-auto">
                <h3 className="text-xl font-semibold text-center">First, you&apos;ll need an Output Descriptor</h3>
                <p className="text-muted-foreground">
                  To get started, you&apos;ll need an <span className="font-semibold text-foreground">output descriptor</span> from your existing Bitcoin wallet.
                </p>
                
                <div className="bg-muted/30 rounded-lg p-6 space-y-3">
                  <h4 className="font-semibold text-lg">What is an Output Descriptor?</h4>
                  <p className="text-muted-foreground">
                    An output descriptor is a standardized way to describe Bitcoin wallet addresses and scripts. 
                    It contains all the information needed to track your wallet&apos;s transactions without having access 
                    to your private keys—making it perfect for watch-only monitoring.
                  </p>
                </div>
                
                <div className="bg-muted/30 rounded-lg p-6 space-y-3">
                  <h4 className="font-semibold text-lg">How to Get Your Output Descriptor</h4>
                  <div className="space-y-2 text-muted-foreground">
                    <p className="font-medium text-foreground">From Sparrow Wallet:</p>
                    <ol className="list-decimal list-inside space-y-1 ml-2">
                      <li>Open your wallet in Sparrow</li>
                      <li>Go to Settings → Script Type</li>
                      <li>Click &quot;Show&quot; next to Output Descriptor</li>
                      <li>Copy the descriptor string</li>
                    </ol>
                    
                    <p className="font-medium text-foreground mt-4">From other wallets:</p>
                    <p>Most modern Bitcoin wallets support output descriptors. Look for options like &quot;Export&quot;, 
                       &quot;Wallet Information&quot;, or &quot;Watch-Only Export&quot; in your wallet&apos;s settings.</p>
                  </div>
                </div>
                
                <div className="bg-muted border border-border rounded-lg p-4 mt-6">
                  <p className="text-sm text-foreground">
                    <strong>Note:</strong> Canary is a watch-only service. It can monitor your wallet&apos;s transactions 
                    but cannot spend your Bitcoin. Your private keys remain secure in your wallet software.
                  </p>
                </div>
              </div>
              
              <div className="text-center pt-4">
                <Button
                  onClick={onAddWallet}
                  size="lg"
                  className="bg-blue-600 hover:bg-blue-700 text-white gap-2 text-lg px-8 py-3"
                >
                  <Plus size={20} />
                  Add Your First Wallet & Start Trial
                </Button>
                <p className="text-sm text-muted-foreground mt-3">
                  Trial starts when you add your wallet • No credit card required
                </p>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>
    )
  }
  
  // For non-pending users with no wallets (they already know what descriptors are)
  return (
    <div className="max-w-lg mx-auto mt-16">
      <Card className="border-muted">
        <CardContent className="pt-12 pb-10 px-8">
          <div className="text-center space-y-6">
            <div className="inline-flex items-center justify-center w-16 h-16 bg-accent/10 rounded-full mb-4">
              <Plus className="w-8 h-8 text-accent" />
            </div>
            
            <h2 className="text-2xl font-bold">
              No wallets yet
            </h2>
            
            <p className="text-muted-foreground">
              Add a wallet to start monitoring your Bitcoin transactions.
            </p>
            
            <div className="pt-4">
              <Button
                onClick={onAddWallet}
                size="lg"
                className="bg-accent hover:bg-accent/90 text-accent-foreground gap-2"
              >
                <Plus size={20} />
                Add Wallet
              </Button>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}