import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'

import { ErrorBoundary } from '../error-boundary'

function ThrowingComponent() {
  throw new Error('Test render failure')
}

describe('ErrorBoundary', () => {
  let consoleErrorSpy: jest.SpyInstance

  beforeEach(() => {
    consoleErrorSpy = jest.spyOn(console, 'error').mockImplementation(() => {})
  })

  afterEach(() => {
    consoleErrorSpy.mockRestore()
  })

  it('renders children when there is no error', () => {
    render(
      <ErrorBoundary>
        <div>Working app</div>
      </ErrorBoundary>
    )

    expect(screen.getByText('Working app')).toBeInTheDocument()
  })

  it('renders a recovery message when a child throws', () => {
    render(
      <ErrorBoundary>
        <ThrowingComponent />
      </ErrorBoundary>
    )

    expect(screen.getByRole('heading', { name: 'Something went wrong' })).toBeInTheDocument()
    expect(screen.getByText('Canary hit an unexpected problem. Try again, or reload the page to recover.')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Reload' })).toBeInTheDocument()
  })

  it('renders static messages when messages are provided directly', () => {
    render(
      <ErrorBoundary
        messages={{
          title: 'Static title',
          description: 'Static description',
          tryAgain: 'Static retry',
          reload: 'Static reload',
        }}
      >
        <ThrowingComponent />
      </ErrorBoundary>
    )

    expect(screen.getByRole('heading', { name: 'Static title' })).toBeInTheDocument()
    expect(screen.getByText('Static description')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Static retry' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Static reload' })).toBeInTheDocument()
  })

  it('moves focus to the recovery message when a child throws', async () => {
    render(
      <ErrorBoundary>
        <ThrowingComponent />
      </ErrorBoundary>
    )

    await waitFor(() => expect(screen.getByRole('alert')).toHaveFocus())
  })

  it('can reset after a recoverable error', async () => {
    const user = userEvent.setup()
    let shouldThrow = true
    function RecoverableComponent() {
      if (shouldThrow) {
        throw new Error('Recoverable render failure')
      }

      return <div>Recovered app</div>
    }

    render(
      <ErrorBoundary>
        <RecoverableComponent />
      </ErrorBoundary>
    )

    shouldThrow = false
    await user.click(screen.getByRole('button', { name: 'Try again' }))

    expect(screen.getByText('Recovered app')).toBeInTheDocument()
  })

  it('reloads the page from the recovery action', async () => {
    const user = userEvent.setup()
    const reloadPage = jest.fn()

    render(
      <ErrorBoundary reloadPage={reloadPage}>
        <ThrowingComponent />
      </ErrorBoundary>
    )

    await user.click(screen.getByRole('button', { name: 'Reload' }))

    expect(reloadPage).toHaveBeenCalledTimes(1)
  })

  it('calls the error callback when a child throws', () => {
    const onError = jest.fn()

    render(
      <ErrorBoundary onError={onError}>
        <ThrowingComponent />
      </ErrorBoundary>
    )

    expect(onError).toHaveBeenCalledTimes(1)
    expect(onError.mock.calls[0][0]).toBeInstanceOf(Error)
  })
})
