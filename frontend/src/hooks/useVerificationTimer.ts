import { useState, useCallback, useRef, useEffect } from "react"

interface UseVerificationTimerReturn {
  timeRemaining: number
  startTimer: () => void
  clearTimer: () => void
  formatTime: (seconds: number) => string
  isExpired: boolean
}

export function useVerificationTimer(
  duration: number = 600, // 10 minutes default
  onExpire?: () => void
): UseVerificationTimerReturn {
  const [timeRemaining, setTimeRemaining] = useState<number>(0)
  const timerRef = useRef<NodeJS.Timeout | null>(null)

  // Clear timer on unmount
  useEffect(() => {
    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current)
      }
    }
  }, [])

  const clearTimer = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current)
      timerRef.current = null
    }
    setTimeRemaining(0)
  }, [])

  const startTimer = useCallback(() => {
    setTimeRemaining(duration)
    if (timerRef.current) {
      clearInterval(timerRef.current)
    }
    timerRef.current = setInterval(() => {
      setTimeRemaining(prev => {
        if (prev <= 1) {
          if (timerRef.current) {
            clearInterval(timerRef.current)
            timerRef.current = null
          }
          onExpire?.()
          return 0
        }
        return prev - 1
      })
    }, 1000)
  }, [duration, onExpire])

  const formatTime = useCallback((seconds: number) => {
    const mins = Math.floor(seconds / 60)
    const secs = seconds % 60
    return `${mins}:${secs.toString().padStart(2, '0')}`
  }, [])

  return {
    timeRemaining,
    startTimer,
    clearTimer,
    formatTime,
    isExpired: timeRemaining === 0
  }
}
