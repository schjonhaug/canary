import { Card, CardDescription, CardHeader, CardTitle } from "./card"
import { AlertTriangle, CheckCircle, XCircle } from "lucide-react"
import { Alert, AlertDescription } from "./alert"

interface ErrorDisplayProps {
  title?: string
  message: string
  variant?: 'card' | 'inline'
  className?: string
}

export function ErrorDisplay({ 
  title = "Error", 
  message, 
  variant = 'card', 
  className = "" 
}: ErrorDisplayProps) {
  if (variant === 'inline') {
    return (
      <Alert variant="destructive" className={className}>
        <XCircle className="h-4 w-4" />
        <AlertDescription>{message}</AlertDescription>
      </Alert>
    )
  }

  return (
    <Card className={className}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-destructive">
          <AlertTriangle className="h-5 w-5" />
          {title}
        </CardTitle>
        <CardDescription className="text-destructive">
          {message}
        </CardDescription>
      </CardHeader>
    </Card>
  )
}

interface SuccessDisplayProps {
  message: string
  className?: string
}

export function SuccessDisplay({ message, className = "" }: SuccessDisplayProps) {
  return (
    <Alert className={`border-green-200 bg-green-50 text-green-700 ${className}`}>
      <CheckCircle className="h-4 w-4 text-green-600" />
      <AlertDescription className="text-green-700">{message}</AlertDescription>
    </Alert>
  )
}
