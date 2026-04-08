import { useEffect, useState, useRef, useCallback } from 'react'
import {
  Wallet,
  Transaction,
  Contact,
  BalanceAlert,
  WalletDetailResponse,
  NotificationStatus,
} from '../types'
import { useAuth } from '../contexts/auth-context'
import { api } from '../lib/api'

const DEFAULT_PAGE_SIZE = 100
const POLLING_PAGE_SIZE = 250

function getTransactionCacheKey(walletChecksum: string, txid: string) {
  return `${walletChecksum}:${txid}`
}

function getTransactionSortTimestamp(transaction: Transaction) {
  return transaction.confirmed_at ?? transaction.first_seen_at
}

function sortTransactions(transactions: Transaction[]) {
  return [...transactions].sort((left, right) => {
    const timestampDiff =
      getTransactionSortTimestamp(right) - getTransactionSortTimestamp(left)
    if (timestampDiff !== 0) {
      return timestampDiff
    }

    return right.txid.localeCompare(left.txid)
  })
}

function mergeTransactions(current: Transaction[], incoming: Transaction[]) {
  const merged = new Map(current.map((transaction) => [transaction.txid, transaction]))

  for (const transaction of incoming) {
    merged.set(transaction.txid, transaction)
  }

  return sortTransactions(Array.from(merged.values()))
}

export function useWalletDetail(walletChecksum: string | null) {
  const [wallet, setWallet] = useState<Wallet | null>(null)
  const [transactions, setTransactions] = useState<Transaction[]>([])
  const [contacts, setContacts] = useState<Contact[]>([])
  const [balanceAlerts, setBalanceAlerts] = useState<BalanceAlert[]>([])
  const [lastUpdate, setLastUpdate] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [isLoadingMore, setIsLoadingMore] = useState(false)
  const [isConnected, setIsConnected] = useState(true)
  const [hasMoreTransactions, setHasMoreTransactions] = useState(false)
  const [transactionNotifications, setTransactionNotifications] = useState<Record<string, NotificationStatus[]>>({})
  const [loadingTransactionNotifications, setLoadingTransactionNotifications] = useState<Record<string, boolean>>({})
  const [transactionNotificationErrors, setTransactionNotificationErrors] = useState<Record<string, string | null>>({})
  const { isAuthenticated, billingStatus } = useAuth()

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null)
  const historyCursorRef = useRef<string | null>(null)
  const incrementalSinceTimestampRef = useRef<number | null>(null)
  const isPollingRef = useRef(false)
  const isLoadingMoreRef = useRef(false)
  const transactionsRef = useRef<Transaction[]>([])
  const transactionNotificationsRef = useRef<Record<string, NotificationStatus[]>>({})
  const loadingTransactionNotificationsRef = useRef<Record<string, boolean>>({})

  useEffect(() => {
    transactionsRef.current = transactions
  }, [transactions])

  useEffect(() => {
    transactionNotificationsRef.current = transactionNotifications
  }, [transactionNotifications])

  useEffect(() => {
    loadingTransactionNotificationsRef.current = loadingTransactionNotifications
  }, [loadingTransactionNotifications])

  const getPollingInterval = useCallback(() => {
    const syncIntervalSeconds = billingStatus?.limits?.sync_interval_seconds || 60
    return syncIntervalSeconds * 1000
  }, [billingStatus?.limits?.sync_interval_seconds])

  const pruneNotificationCaches = useCallback((nextTransactions: Transaction[]) => {
    const nextKeys = new Set(
      nextTransactions.map((transaction) =>
        getTransactionCacheKey(transaction.wallet_checksum, transaction.txid)
      )
    )

    setTransactionNotifications((prev) =>
      Object.fromEntries(Object.entries(prev).filter(([key]) => nextKeys.has(key)))
    )
    setLoadingTransactionNotifications((prev) =>
      Object.fromEntries(
        Object.entries(prev).filter(([key, loading]) => nextKeys.has(key) && loading)
      )
    )
    setTransactionNotificationErrors((prev) =>
      Object.fromEntries(Object.entries(prev).filter(([key]) => nextKeys.has(key)))
    )
  }, [])

  const requestWalletDetail = useCallback(
    async (params?: {
      cursor?: string | null
      sinceTimestamp?: number | null
      pageSize?: number
    }) => {
      if (!isAuthenticated || !walletChecksum) {
        return null
      }

      return api.getWalletDetail(walletChecksum, {
        cursor: params?.cursor,
        sinceTimestamp: params?.sinceTimestamp,
        pageSize: params?.pageSize ?? DEFAULT_PAGE_SIZE,
      })
    },
    [isAuthenticated, walletChecksum]
  )

  const handleRequestError = useCallback((err: unknown, fallbackMessage: string) => {
    const status =
      typeof err === 'object' && err !== null && 'status' in err
        ? (err as { status?: number }).status
        : undefined

    if (status === 404) {
      setError('Wallet not found')
      setIsConnected(true)
      return
    }

    if (status === 403) {
      setError('Access denied to wallet')
      setIsConnected(true)
      return
    }

    console.error(fallbackMessage, err)
    setError('Failed to load wallet detail')
    setIsConnected(false)
  }, [])

  const applySharedData = useCallback((data: WalletDetailResponse) => {
    setWallet(data.wallet)
    setContacts(data.contacts || [])
    setBalanceAlerts(data.balance_alerts || [])
    setLastUpdate(data.timestamp)
    setIsConnected(true)
    setError(null)
  }, [])

  const fetchWalletDetail = useCallback(async () => {
    setIsLoading(true)
    setError(null)

    try {
      const data = await requestWalletDetail()
      if (!data) {
        return
      }

      const nextTransactions = sortTransactions(data.transactions)
      applySharedData(data)
      setTransactions(nextTransactions)
      pruneNotificationCaches(nextTransactions)
      historyCursorRef.current = data.pagination.next_cursor
      setHasMoreTransactions(data.pagination.has_more)
      incrementalSinceTimestampRef.current = data.timestamp
    } catch (err) {
      handleRequestError(err, 'Failed to fetch wallet detail:')
    } finally {
      setIsLoading(false)
    }
  }, [applySharedData, handleRequestError, pruneNotificationCaches, requestWalletDetail])

  const pollWalletDetail = useCallback(async () => {
    if (isPollingRef.current) {
      return
    }

    const sinceTimestamp = incrementalSinceTimestampRef.current
    if (!sinceTimestamp) {
      await fetchWalletDetail()
      return
    }

    isPollingRef.current = true
    try {
      const data = await requestWalletDetail({
        sinceTimestamp,
        pageSize: POLLING_PAGE_SIZE,
      })
      if (!data) {
        return
      }

      if (data.pagination.has_more) {
        await fetchWalletDetail()
        return
      }

      applySharedData(data)
      if (data.transactions.length > 0) {
        const mergedTransactions = mergeTransactions(transactionsRef.current, data.transactions)
        setTransactions(mergedTransactions)
        pruneNotificationCaches(mergedTransactions)
      }

      incrementalSinceTimestampRef.current = data.timestamp
      setLastUpdate(data.timestamp)
    } catch (err) {
      handleRequestError(err, 'Failed to poll wallet detail:')
    } finally {
      isPollingRef.current = false
    }
  }, [applySharedData, fetchWalletDetail, handleRequestError, pruneNotificationCaches, requestWalletDetail])

  const loadMoreTransactions = useCallback(async () => {
    if (!historyCursorRef.current || isLoadingMoreRef.current) {
      return
    }

    isLoadingMoreRef.current = true
    setIsLoadingMore(true)
    try {
      const data = await requestWalletDetail({ cursor: historyCursorRef.current })
      if (!data) {
        return
      }

      applySharedData(data)
      const mergedTransactions = mergeTransactions(transactionsRef.current, data.transactions)
      setTransactions(mergedTransactions)
      pruneNotificationCaches(mergedTransactions)
      historyCursorRef.current = data.pagination.next_cursor
      setHasMoreTransactions(data.pagination.has_more)
    } catch (err) {
      handleRequestError(err, 'Failed to load more transactions:')
    } finally {
      isLoadingMoreRef.current = false
      setIsLoadingMore(false)
    }
  }, [applySharedData, handleRequestError, pruneNotificationCaches, requestWalletDetail])

  const loadTransactionNotifications = useCallback(async (transactionWalletChecksum: string, txid: string) => {
    if (!isAuthenticated) {
      return
    }

    const cacheKey = getTransactionCacheKey(transactionWalletChecksum, txid)

    if (
      transactionNotificationsRef.current[cacheKey] ||
      loadingTransactionNotificationsRef.current[cacheKey]
    ) {
      return
    }

    setLoadingTransactionNotifications((prev) => ({
      ...prev,
      [cacheKey]: true,
    }))
    setTransactionNotificationErrors((prev) => ({
      ...prev,
      [cacheKey]: null,
    }))

    try {
      const notifications = await api.getTransactionNotifications(transactionWalletChecksum, txid)
      setTransactionNotifications((prev) => ({
        ...prev,
        [cacheKey]: notifications,
      }))
    } catch (err) {
      console.error('Failed to fetch transaction notifications:', err)
      setTransactionNotificationErrors((prev) => ({
        ...prev,
        [cacheKey]: 'Failed to load transaction notifications',
      }))
    } finally {
      setLoadingTransactionNotifications((prev) => ({
        ...prev,
        [cacheKey]: false,
      }))
    }
  }, [isAuthenticated])

  const refresh = useCallback(() => {
    historyCursorRef.current = null
    incrementalSinceTimestampRef.current = null
    fetchWalletDetail()
  }, [fetchWalletDetail])

  useEffect(() => {
    setWallet(null)
    setTransactions([])
    setContacts([])
    setBalanceAlerts([])
    setLastUpdate(null)
    setError(null)
    setHasMoreTransactions(false)
    setTransactionNotifications({})
    setLoadingTransactionNotifications({})
    setTransactionNotificationErrors({})
    historyCursorRef.current = null
    incrementalSinceTimestampRef.current = null

    if (!walletChecksum) {
      return
    }

    fetchWalletDetail()

    const intervalMs = getPollingInterval()
    pollingIntervalRef.current = setInterval(() => {
      pollWalletDetail()
    }, intervalMs)

    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current)
      }
    }
  }, [fetchWalletDetail, walletChecksum, getPollingInterval, pollWalletDetail])

  return {
    wallet,
    transactions,
    contacts,
    balanceAlerts,
    lastUpdate,
    error,
    isLoading,
    isLoadingMore,
    hasMoreTransactions,
    isConnected,
    transactionNotifications,
    loadingTransactionNotifications,
    transactionNotificationErrors,
    loadTransactionNotifications,
    refresh,
    loadMoreTransactions,
  }
}
