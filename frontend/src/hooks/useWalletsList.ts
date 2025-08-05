import { useEffect, useState, useRef, useCallback } from 'react';
import { Wallet } from '../types';
import { useAuth } from '../contexts/auth-context';

// Get polling interval from environment variable (in seconds), default to 60
const POLLING_INTERVAL = (parseInt(process.env.NEXT_PUBLIC_SYNC_INTERVAL || '60') || 60) * 1000;

interface WalletsListResponse {
  timestamp: number;
  wallets: Wallet[];
}

export function useWalletsList() {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const { token, isAuthenticated } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  const fetchWallets = useCallback(async () => {
    // Only fetch data if user is authenticated and has a token
    if (!isAuthenticated || !token) {
      // User not authenticated, skip wallet data fetch
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
        console.log('Wallets list updated:', data.wallets.length, 'wallets');
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
  }, [token, isAuthenticated]);

  const refresh = useCallback(() => {
    fetchWallets();
  }, [fetchWallets]);

  useEffect(() => {
    // Load initial data immediately
    fetchWallets();

    // Set up polling interval
    pollingIntervalRef.current = setInterval(() => {
      fetchWallets();
    }, POLLING_INTERVAL);

    // Cleanup on unmount
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
      }
    };
  }, [fetchWallets]);

  return { 
    wallets, 
    lastUpdate, 
    error, 
    isLoading,
    isConnected,
    refresh, // Manual refresh function
  };
}