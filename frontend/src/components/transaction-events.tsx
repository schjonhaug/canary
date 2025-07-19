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
import { Skeleton } from "@/components/ui/skeleton"
import { CheckCircle, Clock, RefreshCw, Zap } from "lucide-react"
import { TransactionEvent } from "../types"
import { formatBitcoinAmount, formatDateTime } from "@/lib/utils"

interface TransactionEventsProps {
  selectedWalletId?: number | null
  events: TransactionEvent[]
  isConnected: boolean
  error: string | null
  lastUpdate: number | null
}

export function TransactionEvents({ selectedWalletId, events, error, lastUpdate }: TransactionEventsProps) {
  const [hasReceivedData, setHasReceivedData] = useState(false)

  // Track when we've received data for the first time
  useEffect(() => {
    if (lastUpdate !== null) {
      setHasReceivedData(true)
    }
  }, [lastUpdate])


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

  if (!hasReceivedData) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>{getCardTitle()}</CardTitle>
          <CardDescription>Loading transaction events...</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Date/Time</TableHead>
                <TableHead>Wallet</TableHead>
                <TableHead>Transaction</TableHead>
                <TableHead>Amount</TableHead>
                <TableHead>Total Balance</TableHead>
                <TableHead>SMS Recipients</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {[1, 2, 3, 4, 5].map((i) => (
                <TableRow key={i}>
                  <TableCell>
                    <Skeleton className="h-4 w-32" />
                  </TableCell>
                  <TableCell>
                    <Skeleton className="h-4 w-24" />
                  </TableCell>
                  <TableCell>
                    <Skeleton className="h-6 w-20" />
                  </TableCell>
                  <TableCell>
                    <Skeleton className="h-4 w-28" />
                  </TableCell>
                  <TableCell>
                    <Skeleton className="h-4 w-28" />
                  </TableCell>
                  <TableCell>
                    <Skeleton className="h-4 w-24" />
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
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
            <TableCaption>A list of all transaction events from the Canary system.</TableCaption>
            <TableHeader>
              <TableRow>
                <TableHead>Date/Time</TableHead>
                <TableHead>Wallet</TableHead>
                <TableHead>Transaction</TableHead>
                <TableHead>Amount</TableHead>
                <TableHead>Total Balance</TableHead>
                <TableHead>SMS Recipients</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredEvents.map((event) => (
                <TableRow key={event.id}>
                  <TableCell className="text-sm">
                    {formatDateTime(event.transaction_time)}
                  </TableCell>
                  <TableCell className="font-medium">{event.wallet_name}</TableCell>
                  <TableCell>
                    <div className="flex items-center gap-1">
                      <Badge 
                        variant="outline"
                        className="flex items-center gap-1"
                        title={`${event.event_type === "receive" ? "Receive" : "Send"} - ${event.is_confirmed ? "Confirmed" : "Pending"}`}
                      >
                        {event.is_confirmed ? (
                          <CheckCircle className="h-3 w-3 text-green-500" />
                        ) : (
                          <Clock className="h-3 w-3 text-yellow-500" />
                        )}
                        {event.is_confirmed 
                          ? (event.event_type === "receive" ? "Received" : "Sent")
                          : (event.event_type === "receive" ? "Receiving" : "Sending")
                        }
                        {event.is_rbf && (
                          <span title="Replace-By-Fee (RBF)">
                            <RefreshCw className="h-2 w-2 ml-1" />
                          </span>
                        )}
                        {event.is_cpfp && (
                          <span title="Child-Pays-For-Parent (CPFP)">
                            <Zap className="h-2 w-2 ml-1" />
                          </span>
                        )}
                      </Badge>
                    </div>
                  </TableCell>
                  <TableCell className="font-mono">
                    {formatBitcoinAmount(event.amount_sats)}
                  </TableCell>
                  <TableCell className="font-mono">
                    {event.balance_total ? formatBitcoinAmount(event.balance_total) : "N/A"}
                  </TableCell>
                  <TableCell className="text-sm">
                    {event.sms_recipients_status && event.sms_recipients_status.length > 0 ? (
                      <div className="flex flex-wrap gap-1">
                        {event.sms_recipients_status.map((recipient, index) => (
                          <span
                            key={index}
                            className="inline-flex items-center gap-1"
                            title={recipient.error_message || `SMS ${recipient.status}`}
                          >
                            <span>
                              {recipient.status === 'sent' ? '✅' : 
                               recipient.status === 'failed' ? '❌' : 
                               '⏳'}
                            </span>
                            <span>{recipient.name}</span>
                            {index < event.sms_recipients_status.length - 1 && <span>,</span>}
                          </span>
                        ))}
                      </div>
                    ) : event.sms_recipients.length > 0 ? (
                      event.sms_recipients.join(", ")
                    ) : (
                      "None"
                    )}
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