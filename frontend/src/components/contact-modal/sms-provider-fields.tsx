"use client"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { useTranslations } from "next-intl"
import { parsePhoneNumberFromString } from "libphonenumber-js"

// Time threshold (in seconds) before resend is allowed
// With 10-minute (600s) verification expiry, allowing resend after 1 minute means checking for > 540s remaining
const RESEND_COOLDOWN_THRESHOLD = 540

interface SmsProviderFieldsProps {
  phoneNumber: string
  onPhoneNumberChange: (phone: string) => void
  phonePlaceholder: string
  phoneError: string | null
  disabled?: boolean

  // Verification state
  verificationRequired: boolean
  verificationSent: boolean
  verificationCode: string
  onVerificationCodeChange: (code: string) => void
  verificationPhone: string | null
  verificationError: string | null
  isVerified: boolean
  showSuccess: boolean
  isSending: boolean
  isVerifying: boolean
  timeRemaining: number
  formatTime: (seconds: number) => string

  // Actions
  onSendVerification: () => void
  onVerifyCode: () => void
  onResendCode: () => void
}

export function SmsProviderFields({
  phoneNumber,
  onPhoneNumberChange,
  phonePlaceholder,
  phoneError,
  disabled = false,
  verificationRequired,
  verificationSent,
  verificationCode,
  onVerificationCodeChange,
  verificationPhone,
  verificationError,
  isVerified,
  showSuccess,
  isSending,
  isVerifying,
  timeRemaining,
  formatTime,
  onSendVerification,
  onVerifyCode,
  onResendCode
}: SmsProviderFieldsProps) {
  const t = useTranslations('contacts')

  const formattedPhone = verificationPhone
    ? (parsePhoneNumberFromString(verificationPhone)?.formatInternational() ?? verificationPhone)
    : ''

  return (
    <div className="mt-2 space-y-3">
      <div>
        <Input
          value={phoneNumber}
          onChange={(e) => onPhoneNumberChange(e.target.value)}
          placeholder={phonePlaceholder}
          disabled={disabled || isSending}
          className={phoneError ? 'border-red-500 focus:border-red-500' : ''}
        />
        {phoneError && (
          <div className="text-sm text-red-600 mt-1">
            {phoneError}
          </div>
        )}
        {(!phoneNumber || !isVerified) && !phoneError && (
          <p className="text-xs text-muted-foreground mt-1">
            {t('add.sms.phoneHint')}
          </p>
        )}
      </div>

      {/* Send Verification Button */}
      {verificationRequired && !verificationSent && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onSendVerification}
          disabled={isSending || disabled || !phoneNumber.trim()}
          className="w-full"
        >
          {isSending ? t('verification.sendingCode') : t('verification.sendCode')}
        </Button>
      )}

      {/* OTP Input Field */}
      {verificationSent && !isVerified && (
        <div className="space-y-3">
          <div>
            <Label htmlFor="sms-verification-code">{t('verification.codeLabel')}</Label>
            <div className="flex gap-2">
              <Input
                id="sms-verification-code"
                value={verificationCode}
                onChange={(e) => onVerificationCodeChange(e.target.value)}
                placeholder={t('verification.codePlaceholder')}
                disabled={disabled || isVerifying}
                maxLength={6}
                autoComplete="one-time-code"
                autoCorrect="off"
                autoCapitalize="off"
                spellCheck="false"
                inputMode="numeric"
                className={`flex-1 ${verificationError ? 'border-red-500 focus:border-red-500' : ''}`}
              />
              <Button
                type="button"
                variant="outline"
                onClick={onVerifyCode}
                disabled={!verificationCode.trim() || isVerifying || disabled}
              >
                {isVerifying ? t('verification.verifying') : t('verification.verify')}
              </Button>
            </div>
            {verificationError && (
              <div className="text-sm text-red-600 mt-1">
                {verificationError}
              </div>
            )}
          </div>
          <div className="flex justify-between items-center text-xs text-muted-foreground">
            <span>
              {t('verification.codeSentTo', { target: formattedPhone })}
              {timeRemaining > 0 && (
                <span className="block">{t('verification.expiresIn', { time: formatTime(timeRemaining) })}</span>
              )}
            </span>
            <button
              type="button"
              onClick={onResendCode}
              disabled={isSending || timeRemaining > RESEND_COOLDOWN_THRESHOLD}
              className="text-blue-600 hover:text-blue-800 disabled:text-gray-400 underline"
            >
              {t('verification.resend')}
            </button>
          </div>
        </div>
      )}

      {/* Verification Success */}
      {showSuccess && (
        <div className="flex items-center gap-2 text-green-600 text-sm">
          <svg className="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
          </svg>
          {t('verification.smsVerified')}
        </div>
      )}
    </div>
  )
}
