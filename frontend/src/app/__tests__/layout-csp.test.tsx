import { Children, isValidElement, type ReactElement, type ReactNode } from 'react'
import RootLayout from '../layout'

const testNonce = 'strict-csp-test-nonce'

jest.mock('next/headers', () => ({
  headers: jest.fn(async () => new Headers({ 'x-nonce': testNonce })),
}))

jest.mock('next/font/google', () => ({
  Geist: () => ({ variable: '--font-geist-sans' }),
  Geist_Mono: () => ({ variable: '--font-geist-mono' }),
}))

jest.mock('next-intl/server', () => ({
  getLocale: jest.fn(async () => 'en-US'),
  getMessages: jest.fn(async () => ({})),
}))

function getElementChildren(node: ReactNode): ReactElement[] {
  if (!isValidElement(node)) {
    return []
  }

  return Children.toArray((node.props as { children?: ReactNode }).children)
    .filter(isValidElement)
}

describe('RootLayout CSP integration', () => {
  it('adds the request nonce to every inline script in the document head', async () => {
    const layout = await RootLayout({ children: <main>Canary</main> })
    const head = getElementChildren(layout).find((element) => element.type === 'head')
    const scripts = getElementChildren(head).filter((element) => element.type === 'script')

    expect(scripts).toHaveLength(2)
    expect(scripts.map((script) => script.props.nonce)).toEqual([testNonce, testNonce])
    expect(scripts[0].props.dangerouslySetInnerHTML.__html).toContain('canary-theme')
    expect(scripts[1].props.type).toBe('application/ld+json')
  })
})
