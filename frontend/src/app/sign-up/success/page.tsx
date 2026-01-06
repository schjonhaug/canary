'use client'

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Alert, AlertDescription } from '@/components/ui/alert'
import { Button } from '@/components/ui/button'
import Image from 'next/image'
import Link from 'next/link'
import { useTranslations } from 'next-intl'

export default function SignUpSuccessPage() {
  const t = useTranslations('auth.signUpSuccess')
  const tCommon = useTranslations('common')

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
            {t('title')}
          </CardTitle>
          <CardDescription className="text-center">
            {t('description')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Alert>
            <AlertDescription>
              {t('description')}
            </AlertDescription>
          </Alert>

          <div className="text-center space-y-2">
            <p className="text-sm text-gray-600">
              {t('didntReceive')} {t('checkSpam')}
            </p>
          </div>

          <Link href="/sign-in" className="block">
            <Button className="w-full">
              {tCommon('backToSignIn')}
            </Button>
          </Link>
        </CardContent>
      </Card>
    </div>
  )
}
