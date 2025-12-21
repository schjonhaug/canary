"use client"

import Link from "next/link"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { XCircle, ArrowLeft, MessageCircle } from "lucide-react"

export default function BillingCancelPage() {
  return (
    <div className="max-w-2xl mx-auto p-6 space-y-6">
      <Card>
        <CardHeader className="text-center">
          <div className="mx-auto w-16 h-16 rounded-full bg-orange-100 flex items-center justify-center mb-4">
            <XCircle className="h-8 w-8 text-orange-600" />
          </div>
          <CardTitle className="text-2xl">Payment Cancelled</CardTitle>
          <CardDescription>
            You cancelled the payment process. No charges have been made to your account.
          </CardDescription>
        </CardHeader>

        <CardContent className="space-y-6">
          {/* What happened */}
          <div className="bg-muted/50 rounded-lg p-4 space-y-2">
            <h3 className="font-semibold">What happened?</h3>
            <p className="text-sm text-muted-foreground">
              You closed the payment window or clicked the back button during checkout.
              Your subscription remains unchanged, and no payment was processed.
            </p>
          </div>

          {/* Options */}
          <div className="space-y-3">
            <h3 className="font-semibold">What would you like to do?</h3>
            <div className="grid gap-3">
              <Button asChild>
                <a href="/subscription">
                  <ArrowLeft className="mr-2 h-4 w-4" />
                  Try Again
                </a>
              </Button>

              <Button variant="outline" asChild>
                <Link href="/wallets">
                  Continue with Current Plan
                </Link>
              </Button>

              <Button variant="outline" asChild>
                <a href="mailto:support@canarybitcoin.com?subject=Billing Question&body=Hi, I was trying to upgrade my plan but cancelled the payment. Can you help me with...">
                  <MessageCircle className="mr-2 h-4 w-4" />
                  Contact Support
                </a>
              </Button>
            </div>
          </div>

          {/* Help section */}
          <div className="border-t pt-4 space-y-2">
            <h4 className="font-medium text-sm">Need help choosing a plan?</h4>
            <div className="text-sm text-muted-foreground space-y-1">
              <p>• <strong>Personal:</strong> Perfect for individual users managing their own Bitcoin</p>
              <p>• <strong>Team:</strong> Great for family guardians managing multiple wallets</p>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Support Info */}
      <Card>
        <CardContent className="pt-6">
          <div className="text-center text-sm text-muted-foreground space-y-1">
            <p>Questions about our plans? We&apos;re here to help!</p>
            <p>Email us at <a href="mailto:support@canarybitcoin.com" className="text-primary hover:underline">support@canarybitcoin.com</a></p>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
