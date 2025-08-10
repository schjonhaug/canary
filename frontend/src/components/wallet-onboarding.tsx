"use client"

import { Card, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Plus, Info } from "lucide-react"

interface User {
  name?: string
  email: string
}

interface WalletOnboardingProps {
  onAddWallet: () => void
  user?: User | null
}

export function WalletOnboarding({ onAddWallet, user }: WalletOnboardingProps) {
  return (
    <div className="max-w-3xl mx-auto mt-16">
      <Card className="border-muted">
        <CardContent className="pt-12 pb-10 px-8">
          <div className="text-center space-y-6">
            <div className="inline-flex items-center justify-center w-16 h-16 bg-accent/10 rounded-full mb-4">
              <Info className="w-8 h-8 text-accent" />
            </div>
            
            <h2 className="text-3xl font-bold">
              Welcome to Canary{user?.name ? `, ${user.name}` : ''}
            </h2>
            
            <div className="space-y-4 text-left max-w-2xl mx-auto">
              <p className="text-lg text-muted-foreground">
                To get started, you&apos;ll need an <span className="font-semibold text-foreground">output descriptor</span> from your existing Bitcoin wallet.
              </p>
              
              <div className="bg-muted/30 rounded-lg p-6 space-y-3">
                <h3 className="font-semibold text-lg">What is an Output Descriptor?</h3>
                <p className="text-muted-foreground">
                  An output descriptor is a standardized way to describe Bitcoin wallet addresses and scripts. 
                  It contains all the information needed to track your wallet&apos;s transactions without having access 
                  to your private keys—making it perfect for watch-only monitoring.
                </p>
              </div>
              
              <div className="bg-muted/30 rounded-lg p-6 space-y-3">
                <h3 className="font-semibold text-lg">How to Get Your Output Descriptor</h3>
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
            
            <div className="pt-6">
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