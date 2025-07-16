import { Card, CardDescription, CardHeader, CardTitle } from "./card"
import { AlertTriangle, XCircle } from "lucide-react"

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
      <div className={`p-3 bg-red-50 border border-red-200 rounded-lg flex items-center gap-2 ${className}`}>
        <XCircle className="h-4 w-4 text-red-500 flex-shrink-0" />
        <p className="text-sm text-red-700">{message}</p>
      </div>
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
    <div className={`p-3 bg-green-50 border border-green-200 rounded-lg flex items-center gap-2 ${className}`}>
      <XCircle className="h-4 w-4 text-green-500 flex-shrink-0" />
      <p className="text-sm text-green-700">{message}</p>
    </div>
  )
}