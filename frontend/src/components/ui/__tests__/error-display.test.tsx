import { render, screen } from '@testing-library/react'

import { ErrorDisplay, FieldError, SuccessDisplay } from '../error-display'

describe('ErrorDisplay', () => {
  it('renders the card variant with the default title', () => {
    render(<ErrorDisplay message="Card failure" />)

    expect(screen.getByRole('alert')).toBeInTheDocument()
    expect(screen.getByText('Error')).toBeInTheDocument()
    expect(screen.getByText('Card failure')).toBeInTheDocument()
  })

  it('renders the card variant with an explicit title and description class', () => {
    render(
      <ErrorDisplay
        title="Could not load"
        message="Refresh the page"
        titleClassName="justify-center"
        descriptionClassName="text-center"
      />
    )

    expect(screen.getByText('Could not load')).toHaveClass('justify-center')
    expect(screen.getByText('Refresh the page')).toHaveClass('text-center')
  })

  it('renders inline errors without the default card title', () => {
    render(<ErrorDisplay message="Inline failure" variant="inline" />)

    expect(screen.getByRole('alert')).toHaveTextContent('Inline failure')
    expect(screen.queryByText('Error')).not.toBeInTheDocument()
  })

  it('renders explicit inline titles', () => {
    render(<ErrorDisplay title="Could not save" message="Try again" variant="inline" />)

    expect(screen.getByRole('alert')).toHaveTextContent('Could not save')
    expect(screen.getByRole('alert')).toHaveTextContent('Try again')
  })

  it('renders rich inline messages', () => {
    render(
      <ErrorDisplay
        message={
          <>
            Open <a href="/settings">settings</a>
          </>
        }
        variant="inline"
      />
    )

    expect(screen.getByRole('link', { name: 'settings' })).toHaveAttribute('href', '/settings')
  })
})

describe('FieldError', () => {
  it('renders field-level messages without alerting by default', () => {
    render(<FieldError message="Name is required" />)

    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.getByText('Name is required')).toBeInTheDocument()
  })

  it('can announce dynamic field-level messages', () => {
    render(<FieldError message="Name is required" className="mt-1" announce />)

    expect(screen.getByRole('alert')).toHaveTextContent('Name is required')
    expect(screen.getByRole('alert')).toHaveClass('mt-1')
  })
})

describe('SuccessDisplay', () => {
  it('renders success messages as polite status updates', () => {
    render(<SuccessDisplay message="Saved" />)

    expect(screen.getByRole('status')).toHaveTextContent('Saved')
  })

  it('renders rich success messages', () => {
    render(
      <SuccessDisplay
        message={
          <>
            Saved <a href="https://example.com/wallets">wallet</a>
          </>
        }
      />
    )

    expect(screen.getByRole('link', { name: 'wallet' })).toHaveAttribute('href', 'https://example.com/wallets')
  })

  it('renders compact success messages without the alert container', () => {
    render(<SuccessDisplay message="Verified" variant="compact" />)

    expect(screen.getByRole('status')).toHaveTextContent('Verified')
  })
})
