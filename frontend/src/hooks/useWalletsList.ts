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
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const [pendingWallets, setPendingWallets] = useState<Wallet[]>([]);
  const { token, isAuthenticated, billingStatus } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Get polling interval - faster when there are pending wallets, otherwise use tier-based
  const getPollingInterval = useCallback(() => {
    if (pendingWallets.length > 0) {
      return 1000; // 1 second when wallets are syncing
    }
    const syncIntervalSeconds = billingStatus?.limits?.sync_interval_seconds || 60;
    return syncIntervalSeconds * 1000; // Convert to milliseconds
  }, [billingStatus?.limits?.sync_interval_seconds, pendingWallets.length]);

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
        
        // Check if any pending wallets are now fully loaded (have balance and/or last_activity)
        if (pendingWallets.length > 0) {
          const updatedPending = pendingWallets.filter(pending => {
            const updated = data.wallets.find(w => w.checksum === pending.checksum);
            // Remove from pending if wallet now has balance > 0 OR has last_activity
            return updated && updated.balance_total === 0 && !updated.last_activity;
          });
          setPendingWallets(updatedPending);
        }
        
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
  }, [token, isAuthenticated, shouldFetch, pendingWallets]);

  const refresh = useCallback(() => {
    fetchWallets();
  }, [fetchWallets]);

  // Add a newly created wallet to the list immediately
  const addWallet = useCallback((wallet: Wallet) => {
    // Add to regular wallets list immediately
    setWallets(prev => [...prev, wallet]);
    
    // If wallet appears to be pending (no balance, no activity), add to pending list for fast polling
    if (wallet.balance_total === 0 && !wallet.last_activity) {
      setPendingWallets(prev => [...prev, wallet]);
    }
  }, []);

  // Return combined list of regular + pending wallets for UI
  const allWallets = useMemo(() => {
    return [...wallets];
  }, [wallets]);

  useEffect(() => {
    // Only set up polling if we should fetch
    if (!shouldFetch) {
      return;
    }

    // Load initial data immediately
    fetchWallets();

    // Set up polling interval using dynamic interval
    const intervalMs = getPollingInterval();
    
    // Clear existing interval before setting new one
    if (pollingIntervalRef.current) {
      clearInterval(pollingIntervalRef.current);
    }
    
    pollingIntervalRef.current = setInterval(() => {
      fetchWallets();
    }, intervalMs);

    // Cleanup on unmount
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
        pollingIntervalRef.current = null;
      }
    };
  }, [fetchWallets, shouldFetch, getPollingInterval]);

  return { 
    wallets: allWallets, 
    lastUpdate, 
    error, 
    isLoading,
    isConnected,
    refresh, // Manual refresh function
    addWallet, // Add new wallet immediately
    hasPendingWallets: pendingWallets.length > 0,
  };
}