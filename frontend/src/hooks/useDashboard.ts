import { useEffect, useState } from 'react';
import { Wallet, TransactionEvent, DashboardUpdate, CachedData } from '../types';

const CACHE_KEY = 'kanari-dashboard-cache';

export function useDashboard() {
  const [wallets, setWallets] = useState<Wallet[]>([]);
  const [events, setEvents] = useState<TransactionEvent[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [isUsingCache, setIsUsingCache] = useState(false);

  // Load cached data on mount
  useEffect(() => {
    try {
      const cached = localStorage.getItem(CACHE_KEY);
      if (cached) {
        const cachedData: CachedData = JSON.parse(cached);
        // Only use cache if it's less than 5 minutes old
        const fiveMinutesAgo = Date.now() - 5 * 60 * 1000;
        if (cachedData.timestamp > fiveMinutesAgo) {
          setWallets(cachedData.wallets);
          setEvents(cachedData.events);
          setLastUpdate(cachedData.lastUpdate);
          setIsUsingCache(true);
          console.log('Loaded cached dashboard data');
        }
      }
    } catch (err) {
      console.error('Failed to load cached data:', err);
    }
  }, []);

  useEffect(() => {
    // Use the configured API URL or fallback to current hostname
    const baseUrl = process.env.NEXT_PUBLIC_API_URL || '';
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
        setIsUsingCache(false);
        
        // Cache the data
        try {
          const cacheData: CachedData = {
            wallets: update.wallets,
            events: update.events,
            lastUpdate: update.timestamp,
            timestamp: Date.now()
          };
          localStorage.setItem(CACHE_KEY, JSON.stringify(cacheData));
        } catch (cacheErr) {
          console.error('Failed to cache dashboard data:', cacheErr);
        }
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
    isUsingCache
  };
}