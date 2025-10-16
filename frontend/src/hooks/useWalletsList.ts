import { useEffect, useState, useRef, useCallback, useMemo } from 'react';
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
  const [isLoading, setIsLoading] = useState(shouldFetch); // Start as true if we should fetch
  const [isConnected, setIsConnected] = useState(true);
  const [currentPollingInterval, setCurrentPollingInterval] = useState(60000); // Default 60 seconds
  const [hasInitialData, setHasInitialData] = useState(false); // Track if we've ever loaded data
  const { token, isAuthenticated, billingStatus } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  const fetchWallets = useCallback(async () => {
    // Only fetch data if user is authenticated, has a token, and should fetch
    if (!isAuthenticated || !token || !shouldFetch) {
      return;
    }

    // Only show loading spinner on initial fetch, not on background refreshes
    // This prevents the "blinking" effect when polling with pending wallets
    if (!hasInitialData) {
      setIsLoading(true);
    }
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

        // Note: We now rely on backend's status field instead of client-side pending tracking

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
      // Mark that we've attempted initial data fetch, stop showing loading spinner
      if (!hasInitialData) {
        setIsLoading(false);
        setHasInitialData(true);
      }
    }
  }, [token, isAuthenticated, shouldFetch, hasInitialData]);

  // Update polling interval when wallets or billing status changes
  useEffect(() => {
    const hasPendingWallets = wallets.some(wallet => wallet.status === 'pending');
    let newInterval: number;
    
    if (hasPendingWallets) {
      newInterval = 1000; // 1 second when wallets are syncing
    } else {
      const syncIntervalSeconds = billingStatus?.limits?.sync_interval_seconds || 60;
      newInterval = syncIntervalSeconds * 1000; // Convert to milliseconds
    }
    
    // Only update if interval actually changed to avoid unnecessary re-renders
    if (newInterval !== currentPollingInterval) {
      setCurrentPollingInterval(newInterval);
    }
  }, [wallets, billingStatus?.limits?.sync_interval_seconds, currentPollingInterval]);

  const refresh = useCallback(() => {
    fetchWallets();
  }, [fetchWallets]);

  // Add a newly created wallet to the list immediately
  const addWallet = useCallback((wallet: Wallet) => {
    // Add to regular wallets list immediately
    setWallets(prev => [...prev, wallet]);
    // Note: Polling interval will automatically adjust based on wallet.status
  }, []);

  // Return wallets for UI (filter out deleted wallets)
  const allWallets = useMemo(() => {
    return wallets.filter(wallet => wallet.status !== 'deleted');
  }, [wallets]);

  // Set up initial fetch when auth changes
  useEffect(() => {
    // Only set up polling if we should fetch
    if (!shouldFetch) {
      return;
    }

    // Load initial data immediately when component mounts or auth changes
    fetchWallets();
  }, [fetchWallets, shouldFetch]);

  // Set up polling interval - separate effect to avoid recreating interval unnecessarily
  useEffect(() => {
    // Only set up polling if we should fetch AND we have wallets to monitor
    if (!shouldFetch || wallets.length === 0) {
      // Clear any existing interval if we don't need to poll
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
        pollingIntervalRef.current = null;
      }
      return;
    }

    // Clear existing interval before setting new one
    if (pollingIntervalRef.current) {
      clearInterval(pollingIntervalRef.current);
    }
    
    // Set up new interval with current polling interval
    pollingIntervalRef.current = setInterval(() => {
      fetchWallets();
    }, currentPollingInterval);

    // Cleanup on unmount or interval change
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
        pollingIntervalRef.current = null;
      }
    };
  }, [fetchWallets, shouldFetch, currentPollingInterval, wallets.length]);

  return { 
    wallets: allWallets, 
    lastUpdate, 
    error, 
    isLoading,
    isConnected,
    refresh, // Manual refresh function
    addWallet, // Add new wallet immediately
    hasPendingWallets: wallets.some(wallet => wallet.status === 'pending'),
  };
}