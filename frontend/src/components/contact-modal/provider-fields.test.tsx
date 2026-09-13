import { act, render, screen } from '@testing-library/react'
import { useState } from 'react'
import userEvent from '@testing-library/user-event'

import { EmailProviderFields } from './email-provider-fields'
import { SmsProviderFields } from './sms-provider-fields'
import { NtfyProviderFields, ntfyTopicPrivacyHintKey } from './ntfy-provider-fields'
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
    expect(await screen.findByRole('status')).toHaveTextContent('Test webhook delivered. This checks that the URL is reachable.')

    rerender(
      <WebhookProviderFields
        url="https://example.com/second"
        onUrlChange={jest.fn()}
      />
    )
    await user.click(screen.getByRole('button', { name: 'Test' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Test failed: HTTP 500')
  })

  it('discards a test result when the URL changes before the request finishes', async () => {
    const user = userEvent.setup()
    let resolveRequest: (value: { success: boolean }) => void = () => undefined
    mockApi.sendTestWebhookNotification.mockReturnValue(
      new Promise((resolve) => {
        resolveRequest = resolve
      })
    )

    function Harness() {
      const [url, setUrl] = useState('https://example.com/first')
      return <WebhookProviderFields url={url} onUrlChange={setUrl} />
    }

    render(<Harness />)
    await user.click(screen.getByRole('button', { name: 'Test' }))
    const input = screen.getByLabelText('Webhook URL')
    await user.clear(input)
    await user.type(input, 'https://example.com/second')
    await act(async () => resolveRequest({ success: true }))

    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('ntfyTopicPrivacyHintKey', () => {
  it('uses generated copy only for Canary 128-bit topics', () => {
    expect(ntfyTopicPrivacyHintKey('canary-0123456789abcdef0123456789abcdef')).toBe('topicPrivacyGenerated')
    expect(ntfyTopicPrivacyHintKey('canary-0123456789ABCDEF0123456789ABCDEF')).toBe('topicPrivacyCustom')
    expect(ntfyTopicPrivacyHintKey('alice-private-topic')).toBe('topicPrivacyCustom')
    expect(ntfyTopicPrivacyHintKey('')).toBe('topicPrivacyCustom')
  })

  it('does not claim Canary generated a managed default topic', () => {
    expect(
      ntfyTopicPrivacyHintKey('canary-0123456789abcdef0123456789abcdef', 'canary-0123456789abcdef0123456789abcdef')
    ).toBe('topicPrivacyCustom')
    expect(ntfyTopicPrivacyHintKey('canary', 'canary')).toBe('topicPrivacyCustom')
  })
})

describe('NtfyProviderFields', () => {
  it('explains that generated topics are editable and hard to guess', () => {
    render(
      <NtfyProviderFields
        topic="canary-0123456789abcdef0123456789abcdef"
        onTopicChange={jest.fn()}
        defaultTopicPlaceholder="canary-0123456789abcdef0123456789abcdef"
        ntfyServerUrl="https://ntfy.sh"
      />
    )

    expect(screen.getByText(/You can change this topic\. We generated a hard-to-guess name/)).toBeInTheDocument()
    expect(screen.getByText(/Enter the topic name only, not a URL\. Notifications go to ntfy\.sh/)).toBeInTheDocument()
    expect(screen.queryByText(/We generated a hard-to-guess name/)).toBeInTheDocument()
  })

  it('does not claim Canary generated managed or custom topics', () => {
    const { rerender } = render(
      <NtfyProviderFields
        topic="canary"
        onTopicChange={jest.fn()}
        defaultTopicPlaceholder="canary"
        ntfyServerUrl="http://localhost:2586"
        ntfyServerIsBrowserSafe={false}
        managedDefaultTopic="canary"
      />
    )

    expect(screen.getByText(/You can change this topic\. Choose a hard-to-guess name/)).toBeInTheDocument()
    expect(screen.queryByText(/We generated a hard-to-guess name/)).not.toBeInTheDocument()
    expect(screen.getByText(/Notifications go to local ntfy/)).toBeInTheDocument()

    rerender(
      <NtfyProviderFields
        topic="alice-custom-topic"
        onTopicChange={jest.fn()}
        defaultTopicPlaceholder="canary"
        ntfyServerUrl="https://ntfy.sh"
        managedDefaultTopic="canary"
      />
    )

    expect(screen.getByText(/You can change this topic\. Choose a hard-to-guess name/)).toBeInTheDocument()
    expect(screen.queryByText(/We generated a hard-to-guess name/)).not.toBeInTheDocument()
  })

  it('switches to custom copy after the generated topic is edited', async () => {
    const user = userEvent.setup()
    function Harness() {
      const [topic, setTopic] = useState('canary-0123456789abcdef0123456789abcdef')
      return (
        <NtfyProviderFields
          topic={topic}
          onTopicChange={setTopic}
          defaultTopicPlaceholder="canary-0123456789abcdef0123456789abcdef"
          ntfyServerUrl="https://ntfy.sh"
        />
      )
    }

    render(<Harness />)
    expect(screen.getByText(/We generated a hard-to-guess name/)).toBeInTheDocument()
    await user.clear(screen.getByLabelText('ntfy Topic'))
    await user.type(screen.getByLabelText('ntfy Topic'), 'desk-alerts')
    expect(screen.getByText(/You can change this topic\. Choose a hard-to-guess name/)).toBeInTheDocument()
    expect(screen.queryByText(/We generated a hard-to-guess name/)).not.toBeInTheDocument()
  })
})
