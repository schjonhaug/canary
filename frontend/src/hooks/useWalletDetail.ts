import { useEffect, useState, useRef, useCallback } from 'react';
import { Wallet, Transaction, Contact, BalanceAlert } from '../types';
import { useAuth } from '../contexts/auth-context';
import { getApiBaseUrl } from '../lib/utils';

interface WalletDetailResponse {
  timestamp: number;
  wallet: Wallet;
  transactions: Transaction[];
  contacts: Contact[];
  balance_alerts: BalanceAlert[];
}

export function useWalletDetail(walletChecksum: string | null) {
  const [wallet, setWallet] = useState<Wallet | null>(null);
  const [transactions, setTransactions] = useState<Transaction[]>([]);
  const [contacts, setContacts] = useState<Contact[]>([]);
  const [balanceAlerts, setBalanceAlerts] = useState<BalanceAlert[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const { isAuthenticated, billingStatus } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Get polling interval from billing status sync interval or default to 60 seconds
  const getPollingInterval = useCallback(() => {
    const syncIntervalSeconds = billingStatus?.limits?.sync_interval_seconds || 60;
    return syncIntervalSeconds * 1000; // Convert to milliseconds
  }, [billingStatus?.limits?.sync_interval_seconds]);

  const fetchWalletDetail = useCallback(async () => {
    // Only fetch data if user is authenticated and walletChecksum is provided
    if (!isAuthenticated || !walletChecksum) {
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      // Use credentials: 'include' to send HttpOnly auth cookie
      const baseUrl = getApiBaseUrl();
      const response = await fetch(`${baseUrl}/api/wallets/${walletChecksum}/detail`, {
        credentials: 'include',
      });

      if (response.ok) {
        const data: WalletDetailResponse = await response.json();
        setWallet(data.wallet);
        setTransactions(data.transactions);
        setContacts(data.contacts || []);
        setBalanceAlerts(data.balance_alerts || []);
        setLastUpdate(data.timestamp);
        setIsConnected(true);
        setError(null);
      } else if (response.status === 404) {
        setError('Wallet not found');
        setIsConnected(true);
      } else if (response.status === 403) {
        setError('Access denied to wallet');
        setIsConnected(true);
      } else {
        console.error('Failed to load wallet detail:', response.status);
        setError('Failed to load wallet detail');
        setIsConnected(false);
      }
    } catch (err) {
      console.error('Failed to fetch wallet detail:', err);
      setError('Failed to load wallet detail');
      setIsConnected(false);
    } finally {
      setIsLoading(false);
    }
  }, [isAuthenticated, walletChecksum]);

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

    // Only fetch if walletChecksum is provided
    if (!walletChecksum) {
      return;
    }

    // Load initial data immediately
    fetchWalletDetail();

    // Set up polling interval using dynamic interval
    const intervalMs = getPollingInterval();
    pollingIntervalRef.current = setInterval(() => {
      fetchWalletDetail();
    }, intervalMs);

    // Cleanup on unmount or walletChecksum change
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
      }
    };
  }, [fetchWalletDetail, walletChecksum, getPollingInterval]);

  return {
    wallet,
    transactions,
    contacts,
    balanceAlerts,
    lastUpdate,
    error,
    isLoading,
    isConnected,
    refresh, // Manual refresh function
  };
}
