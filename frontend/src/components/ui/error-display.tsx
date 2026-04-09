import type { ReactNode } from "react"
import { Card, CardDescription, CardHeader, CardTitle } from "./card"
import { AlertTriangle, CheckCircle, XCircle } from "lucide-react"
import { Alert, AlertDescription } from "./alert"
import { cn } from "@/lib/utils"

interface ErrorDisplayProps {
  title?: string
  message: ReactNode
  variant?: 'card' | 'inline'
  className?: string
  titleClassName?: string
  descriptionClassName?: string
}

export function ErrorDisplay({
  title,
  message,
  variant = 'card',
  className = "",
  titleClassName = "",
  descriptionClassName = ""
}: ErrorDisplayProps) {
  if (variant === 'inline') {
    return (
      <Alert variant="destructive" className={className}>
        <XCircle className="h-4 w-4" />
        <AlertDescription className={descriptionClassName}>
          {title && <span className="block font-medium">{title}</span>}
          {message}
        </AlertDescription>
      </Alert>
    )
  }

  return (
    <Card role="alert" className={className}>
      <CardHeader>
        <CardTitle className={cn("flex items-center gap-2 text-destructive", titleClassName)}>
          <AlertTriangle className="h-5 w-5" />
          {title ?? "Error"}
        </CardTitle>
        <CardDescription className={cn("text-destructive", descriptionClassName)}>
          {message}
        </CardDescription>
      </CardHeader>
    </Card>
  )
}

interface SuccessDisplayProps {
  message: ReactNode
  variant?: 'inline' | 'compact'
  className?: string
}

export function SuccessDisplay({
  message,
  variant = 'inline',
  className = ""
}: SuccessDisplayProps) {
  if (variant === 'compact') {
    return (
      <div role="status" className={cn("flex items-center gap-2 text-green-600 text-sm", className)}>
        <CheckCircle className="h-4 w-4" />
        {message}
      </div>
    )
  }

  return (
    <div
      role="status"
      className={cn("flex w-full items-start gap-3 rounded-lg border border-green-200 bg-green-50 p-4 text-green-700", className)}
    >
      <CheckCircle className="mt-0.5 h-4 w-4 shrink-0 text-green-600" />
      <div className="text-sm text-green-700">{message}</div>
    </div>
  )
}

interface FieldErrorProps {
  message: string
  announce?: boolean
  className?: string
}

export function FieldError({
  message,
  announce = false,
  className = ""
}: FieldErrorProps) {
  return (
    <p role={announce ? "alert" : undefined} className={cn("text-sm text-destructive", className)}>
      {message}
    </p>
  )
}
