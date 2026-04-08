import React from 'react'
import { render, screen } from '@testing-library/react'
import BillingCancelPage from '../cancel'
import { SUPPORT_EMAIL } from '@/lib/constants'

describe('BillingCancelPage', () => {
  it('renders the cancellation UI and plan descriptions', () => {
    render(<BillingCancelPage />)

    expect(screen.getByText('Payment Cancelled')).toBeInTheDocument()
    expect(screen.getByText('You cancelled the payment process. No charges have been made to your account.')).toBeInTheDocument()
    expect(screen.getByText('What happened?')).toBeInTheDocument()
    expect(screen.getByText('You closed the payment window or clicked the back button during checkout. Your subscription remains unchanged, and no payment was processed.')).toBeInTheDocument()
    expect(screen.getByText('What would you like to do?')).toBeInTheDocument()
    expect(screen.getByText('Need help choosing a plan?')).toBeInTheDocument()
    expect(screen.getByText("Questions about our plans? We're here to help!")).toBeInTheDocument()

    expect(screen.getByText('Personal:')).toHaveProperty('tagName', 'STRONG')
    expect(screen.getByText('Team:')).toHaveProperty('tagName', 'STRONG')
    expect(screen.getByText(/Perfect for individual users managing their own Bitcoin/)).toBeInTheDocument()
    expect(screen.getByText(/Great for family guardians managing multiple wallets/)).toBeInTheDocument()
  })

  it('renders the navigation and support links with the expected href values', () => {
    render(<BillingCancelPage />)

    expect(screen.getByRole('link', { name: 'Try Again' })).toHaveAttribute('href', '/subscription')
    expect(screen.getByRole('link', { name: 'Continue with Current Plan' })).toHaveAttribute('href', '/wallets')

    const subject = encodeURIComponent('Billing Question')
    const body = encodeURIComponent('Hi, I was trying to upgrade my plan but cancelled the payment. Can you help me with...')
    expect(screen.getByRole('link', { name: 'Contact Support' })).toHaveAttribute(
      'href',
      `mailto:${SUPPORT_EMAIL}?subject=${subject}&body=${body}`
    )

    const supportEmailLink = screen.getByRole('link', { name: SUPPORT_EMAIL })
    expect(supportEmailLink).toHaveAttribute('href', `mailto:${SUPPORT_EMAIL}`)
    expect(supportEmailLink).toHaveTextContent(SUPPORT_EMAIL)
  })
})
