import { render, screen } from '@testing-library/react'
import PrivatePage, { metadata } from '@/app/private/page'
import LandingPage from '../landing-page'
import CloudPage from '../cloud-page'
import { privateOffer } from '@/lib/private-offer'
import fs from 'fs'
import path from 'path'

jest.mock('@/contexts/auth-context', () => ({ useAuth: () => ({ user: null, isAuthenticated: false }) }))
jest.mock('next/navigation', () => ({ notFound: () => { throw new Error('NEXT_NOT_FOUND') } }))
jest.mock('../plan-comparison', () => ({ PlanComparison: () => <p>Pricing unavailable</p> }))

afterEach(() => { process.env.NEXT_PUBLIC_CANARY_MODE = 'cloud' })

it('serves the public page with enquiry anchors and dedicated metadata', () => {
  render(<PrivatePage />)
  expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Your own Bitcoin node. Hosted for you.')
  expect(screen.getByText(`From ${privateOffer.startingPrice}/month`)).toBeInTheDocument()
  screen.getAllByRole('link', { name: 'Contact us' }).forEach(link => expect(link).toHaveAttribute('href', '#enquiry'))
  expect(document.getElementById('enquiry')).toContainElement(screen.getByLabelText('Message'))
  expect(metadata.alternates?.canonical).toBe('https://canarybitcoin.com/private')
  expect(metadata.openGraph).toMatchObject({ url: 'https://canarybitcoin.com/private' })
  expect(metadata.twitter).toMatchObject({ card: 'summary_large_image' })
  expect(fs.readFileSync(path.join(process.cwd(), 'public/sitemap.xml'), 'utf8')).toContain('https://canarybitcoin.com/private')
})

it('returns not-found in self-hosted mode', () => {
  process.env.NEXT_PUBLIC_CANARY_MODE = 'self-hosted'
  expect(() => PrivatePage()).toThrow('NEXT_NOT_FOUND')
})

it.each([['home', LandingPage], ['cloud', CloudPage]])('promotes Private on %s even without Cloud pricing', (_, Page) => {
  render(<Page />)
  expect(screen.getByRole('link', { name: 'Explore Canary Private' })).toHaveAttribute('href', '/private')
  expect(screen.getByText(`From ${privateOffer.startingPrice}/month`)).toBeInTheDocument()
  expect(screen.getAllByRole('link', { name: 'Private' }).length).toBeGreaterThan(0)
  if (Page === CloudPage) {
    const promotion = screen.getByRole('region', { name: 'Canary Private' })
    expect(screen.getByText('Pricing unavailable').compareDocumentPosition(promotion) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(promotion.compareDocumentPosition(screen.getByRole('heading', { name: 'Privacy questions' })) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  }
})

it('has matching translated keys and price placeholders across all nine locales', () => {
  const dir = path.join(process.cwd(), 'messages')
  const flatten = (value: Record<string, unknown>, prefix = ''): Record<string, string> => Object.fromEntries(Object.entries(value).flatMap(([key, item]) => typeof item === 'string' ? [[prefix + key, item]] : Object.entries(flatten(item as Record<string, unknown>, `${prefix}${key}.`))))
  const source = flatten(JSON.parse(fs.readFileSync(path.join(dir, 'en-US.json'), 'utf8')).privatePage)
  const locales = fs.readdirSync(dir).filter(file => file.endsWith('.json'))
  expect(locales).toHaveLength(9)
  locales.forEach(file => {
    const translated = flatten(JSON.parse(fs.readFileSync(path.join(dir, file), 'utf8')).privatePage)
    expect(Object.keys(translated)).toEqual(Object.keys(source))
    Object.entries(translated).forEach(([key, value]) => {
      expect(value.trim()).not.toBe('')
      expect(value.match(/\{\w+\}/g)).toEqual(source[key].match(/\{\w+\}/g))
    })
  })
})
