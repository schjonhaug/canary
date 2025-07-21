import { useEffect, useState } from 'react';
import { Wallet, TransactionEvent, DashboardUpdate } from '../types';
import { getApiBaseUrl } from '../lib/utils';

export function useDashboard() {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [events, setEvents] = useState<TransactionEvent[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  // Load initial data via REST API on mount
  useEffect(() => {
    const loadInitialData = async () => {
      try {
        // Fetch fresh data from REST API
        const baseUrl = getApiBaseUrl();
        const response = await fetch(`${baseUrl}/api/dashboard`);
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
  }, []);

  useEffect(() => {
    // Use the configured API URL or empty string for relative URLs
    const baseUrl = getApiBaseUrl();
    const streamUrl = `${baseUrl}/api/dashboard/stream`;
    
    console.log('Connecting to dashboard stream:', streamUrl);
    
    // Set up Server-Sent Events for real-time dashboard updates
    const eventSource = new EventSource(streamUrl);
    
    eventSource.onopen = () => {
      console.log('Dashboard stream connected');
      setIsConnected(true);
      setError(null);
    };

    eventSource.onmessage = (event) => {
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
    };

    eventSource.onerror = (error) => {
      console.error('Dashboard EventSource failed:', error);
      setIsConnected(false);
      setError(null);
    };

    return () => {
      eventSource.close();
    };
  }, []);

  return { 
    wallets, 
    events, 
    lastUpdate, 
    error, 
    isConnected,
  };
}