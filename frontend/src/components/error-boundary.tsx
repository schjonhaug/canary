'use client'

import { Component, useEffect, useId, useRef, type ErrorInfo, type ReactNode } from 'react'
import { AlertTriangle } from 'lucide-react'
import { useTranslations } from 'next-intl'

import { Button } from '@/components/ui/button'
import type { ErrorBoundaryMessages } from '@/components/error-boundary-messages'

interface ErrorBoundaryProps {
  children: ReactNode
  reloadPage?: () => void
  messages?: ErrorBoundaryMessages
  onError?: (error: Error, errorInfo: ErrorInfo) => void
}

interface ErrorBoundaryState {
  hasError: boolean
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = {
    hasError: false,
  }

  static getDerivedStateFromError(): ErrorBoundaryState {
    return { hasError: true }
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('Unhandled React error caught by boundary', error, errorInfo)
    this.props.onError?.(error, errorInfo)
  }

  private handleReload = () => {
    if (this.props.reloadPage) {
      this.props.reloadPage()
      return
    }

    window.location.reload()
  }

  private handleReset = () => {
    // If children still throw, React will immediately return to the fallback.
    this.setState({ hasError: false })
  }

  render() {
    if (this.state.hasError) {
      if (this.props.messages) {
        return (
          <TranslatedErrorBoundaryFallback
            messages={this.props.messages}
            onReload={this.handleReload}
            onReset={this.handleReset}
          />
        )
      }

      return <LocalizedErrorBoundaryFallback onReload={this.handleReload} onReset={this.handleReset} />
    }

    return this.props.children
  }
}

interface ErrorBoundaryFallbackProps {
  onReload: () => void
  onReset: () => void
}

function LocalizedErrorBoundaryFallback({ onReload, onReset }: ErrorBoundaryFallbackProps) {
  const t = useTranslations('errorBoundary')

  return (
    <TranslatedErrorBoundaryFallback
      messages={{
        title: t('title'),
        description: t('description'),
        tryAgain: t('tryAgain'),
        reload: t('reload'),
      }}
      onReload={onReload}
      onReset={onReset}
    />
  )
}

interface TranslatedErrorBoundaryFallbackProps extends ErrorBoundaryFallbackProps {
  messages: ErrorBoundaryMessages
}

function TranslatedErrorBoundaryFallback({
  messages,
  onReload,
  onReset,
}: TranslatedErrorBoundaryFallbackProps) {
  const containerRef = useRef<HTMLElement>(null)
  const titleId = useId()

  useEffect(() => {
    containerRef.current?.focus()
  }, [])

  return (
    <main className="flex min-h-screen items-center justify-center bg-background px-6 py-16">
      <section
        ref={containerRef}
        role="alert"
        aria-labelledby={titleId}
        tabIndex={-1}
        className="w-full max-w-md rounded-lg border bg-card p-6 text-card-foreground shadow-sm"
      >
        <div className="flex items-start gap-3">
          <AlertTriangle className="mt-0.5 h-6 w-6 shrink-0 text-destructive" aria-hidden="true" />
          <div className="space-y-4">
            <div className="space-y-2">
              <h2 id={titleId} className="text-xl font-semibold">
                {messages.title}
              </h2>
              <p className="text-sm text-muted-foreground">
                {messages.description}
              </p>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button type="button" onClick={onReset}>
                {messages.tryAgain}
              </Button>
              <Button type="button" variant="outline" onClick={onReload}>
                {messages.reload}
              </Button>
            </div>
          </div>
        </div>
      </section>
    </main>
  )
}
