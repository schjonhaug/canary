import { useEffect, useState, useRef, useCallback } from 'react';
import { Wallet, Transaction, Contact, BalanceAlert, WalletDetailResponse } from '../types';
import { useAuth } from '../contexts/auth-context';
import { getApiBaseUrl } from '../lib/utils';

const DEFAULT_PAGE_SIZE = 100;

function getTransactionSortTimestamp(transaction: Transaction) {
  return transaction.confirmed_at ?? transaction.first_seen_at;
}

function sortTransactions(transactions: Transaction[]) {
  return [...transactions].sort((left, right) => {
    const timestampDiff = getTransactionSortTimestamp(right) - getTransactionSortTimestamp(left);
    if (timestampDiff !== 0) {
      return timestampDiff;
    }

    return right.txid.localeCompare(left.txid);
  });
}

function mergeTransactions(current: Transaction[], incoming: Transaction[]) {
  const merged = new Map(current.map((transaction) => [transaction.txid, transaction]));

  for (const transaction of incoming) {
    merged.set(transaction.txid, transaction);
  }

  return sortTransactions(Array.from(merged.values()));
}

export function useWalletDetail(walletChecksum: string | null) {
  const [wallet, setWallet] = useState<Wallet | null>(null);
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [balanceAlerts, setBalanceAlerts] = useState<BalanceAlert[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const [hasMoreTransactions, setHasMoreTransactions] = useState(false);
  const { isAuthenticated, billingStatus } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const historyCursorRef = useRef<string | null>(null);
  const incrementalSinceTimestampRef = useRef<number | null>(null);

  // Get polling interval from billing status sync interval or default to 60 seconds
  const getPollingInterval = useCallback(() => {
    const syncIntervalSeconds = billingStatus?.limits?.sync_interval_seconds || 60;
    return syncIntervalSeconds * 1000; // Convert to milliseconds
  }, [billingStatus?.limits?.sync_interval_seconds]);

  const requestWalletDetail = useCallback(async (params?: {
    cursor?: string | null;
    sinceTimestamp?: number | null;
  }) => {
    // Only fetch data if user is authenticated and walletChecksum is provided
    if (!isAuthenticated || !walletChecksum) {
      return null;
    }

    const baseUrl = getApiBaseUrl();
    const searchParams = new URLSearchParams({
      page_size: DEFAULT_PAGE_SIZE.toString(),
    });

    if (params?.cursor) {
      searchParams.set('cursor', params.cursor);
    }
    if (params?.sinceTimestamp !== null && params?.sinceTimestamp !== undefined) {
      searchParams.set('since_timestamp', params.sinceTimestamp.toString());
    }

    const response = await fetch(
      `${baseUrl}/api/wallets/${walletChecksum}/detail?${searchParams.toString()}`,
      {
        credentials: 'include',
      }
    );

    if (!response.ok) {
      throw response;
    }

    return (await response.json()) as WalletDetailResponse;
  }, [isAuthenticated, walletChecksum]);

  const applySharedData = useCallback((data: WalletDetailResponse) => {
    setWallet(data.wallet);
    setContacts(data.contacts || []);
    setBalanceAlerts(data.balance_alerts || []);
    setLastUpdate(data.timestamp);
    setIsConnected(true);
    setError(null);
  }, []);

  const fetchWalletDetail = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      const data = await requestWalletDetail();
      if (!data) {
        return;
      }

      applySharedData(data);
      setTransactions(sortTransactions(data.transactions));
      historyCursorRef.current = data.pagination.next_cursor;
      setHasMoreTransactions(data.pagination.has_more);
      incrementalSinceTimestampRef.current = data.timestamp;
    } catch (err) {
      if (err instanceof Response && err.status === 404) {
        setError('Wallet not found');
        setIsConnected(true);
      } else if (err instanceof Response && err.status === 403) {
        setError('Access denied to wallet');
        setIsConnected(true);
      } else {
        console.error('Failed to fetch wallet detail:', err);
        setError('Failed to load wallet detail');
        setIsConnected(false);
      }
    } finally {
      setIsLoading(false);
    }
  }, [applySharedData, requestWalletDetail]);

  const pollWalletDetail = useCallback(async () => {
    const sinceTimestamp = incrementalSinceTimestampRef.current;
    if (!sinceTimestamp) {
      await fetchWalletDetail();
      return;
    }

    try {
      let cursor: string | null = null;
      let firstResponseTimestamp: number | null = null;
      const deltaTransactions: Transaction[] = [];

      do {
        const data = await requestWalletDetail({ cursor, sinceTimestamp });
        if (!data) {
          return;
        }

        if (firstResponseTimestamp === null) {
          firstResponseTimestamp = data.timestamp;
        }

        applySharedData(data);
        deltaTransactions.push(...data.transactions);
        cursor = data.pagination.next_cursor;
      } while (cursor);

      if (deltaTransactions.length > 0) {
        setTransactions((currentTransactions) =>
          mergeTransactions(currentTransactions, deltaTransactions)
        );
      }

      if (firstResponseTimestamp !== null) {
        incrementalSinceTimestampRef.current = firstResponseTimestamp;
        setLastUpdate(firstResponseTimestamp);
      }
    } catch (err) {
      console.error('Failed to poll wallet detail:', err);
      setError('Failed to load wallet detail');
      setIsConnected(false);
    }
  }, [applySharedData, fetchWalletDetail, requestWalletDetail]);

  const loadMoreTransactions = useCallback(async () => {
    if (!historyCursorRef.current) {
      return;
    }

    setIsLoadingMore(true);
    try {
      const data = await requestWalletDetail({ cursor: historyCursorRef.current });
      if (!data) {
        return;
      }

      applySharedData(data);
      setTransactions((currentTransactions) =>
        mergeTransactions(currentTransactions, data.transactions)
      );
      historyCursorRef.current = data.pagination.next_cursor;
      setHasMoreTransactions(data.pagination.has_more);
    } catch (err) {
      console.error('Failed to load more transactions:', err);
      setError('Failed to load wallet detail');
    } finally {
      setIsLoadingMore(false);
    }
  }, [applySharedData, requestWalletDetail]);

  const refresh = useCallback(() => {
    fetchWalletDetail();
  }, [fetchWalletDetail]);

  useEffect(() => {
    // Clear previous data when walletChecksum changes
    setWallet(null);
    setTransactions([]);
    setContacts([]);
    setBalanceAlerts([]);
    setLastUpdate(null);
    setError(null);
    setHasMoreTransactions(false);
    historyCursorRef.current = null;
    incrementalSinceTimestampRef.current = null;

    // Only fetch if walletChecksum is provided
    if (!walletChecksum) {
      return;
    }

    // Load initial data immediately
    fetchWalletDetail();

    // Set up polling interval using dynamic interval
    const intervalMs = getPollingInterval();
    pollingIntervalRef.current = setInterval(() => {
      pollWalletDetail();
    }, intervalMs);

    // Cleanup on unmount or walletChecksum change
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
      }
    };
  }, [fetchWalletDetail, walletChecksum, getPollingInterval, pollWalletDetail]);

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
    refresh, // Manual refresh function
    loadMoreTransactions,
  };
}
