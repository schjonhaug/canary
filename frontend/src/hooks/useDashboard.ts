import { useEffect, useState, useRef, useCallback } from 'react';
import { Wallet, TransactionEvent, DashboardUpdate } from '../types';
import { SSE } from 'sse.js';
import { useAuth } from '../contexts/auth-context';

export function useDashboard() {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [events, setEvents] = useState<TransactionEvent[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const { token, isAuthenticated } = useAuth();

  const eventSourceRef = useRef<SSE | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const maxReconnectAttempts = 5;
  const baseReconnectDelay = 1000; // 1 second

  // Load initial data via REST API on mount
  useEffect(() => {
    // Only fetch data if user is authenticated and has a token
    if (!isAuthenticated || !token) {
      console.log('User not authenticated or no token available, skipping dashboard data fetch');
      return;
    }

    const loadInitialData = async () => {
      try {
        // Fetch fresh data from REST API through Next.js proxy
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

  const connect = useCallback(() => {
    // Don't connect if no token is available or user is not authenticated
    if (!isAuthenticated || !token) {
      console.log('User not authenticated or no token available, skipping SSE connection');
      return;
    }

    // Close existing connection if any
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
    }

    // Clear any existing reconnect timeout
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
    }

    // Connect to dashboard stream through Next.js proxy
    // SSE.js requires absolute URLs
    const streamUrl = `${window.location.origin}/api/dashboard/stream`;
    
    console.log('Connecting to dashboard stream:', streamUrl);
    setError(null);
    
    const authHeaders: Record<string, string> = token ? { 'Authorization': `Bearer ${token}` } : {};
    
    // Set up Server-Sent Events for real-time dashboard updates using sse.js library
    const eventSource = new SSE(streamUrl, {
      headers: authHeaders,
    });
    
    eventSourceRef.current = eventSource;

    // Set connection timeout
    const connectionTimeout = setTimeout(() => {
      console.warn('Dashboard stream connection timeout');
      eventSource.close();
      setIsConnected(false);
      setError('Connection timeout');
      
      // Attempt reconnection
      if (reconnectAttemptsRef.current < maxReconnectAttempts) {
        const delay = baseReconnectDelay * Math.pow(2, reconnectAttemptsRef.current);
        reconnectTimeoutRef.current = setTimeout(() => {
          reconnectAttemptsRef.current++;
          connect();
        }, delay);
      }
    }, 10000); // 10 second timeout
    
    eventSource.addEventListener('open', () => {
      console.log('Dashboard stream connected successfully');
      clearTimeout(connectionTimeout);
      reconnectAttemptsRef.current = 0; // Reset reconnect attempts on successful connection
      setIsConnected(true);
      setError(null);
    });

    eventSource.addEventListener('message', (event: MessageEvent) => {
      try {
        // Ignore ping messages (comments)
        if (event.data.startsWith(':') || event.data.trim() === '') {
          console.log('Received ping/keep-alive');
          return;
        }
        
        console.log('Received dashboard update:', event.data);
        const update: DashboardUpdate = JSON.parse(event.data);
        console.log('Parsed update - wallets:', update.wallets.length, 'events:', update.events.length);
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
      clearTimeout(connectionTimeout);
      setIsConnected(false);
      
      // Attempt reconnection
      if (reconnectAttemptsRef.current < maxReconnectAttempts) {
        const delay = baseReconnectDelay * Math.pow(2, reconnectAttemptsRef.current);
        reconnectTimeoutRef.current = setTimeout(() => {
          reconnectAttemptsRef.current++;
          connect();
        }, delay);
      } else {
        setError('Failed to reconnect after multiple attempts');
      }
    });

    eventSource.stream();
  }, [token, isAuthenticated]);

  useEffect(() => {
    connect();

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
    };
  }, [connect]);

  return { 
    wallets, 
    events, 
    lastUpdate, 
    error, 
    isConnected,
  };
}