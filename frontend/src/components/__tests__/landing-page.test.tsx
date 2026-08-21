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
    expect(screen.getAllByRole('link', { name: /Install Canary/ })[0]).toHaveAttribute('href', '#install')
    expect(screen.getByRole('link', { name: 'Do not run a node? Explore Canary Cloud and its privacy tradeoffs.' })).toHaveAttribute('href', '/cloud')
    expect(screen.getByRole('link', { name: /Donate/ })).toHaveAttribute('href', '/donations')
  })

  it.each(installOptions)('renders the $name production install entry safely', (option) => {
    const link = screen.getByRole('link', { name: new RegExp(option.name, 'i') })

    expect(link).toHaveAttribute('href', option.url)
    expect(link).toHaveAttribute('target', '_blank')
    expect(link).toHaveAttribute('rel', expect.stringContaining('noopener'))
    expect(link).toHaveAttribute('rel', expect.stringContaining('noreferrer'))
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
