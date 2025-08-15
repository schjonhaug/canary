import { useEffect, useState, useRef, useCallback } from 'react';
import { Wallet } from '../types';
import { useAuth } from '../contexts/auth-context';

interface WalletsListResponse {
  timestamp: number;
  wallets: Wallet[];
}

export function useWalletsList(shouldFetch: boolean = true) {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const { token, isAuthenticated, billingStatus } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Get polling interval from billing status sync interval or default to 60 seconds
  const getPollingInterval = useCallback(() => {
    const syncIntervalSeconds = billingStatus?.limits?.sync_interval_seconds || 60;
    return syncIntervalSeconds * 1000; // Convert to milliseconds
  }, [billingStatus?.limits?.sync_interval_seconds]);

  const fetchWallets = useCallback(async () => {
    // Only fetch data if user is authenticated, has a token, and should fetch
    if (!isAuthenticated || !token || !shouldFetch) {
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const headers: HeadersInit = {};
      
      // Add Authorization header if token is available
      if (token) {
        headers['Authorization'] = `Bearer ${token}`;
      }
      
      const response = await fetch('/api/wallets', {
        headers,
      });
      
      if (response.ok) {
        const data: WalletsListResponse = await response.json();
        setWallets(data.wallets);
        setLastUpdate(data.timestamp);
        setIsConnected(true);
        setError(null);
      } else {
        console.error('Failed to load wallets list:', response.status);
        setError('Failed to load wallets list');
        setIsConnected(false);
      }
    } catch (err) {
      console.error('Failed to fetch wallets list:', err);
      setError('Failed to load wallets list');
      setIsConnected(false);
    } finally {
      setIsLoading(false);
    }
  }, [token, isAuthenticated, shouldFetch]);

  const refresh = useCallback(() => {
    fetchWallets();
  }, [fetchWallets]);

  useEffect(() => {
    // Only set up polling if we should fetch
    if (!shouldFetch) {
      return;
    }

    // Load initial data immediately
    fetchWallets();

    // Set up polling interval using dynamic interval
    const intervalMs = getPollingInterval();
    pollingIntervalRef.current = setInterval(() => {
      fetchWallets();
    }, intervalMs);

    // Cleanup on unmount
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
      }
    };
  }, [fetchWallets, shouldFetch, getPollingInterval]);

  return { 
    wallets, 
    lastUpdate, 
    error, 
    isLoading,
    isConnected,
    refresh, // Manual refresh function
  };
}