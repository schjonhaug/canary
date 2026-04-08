import React from 'react'
import { render, screen } from '@testing-library/react'
import { SUPPORT_EMAIL } from '@/lib/constants'
import BillingCancelPage from '../cancel'

jest.mock('next-intl', () => {
  const React = require('react')
  const translations = require('../../../../messages/en-US.json')

  function getNestedValue(obj: unknown, path: string) {
    return path.split('.').reduce((current: Record<string, unknown> | undefined, key) => {
      return current && current[key] !== undefined ? current[key] : undefined
    }, obj as Record<string, unknown> | undefined)
  }

  function renderRichText(value: string, params: Record<string, unknown> = {}) {
    const plainParams = Object.fromEntries(
      Object.entries(params).filter(([, v]) => typeof v !== 'function')
    )

    let interpolated = value
    Object.entries(plainParams).forEach(([k, v]) => {
      interpolated = interpolated.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v))
    })

    const tagRegex = /<(\w+)>([\s\S]*?)<\/\1>/g
    const parts: Array<string | React.ReactElement> = []
    let lastIndex = 0
    let match: RegExpExecArray | null

    while ((match = tagRegex.exec(interpolated)) !== null) {
      const [fullMatch, tagName, content] = match
      if (match.index > lastIndex) {
        parts.push(interpolated.slice(lastIndex, match.index))
      }

      const formatter = params[tagName]
      parts.push(typeof formatter === 'function' ? formatter(content) : fullMatch)
      lastIndex = match.index + fullMatch.length
    }

    if (lastIndex < interpolated.length) {
      parts.push(interpolated.slice(lastIndex))
    }

    if (parts.length === 0) return interpolated
    if (parts.length === 1) return parts[0]

    return React.createElement(
      React.Fragment,
      null,
      ...parts.map((part: string | React.ReactElement, index: number) => (
        React.isValidElement(part) ? React.cloneElement(part, { key: index }) : part
      ))
    )
  }

  return {
    useTranslations: (namespace: string) => {
      const namespaceData = namespace ? getNestedValue(translations, namespace) : translations

      const t = (key: string, params?: Record<string, unknown>) => {
        let value = getNestedValue(namespaceData, key)

        if (value === undefined) {
          value = getNestedValue(translations, `${namespace}.${key}`)
        }

        if (value === undefined) {
          return namespace ? `${namespace}.${key}` : key
        }

        if (params && typeof value === 'string') {
          Object.entries(params).forEach(([k, v]) => {
            value = value.replace(new RegExp(`\\{${k}\\}`, 'g'), String(v))
          })
        }

        return value
      }

      t.rich = (key: string, params?: Record<string, unknown>) => {
        let value = getNestedValue(namespaceData, key)

        if (value === undefined) {
          value = getNestedValue(translations, `${namespace}.${key}`)
        }

        if (value === undefined) {
          return namespace ? `${namespace}.${key}` : key
        }

        if (params && typeof value === 'string') {
          return renderRichText(value, params)
        }

        return value
      }

      return t
    },
  }
})

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
