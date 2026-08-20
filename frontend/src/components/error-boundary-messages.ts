export interface ErrorBoundaryMessages {
  title: string
  description: string
  tryAgain: string
  reload: string
}

export const staticErrorBoundaryMessages: ErrorBoundaryMessages = {
  title: 'Something went wrong',
  description: 'Canary Wallet hit an unexpected problem. Try again, or reload the page to recover.',
  tryAgain: 'Try again',
  reload: 'Reload',
}
