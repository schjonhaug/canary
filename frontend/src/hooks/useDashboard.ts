import { useEffect, useState } from 'react';
import { Wallet, TransactionEvent, DashboardUpdate } from '../types';
import { getApiBaseUrl } from '../lib/utils';
import { SSE } from 'sse.js';
import { useAuth } from '../contexts/auth-context';

export function useDashboard() {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [events, setEvents] = useState<TransactionEvent[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const { token, isAuthenticated } = useAuth();

  // Load initial data via REST API on mount
  useEffect(() => {
    // Only fetch data if user is authenticated and has a token
    if (!isAuthenticated || !token) {
      console.log('User not authenticated or no token available, skipping dashboard data fetch');
      return;
    }

    const loadInitialData = async () => {
      try {
        // Fetch fresh data from REST API
        const baseUrl = getApiBaseUrl();
        const headers: HeadersInit = {};
        
        // Add Authorization header if token is available
        if (token) {
          headers['Authorization'] = `Bearer ${token}`;
        }
        
        const response = await fetch(`${baseUrl}/api/dashboard`, {
          headers,
        });
        if (response.ok) {
          const data: DashboardUpdate = await response.json();
          setWallets(data.wallets);
          setEvents(data.events);
          setLastUpdate(data.timestamp);
          console.log('Loaded initial dashboard data from API');
        } else {
          console.error('Failed to load initial dashboard data:', response.status);
        }
      } catch (err) {
        console.error('Failed to load initial data:', err);
        setError('Failed to load dashboard data');
      }
    };

    loadInitialData();
  }, [token, isAuthenticated]);

  useEffect(() => {
    // Don't connect if no token is available or user is not authenticated
    if (!isAuthenticated || !token) {
      console.log('User not authenticated or no token available, skipping SSE connection');
      return;
    }

    // Use the configured API URL or empty string for relative URLs
    const baseUrl = getApiBaseUrl();
    const streamUrl = `${baseUrl}/api/dashboard/stream`;
    
    console.log('Connecting to dashboard stream:', streamUrl);
    
    // Set up Server-Sent Events for real-time dashboard updates using sse.js library
    const eventSource = new SSE(streamUrl, {
      headers: {
        'Authorization': `Bearer ${token}`,
      },
    });
    
    eventSource.addEventListener('open', () => {
      console.log('Dashboard stream connected');
      setIsConnected(true);
      setError(null);
    });

    eventSource.addEventListener('message', (event: MessageEvent) => {
      try {
        console.log('Received dashboard update:', event.data);
        const update: DashboardUpdate = JSON.parse(event.data);
        setWallets(update.wallets);
        setEvents(update.events);
        setLastUpdate(update.timestamp);
        setError(null);
        setIsConnected(true);
      } catch (err) {
        console.error('Failed to parse dashboard update:', err);
        setError('Failed to parse dashboard update data');
      }
    });

    eventSource.addEventListener('error', (error: Event) => {
      console.error('Dashboard SSE failed:', error);
      setIsConnected(false);
      setError(null);
    });

    eventSource.stream();

    return () => {
      eventSource.close();
    };
  }, [token, isAuthenticated]);

  return { 
    wallets, 
    events, 
    lastUpdate, 
    error, 
    isConnected,
  };
}