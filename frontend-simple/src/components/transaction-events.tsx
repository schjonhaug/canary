"use client"

import { useEffect, useState } from "react"
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table"
import { Badge } from "@/components/ui/badge"
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card"

interface TransactionEvent {
  id: number
  wallet_id: number
  wallet_name: string
  event_type: "send" | "receive"
  amount_sats: number
  is_confirmed: boolean
  is_rbf: boolean
  is_cpfp: boolean
  confirmed_amount_sats?: number
  created_at: string
}

export function TransactionEvents() {
  const [events, setEvents] = useState<TransactionEvent[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    async function fetchEvents() {
      try {
        // Use the same hostname as the current page, but port 3000 for the API
        const apiUrl = `http://${window.location.hostname}:3000/transaction-events`
        const response = await fetch(apiUrl)
        if (!response.ok) {
          throw new Error(`HTTP error! status: ${response.status}`)
        }
        const data = await response.json()
        setEvents(data)
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to fetch events")
      } finally {
        setLoading(false)
      }
    }

    fetchEvents()
    
    // Refresh every 5 seconds
    const interval = setInterval(fetchEvents, 5000)
    return () => clearInterval(interval)
  }, [])

  const formatSats = (sats: number) => {
    const btc = sats / 100_000_000
    return `${btc.toLocaleString("nb-NO", { 
      minimumFractionDigits: 8, 
      maximumFractionDigits: 8 
    })} BTC`
  }

  const formatDateTime = (dateTime: string) => {
    return new Date(dateTime).toLocaleString("nb-NO")
  }

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Transaction Events</CardTitle>
          <CardDescription>Loading transaction events...</CardDescription>
        </CardHeader>
      </Card>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Transaction Events</CardTitle>
          <CardDescription className="text-destructive">Error: {error}</CardDescription>
        </CardHeader>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Transaction Events</CardTitle>
        <CardDescription>Real-time Bitcoin transaction events from all wallets</CardDescription>
      </CardHeader>
      <CardContent>
        {events.length === 0 ? (
          <p className="text-muted-foreground">No transaction events found.</p>
        ) : (
          <Table>
            <TableCaption>A list of all transaction events from the TxRay system.</TableCaption>
            <TableHeader>
              <TableRow>
                <TableHead>Wallet</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Amount</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Flags</TableHead>
                <TableHead>Date/Time</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {events.map((event) => (
                <TableRow key={event.id}>
                  <TableCell className="font-medium">{event.wallet_name}</TableCell>
                  <TableCell>
                    <Badge 
                      variant={event.event_type === "receive" ? "default" : "secondary"}
                    >
                      {event.event_type === "receive" ? "📥 Receive" : "📤 Send"}
                    </Badge>
                  </TableCell>
                  <TableCell className="font-mono">
                    {formatSats(event.amount_sats)}
                    {event.confirmed_amount_sats && event.confirmed_amount_sats !== event.amount_sats && (
                      <div className="text-sm text-muted-foreground">
                        Confirmed: {formatSats(event.confirmed_amount_sats)}
                      </div>
                    )}
                  </TableCell>
                  <TableCell>
                    <Badge variant={event.is_confirmed ? "default" : "outline"}>
                      {event.is_confirmed ? "✅ Confirmed" : "⏳ Pending"}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex gap-1">
                      {event.is_rbf && (
                        <Badge variant="outline" className="text-xs">RBF</Badge>
                      )}
                      {event.is_cpfp && (
                        <Badge variant="outline" className="text-xs">CPFP</Badge>
                      )}
                    </div>
                  </TableCell>
                  <TableCell className="text-sm">
                    {formatDateTime(event.created_at)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  )
}