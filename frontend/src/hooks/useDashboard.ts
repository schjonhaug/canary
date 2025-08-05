import { useEffect, useState, useRef, useCallback } from 'react';
import { Wallet, TransactionEvent, DashboardUpdate, BlockHeader } from '../types';
import { useAuth } from '../contexts/auth-context';

// Get polling interval from environment variable (in seconds), default to 60
const POLLING_INTERVAL = (parseInt(process.env.NEXT_PUBLIC_SYNC_INTERVAL || '60') || 60) * 1000;

export function useDashboard() {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [events, setEvents] = useState<TransactionEvent[]>([]);
  const [blockHeader, setBlockHeader] = useState<BlockHeader | null>(null);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const { token, isAuthenticated } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  const fetchDashboard = useCallback(async () => {
    // Only fetch data if user is authenticated and has a token
    if (!isAuthenticated || !token) {
      // User not authenticated, skip dashboard data fetch
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
      
      const response = await fetch('/api/dashboard', {
        headers,
      });
      
      if (response.ok) {
        const data: DashboardUpdate = await response.json();
        setWallets(data.wallets);
        setEvents(data.events);
        setBlockHeader(data.current_block_header);
        setLastUpdate(data.timestamp);
        setIsConnected(true);
        setError(null);
        console.log('Dashboard updated: wallets:', data.wallets.length, 'events:', data.events.length);
      } else {
        console.error('Failed to load dashboard data:', response.status);
        setError('Failed to load dashboard data');
        setIsConnected(false);
      }
    } catch (err) {
      console.error('Failed to fetch dashboard:', err);
      setError('Failed to load dashboard data');
      setIsConnected(false);
    } finally {
      setIsLoading(false);
    }
  }, [token, isAuthenticated]);

  const refresh = useCallback(() => {
    fetchDashboard();
  }, [fetchDashboard]);

  useEffect(() => {
    // Load initial data
    fetchDashboard();

    // Set up polling interval
    pollingIntervalRef.current = setInterval(() => {
      fetchDashboard();
    }, POLLING_INTERVAL);

    // Cleanup on unmount
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
      }
    };
  }, [fetchDashboard]);

  return { 
    wallets, 
    events, 
    blockHeader,
    lastUpdate, 
    error, 
    isLoading,
    isConnected,
    refresh, // Manual refresh function
  };
}