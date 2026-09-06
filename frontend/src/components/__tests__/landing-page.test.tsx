import { render, screen } from '@testing-library/react'
import LandingPage from '../landing-page'
import { installOptions } from '@/lib/install-options'

describe('LandingPage', () => {
  beforeEach(() => {
    render(<LandingPage />)
  })

  it('presents self-hosted installation as the primary journey', () => {
    expect(screen.getByRole('heading', { level: 1, name: 'Know when your bitcoin moves.' })).toBeInTheDocument()
    expect(screen.getByText('Private Bitcoin monitoring')).toBeInTheDocument()
    expect(screen.getByText(/Self-hosting is the Bitcoin way/)).toBeInTheDocument()
    expect(screen.getAllByRole('link', { name: /Install Canary/ })[0]).toHaveAttribute('href', '#install')
    expect(screen.getAllByRole('link', { name: 'Use Canary Cloud' })[0]).toHaveAttribute('href', '/cloud')
    expect(screen.getAllByRole('link', { name: 'Try the demo' })[0]).toHaveAttribute('href', '/demo')
    expect(screen.getByRole('link', { name: /Donate/ })).toHaveAttribute('href', '/donations')
  })

  it('offers Cloud as a prominent path for people without a node', () => {
    expect(screen.getByText('Email alerts')).toBeInTheDocument()
    expect(screen.getByText('SMS alerts')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /No node\? Get email and SMS alerts instead/ })).toHaveAttribute('href', '/cloud')
    expect(screen.getByText('Do not have a node, or prefer not to run Canary yourself?')).toBeInTheDocument()
    expect(screen.getByText('No node required')).toBeInTheDocument()
  })

  it.each(installOptions)('renders the $name production install entry safely', (option) => {
    const link = screen.getByRole('link', { name: new RegExp(option.name, 'i') })

    expect(link).toHaveAttribute('href', option.url)
    expect(link).toHaveAttribute('target', '_blank')
    expect(link).toHaveAttribute('rel', expect.stringContaining('noopener'))
    expect(link).toHaveAttribute('rel', expect.stringContaining('noreferrer'))
  })

  it('states that single-sig and multisig wallets are supported', () => {
    expect(screen.getByText(/Single-sig and multisig wallets are both supported/)).toBeInTheDocument()
    expect(screen.getByText(/watch-only monitoring of single-sig and multisig wallets/)).toBeInTheDocument()
  })

  it('states the privacy boundaries precisely', () => {
    expect(screen.getByText('When self hosted, Canary stores descriptors, XPUBs, addresses, balances, and transaction history on infrastructure you control. A configured notification can still send selected event details to its delivery service.')).toBeInTheDocument()
    expect(screen.getByText('Canary never needs private keys or seed phrases and cannot authorize a Bitcoin transaction.')).toBeInTheDocument()
    expect(screen.getByText('Notification services can learn connection metadata and, depending on configuration, message content. ntfy servers, Nostr relays, and webhook operators each see different information.')).toBeInTheDocument()
  })

  it('does not render pricing, trial calls to action, or absolute privacy claims', () => {
    expect(screen.queryByText('Simple, Transparent Pricing')).not.toBeInTheDocument()
    expect(screen.queryByText(/Start 30-Day Free Trial/i)).not.toBeInTheDocument()
    expect(screen.queryByText('Complete privacy & control')).not.toBeInTheDocument()
  })
})
