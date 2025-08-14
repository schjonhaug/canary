import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import Image from 'next/image'
import Link from 'next/link'

export default function SignUpSuccessPage() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-gray-50 px-4">
      <Card className="w-full max-w-md">
        <CardHeader className="space-y-1">
          <div className="flex items-center justify-center mb-4">
            <Image
              src="/images/canary.svg"
              alt="Canary Logo"
              width={48}
              height={48}
              className="h-12 w-12"
            />
          </div>
          <CardTitle className="text-2xl font-bold text-center">
            Check your email
          </CardTitle>
          <CardDescription className="text-center">
            We&apos;ve sent you a verification link
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Alert>
            <AlertDescription>
              Registration successful! Please check your email to verify your account before signing in.
            </AlertDescription>
          </Alert>
          
          <div className="text-center space-y-2">
            <p className="text-sm text-gray-600">
              Didn&apos;t receive an email? Check your spam folder or contact support.
            </p>
          </div>

          <Link href="/sign-in" className="block">
            <Button className="w-full">
              Continue to Sign In
            </Button>
          </Link>
        </CardContent>
      </Card>
    </div>
  )
}