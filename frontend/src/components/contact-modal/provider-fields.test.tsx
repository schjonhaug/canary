import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { EmailProviderFields } from './email-provider-fields'
import { SmsProviderFields } from './sms-provider-fields'
import { validateWebhookUrl, WebhookProviderFields } from './webhook-provider-fields'

jest.mock('@/lib/api', () => ({
  api: {
    sendTestWebhookNotification: jest.fn(),
  },
}))

const mockApi = jest.requireMock('@/lib/api').api

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

describe('WebhookProviderFields', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  it('validates the same URL constraints as the backend', () => {
    expect(validateWebhookUrl('http://127.0.0.1:8080/hook')).toBe(true)
    expect(validateWebhookUrl('https://example.com/hook?token=secret')).toBe(true)
    expect(validateWebhookUrl('ftp://example.com/hook')).toBe(false)
    expect(validateWebhookUrl('https://user:secret@example.com/hook')).toBe(false)
    expect(validateWebhookUrl('https://example.com/hook#fragment')).toBe(false)
    expect(validateWebhookUrl('http:///missing-host')).toBe(false)
  })

  it('reports inline test success and failure', async () => {
    const user = userEvent.setup()
    mockApi.sendTestWebhookNotification
      .mockResolvedValueOnce({ success: true })
      .mockResolvedValueOnce({ success: false, error: 'HTTP 500' })
    const { rerender } = render(
      <WebhookProviderFields
        url="https://example.com/first"
        onUrlChange={jest.fn()}
      />
    )

    await user.click(screen.getByRole('button', { name: 'Test' }))
    expect(await screen.findByRole('status')).toHaveTextContent('Test webhook delivered successfully.')

    rerender(
      <WebhookProviderFields
        url="https://example.com/second"
        onUrlChange={jest.fn()}
      />
    )
    await user.click(screen.getByRole('button', { name: 'Test' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Test failed: HTTP 500')
  })
})
