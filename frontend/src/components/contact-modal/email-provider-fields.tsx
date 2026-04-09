"use client"

import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Button } from "@/components/ui/button"
import { FieldError, SuccessDisplay } from "@/components/ui/error-display"
import { useTranslations } from "next-intl"

// Time threshold (in seconds) before resend is allowed
// With 10-minute (600s) verification expiry, allowing resend after 1 minute means checking for > 540s remaining
const RESEND_COOLDOWN_THRESHOLD = 540

interface EmailProviderFieldsProps {
  emailAddress: string
  onEmailAddressChange: (email: string) => void
  emailPlaceholder: string
  emailError: string | null
  disabled?: boolean
  hideEmailInput?: boolean

  // Verification state
  verificationRequired: boolean
  verificationSent: boolean
  verificationCode: string
  onVerificationCodeChange: (code: string) => void
  verificationAddress: string | null
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

export function EmailProviderFields({
  emailAddress,
  onEmailAddressChange,
  emailPlaceholder,
  emailError,
  disabled = false,
  hideEmailInput = false,
  verificationRequired,
  verificationSent,
  verificationCode,
  onVerificationCodeChange,
  verificationAddress,
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
}: EmailProviderFieldsProps) {
  const t = useTranslations('contacts')

  return (
    <div className="mt-2 space-y-3">
      {!hideEmailInput && (
        <div>
          <Input
            value={emailAddress}
            onChange={(e) => onEmailAddressChange(e.target.value)}
            placeholder={emailPlaceholder}
            disabled={disabled || isSending}
            type="email"
            enterKeyHint="next"
            className={emailError ? 'border-red-500 focus:border-red-500' : ''}
          />
          {emailError && (
            <FieldError message={emailError} className="mt-1" announce />
          )}
          {(!emailAddress || !isVerified) && !emailError && (
            <p className="text-xs text-muted-foreground mt-1">
              {t('add.email.emailHint')}
            </p>
          )}
        </div>
      )}

      {/* Send Verification Button */}
      {verificationRequired && !verificationSent && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onSendVerification}
          disabled={isSending || disabled || !emailAddress.trim()}
          className="w-full"
        >
          {isSending ? t('verification.sendingCode') : t('verification.sendCode')}
        </Button>
      )}

      {/* OTP Input Field */}
      {verificationSent && !isVerified && (
        <div className="space-y-3">
          <div>
            <Label htmlFor="email-verification-code">{t('verification.codeLabel')}</Label>
            <div className="flex gap-2">
              <Input
                id="email-verification-code"
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
              <FieldError message={verificationError} className="mt-1" announce />
            )}
          </div>
          <div className="flex justify-between items-center text-xs text-muted-foreground">
            <span>
              {t('verification.codeSentTo', { target: verificationAddress || '' })}
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
        <SuccessDisplay message={t('verification.emailVerified')} variant="compact" />
      )}
    </div>
  )
}
