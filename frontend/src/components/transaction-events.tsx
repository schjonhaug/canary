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
import { ArrowDown, ArrowUp, CheckCircle, Clock, RefreshCw, Zap } from "lucide-react"

interface TransactionEvent {
  id: number
  wallet_id: number
  wallet_name: string
  event_type: "send" | "receive"
  amount_sats: number
  is_confirmed: boolean
  is_rbf: boolean
  is_cpfp: boolean
  balance_total?: number
  created_at: string
}

interface TransactionEventsProps {
  selectedWalletId?: number | null
}

export function TransactionEvents({ selectedWalletId }: TransactionEventsProps) {
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
    return `${btc.toLocaleString(undefined, { 
      minimumFractionDigits: 8, 
      maximumFractionDigits: 8 
    })} BTC`
  }

  const formatDateTime = (dateTime: string) => {
    const date = new Date(dateTime)
    return date.toLocaleString()
  }

  // Filter events by selected wallet if one is selected
  const filteredEvents = selectedWalletId 
    ? events.filter(event => event.wallet_id === selectedWalletId)
    : events

  const getCardTitle = () => {
    if (selectedWalletId && filteredEvents.length > 0) {
      const walletName = filteredEvents[0]?.wallet_name || `Wallet ${selectedWalletId}`
      return `Transaction Events - ${walletName}`
    }
    return "Transaction Events"
  }

  const getCardDescription = () => {
    if (selectedWalletId) {
      return filteredEvents.length > 0 
        ? `${filteredEvents.length} transaction event${filteredEvents.length !== 1 ? 's' : ''} for selected wallet`
        : "No transaction events found for selected wallet"
    }
    return "Real-time Bitcoin transaction events from all wallets"
  }

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{getCardTitle()}</CardTitle>
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
        <CardTitle>{getCardTitle()}</CardTitle>
        <CardDescription>{getCardDescription()}</CardDescription>
      </CardHeader>
      <CardContent>
        {filteredEvents.length === 0 ? (
          <p className="text-muted-foreground">
            {selectedWalletId 
              ? "No transaction events found for the selected wallet." 
              : "No transaction events found."
            }
          </p>
        ) : (
          <Table>
            <TableCaption>A list of all transaction events from the Kanari system.</TableCaption>
            <TableHeader>
              <TableRow>
                <TableHead>Wallet</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Amount</TableHead>
                <TableHead>Total Balance</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Flags</TableHead>
                <TableHead>Date/Time</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredEvents.map((event) => (
                <TableRow key={event.id}>
                  <TableCell className="font-medium">{event.wallet_name}</TableCell>
                  <TableCell>
                    <Badge 
                      variant={event.event_type === "receive" ? "default" : "secondary"}
                      className="flex items-center gap-1"
                    >
                      {event.event_type === "receive" ? (
                        <>
                          <ArrowDown className="h-3 w-3" />
                          Receive
                        </>
                      ) : (
                        <>
                          <ArrowUp className="h-3 w-3" />
                          Send
                        </>
                      )}
                    </Badge>
                  </TableCell>
                  <TableCell className="font-mono">
                    {formatSats(event.amount_sats)}
                  </TableCell>
                  <TableCell className="font-mono">
                    {event.balance_total ? formatSats(event.balance_total) : "N/A"}
                  </TableCell>
                  <TableCell>
                    <Badge 
                      variant={event.is_confirmed ? "default" : "outline"}
                      className="flex items-center gap-1"
                    >
                      {event.is_confirmed ? (
                        <>
                          <CheckCircle className="h-3 w-3" />
                          Confirmed
                        </>
                      ) : (
                        <>
                          <Clock className="h-3 w-3" />
                          Pending
                        </>
                      )}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <div className="flex gap-1">
                      {event.is_rbf && (
                        <Badge variant="outline" className="text-xs flex items-center gap-1">
                          <RefreshCw className="h-3 w-3" />
                          RBF
                        </Badge>
                      )}
                      {event.is_cpfp && (
                        <Badge variant="outline" className="text-xs flex items-center gap-1">
                          <Zap className="h-3 w-3" />
                          CPFP
                        </Badge>
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