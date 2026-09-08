import React from 'react'
import { act, fireEvent, render, screen } from '@testing-library/react'
import { ContactForm } from '../contact-form'
import { MESSAGE_CONSTRAINTS } from '@/lib/constants'
import messages from '../../../messages/en-US.json'

jest.mock('@/contexts/auth-context', () => ({ useAuth: jest.fn() }))
jest.mock('@/lib/api', () => {
  const actual = jest.requireActual('@/lib/api')
  return { ApiError: actual.ApiError, api: { submitContactForm: jest.fn() } }
})
const mockUseAuth = jest.requireMock('@/contexts/auth-context').useAuth
const mockSubmit = jest.requireMock('@/lib/api').api.submitContactForm
const prefix = '[Canary Private enquiry]\n\n'
const email = 'visitor@example.com'
const message = 'I would like monitoring for my family.'
function fillForm(body = message, address = email) {
  fireEvent.change(screen.getByLabelText('Email'), { target: { value: address } })
  fireEvent.change(screen.getByLabelText('Message'), { target: { value: body } })
}
function submitForm() {
  fireEvent.submit(screen.getByLabelText('Message').closest('form')!)
}
beforeEach(() => {
  jest.resetAllMocks()
  mockUseAuth.mockReturnValue({ user: null, isAuthenticated: false })
  mockSubmit.mockResolvedValue({ message: 'Your message has been sent.' })
})
describe('ContactForm', () => {
  it('prefills the signed-in email and allows editing it', () => {
    mockUseAuth.mockReturnValue({ user: { email }, isAuthenticated: true })
    render(<ContactForm />)
    expect(screen.getByLabelText('Email')).toHaveValue(email)
    fireEvent.change(screen.getByLabelText('Email'), { target: { value: 'other@example.com' } })
    expect(screen.getByLabelText('Email')).toHaveValue('other@example.com')
  })
  it('preserves ordinary contact copy, untagged delivery, and server confirmation', async () => {
    render(<ContactForm />)
    expect(screen.getByText('Get in Touch')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('How can we help you?')).toBeInTheDocument()
    fillForm()
    fireEvent.click(screen.getByRole('button', { name: 'Send Message' }))
    expect(mockSubmit).toHaveBeenCalledWith(email, message)
    expect(await screen.findByRole('status')).toHaveTextContent('Your message has been sent.')
    expect(screen.getByLabelText('Message')).toHaveValue('')
    expect(screen.getByLabelText('Email')).toHaveValue(email)
  })
  it('tags Private enquiries, trims input, and uses localized confirmation', async () => {
    const copy = messages.privatePage.form
    render(<ContactForm messagePrefix={prefix} title={copy.title} description={copy.description}
      placeholder={copy.placeholder} successMessage={copy.success} />)
    expect(screen.getByText(copy.title)).toBeInTheDocument()
    expect(screen.getByLabelText('Message')).toHaveAccessibleDescription(expect.stringContaining(copy.description))
    expect(screen.getByPlaceholderText(copy.placeholder)).toBeInTheDocument()
    fillForm(`  ${message}  `, `  ${email}  `)
    submitForm()
    expect(mockSubmit).toHaveBeenCalledWith(email, prefix + message)
    expect(await screen.findByRole('status')).toHaveTextContent(copy.success)
    expect(screen.queryByText('Your message has been sent.')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Message')).toHaveValue('')
    expect(screen.getByLabelText('Email')).toHaveValue(email)
  })
  it.each(['', prefix])('rejects whitespace-only messages with prefix %j', (messagePrefix) => {
    render(<ContactForm messagePrefix={messagePrefix} />)
    fillForm('   ')
    submitForm()
    expect(screen.getByRole('alert')).toHaveTextContent('Message is required')
    expect(mockSubmit).not.toHaveBeenCalled()
  })
  it('requires a valid email before submitting', () => {
    render(<ContactForm />)
    fillForm(message, '')
    submitForm()
    expect(screen.getByRole('alert')).toHaveTextContent('Email is required')
    fillForm(message, 'invalid-address')
    submitForm()
    expect(screen.getByRole('alert')).toHaveTextContent('Please enter a valid email address')
    expect(mockSubmit).not.toHaveBeenCalled()
  })
  it('does not count the prefix toward the minimum message length', async () => {
    render(<ContactForm messagePrefix={prefix} />)
    fillForm('x'.repeat(MESSAGE_CONSTRAINTS.MIN_LENGTH - 1))
    submitForm()
    expect(screen.getByRole('alert')).toHaveTextContent(`Message must be at least ${MESSAGE_CONSTRAINTS.MIN_LENGTH} characters`)
    expect(mockSubmit).not.toHaveBeenCalled()
    const minimum = 'x'.repeat(MESSAGE_CONSTRAINTS.MIN_LENGTH)
    fillForm(minimum)
    submitForm()
    expect(mockSubmit).toHaveBeenCalledWith(email, prefix + minimum)
    await screen.findByRole('status')
  })
  it.each(['', prefix])('enforces the delivery limit including prefix %j', async (messagePrefix) => {
    render(<ContactForm messagePrefix={messagePrefix} />)
    const maximum = MESSAGE_CONSTRAINTS.MAX_LENGTH - messagePrefix.length
    fillForm('x'.repeat(maximum + 1))
    submitForm()
    expect(screen.getByRole('alert')).toHaveTextContent(`Message must be less than ${maximum} characters`)
    expect(mockSubmit).not.toHaveBeenCalled()
    const boundary = 'x'.repeat(maximum)
    fillForm(boundary)
    submitForm()
    expect(mockSubmit).toHaveBeenCalledWith(email, messagePrefix + boundary)
    await screen.findByRole('status')
  })
  it('disables fields and prevents duplicate submissions until delivery settles', async () => {
    let resolveDelivery!: (response: { message: string }) => void
    mockSubmit.mockReturnValue(new Promise((resolve) => { resolveDelivery = resolve }))
    render(<ContactForm messagePrefix={prefix} />)
    fillForm()
    act(() => { submitForm(); submitForm() })
    expect(mockSubmit).toHaveBeenCalledTimes(1)
    expect(screen.getByRole('button', { name: 'Sending...' })).toBeDisabled()
    expect(screen.getByLabelText('Email')).toBeDisabled()
    expect(screen.getByLabelText('Message')).toBeDisabled()
    await act(async () => { resolveDelivery({ message: 'Delivered' }) })
    expect(screen.getByRole('status')).toHaveTextContent('Delivered')
    expect(screen.getByLabelText('Email')).toBeEnabled()
    expect(screen.getByLabelText('Message')).toBeEnabled()
  })
  it('retains entered content after failure and permits retry', async () => {
    mockSubmit.mockRejectedValueOnce(new Error('Delivery unavailable'))
    render(<ContactForm messagePrefix={prefix} />)
    fillForm()
    submitForm()
    expect(await screen.findByRole('alert')).toHaveTextContent('Delivery unavailable')
    expect(screen.getByLabelText('Email')).toHaveValue(email)
    expect(screen.getByLabelText('Message')).toHaveValue(message)
    expect(screen.getByRole('button', { name: 'Send Message' })).toBeEnabled()
    submitForm()
    expect(mockSubmit).toHaveBeenCalledTimes(2)
    expect(mockSubmit).toHaveBeenLastCalledWith(email, prefix + message)
    expect(await screen.findByRole('status')).toHaveTextContent('Your message has been sent.')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByLabelText('Message')).toHaveValue('')
  })
})
