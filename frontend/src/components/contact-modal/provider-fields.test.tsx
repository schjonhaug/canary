import { render, screen } from '@testing-library/react'

import { EmailProviderFields } from './email-provider-fields'
import { SmsProviderFields } from './sms-provider-fields'

const baseVerificationProps = {
  disabled: false,
  verificationRequired: true,
  verificationSent: true,
  verificationCode: '',
  onVerificationCodeChange: jest.fn(),
  isVerified: false,
  showSuccess: false,
  isSending: false,
  isVerifying: false,
  timeRemaining: 60,
  formatTime: (seconds: number) => `${seconds}s`,
  onSendVerification: jest.fn(),
  onVerifyCode: jest.fn(),
  onResendCode: jest.fn(),
}

describe('EmailProviderFields', () => {
  it('announces email and verification errors', () => {
    render(
      <EmailProviderFields
        {...baseVerificationProps}
        emailAddress="user@example.com"
        onEmailAddressChange={jest.fn()}
        emailPlaceholder="Email"
        emailError="Email is invalid"
        verificationAddress="user@example.com"
        verificationError="Email code is invalid"
      />
    )

    const alerts = screen.getAllByRole('alert')

    expect(alerts).toHaveLength(2)
    expect(alerts[0]).toHaveTextContent('Email is invalid')
    expect(alerts[1]).toHaveTextContent('Email code is invalid')
  })
})

describe('SmsProviderFields', () => {
  it('announces phone and verification errors', () => {
    render(
      <SmsProviderFields
        {...baseVerificationProps}
        phoneNumber="+4712345678"
        onPhoneNumberChange={jest.fn()}
        phonePlaceholder="Phone"
        phoneError="Phone is invalid"
        verificationPhone="+4712345678"
        verificationError="SMS code is invalid"
      />
    )

    const alerts = screen.getAllByRole('alert')

    expect(alerts).toHaveLength(2)
    expect(alerts[0]).toHaveTextContent('Phone is invalid')
    expect(alerts[1]).toHaveTextContent('SMS code is invalid')
  })
})
