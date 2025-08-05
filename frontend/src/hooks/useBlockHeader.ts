import { useEffect, useState, useRef, useCallback } from 'react';
import { BlockHeader } from '../types';
import { useAuth } from '../contexts/auth-context';

// Get polling interval from environment variable (in seconds), default to 60
const POLLING_INTERVAL = (parseInt(process.env.NEXT_PUBLIC_SYNC_INTERVAL || '60') || 60) * 1000;

export function useBlockHeader() {
  const [blockHeader, setBlockHeader] = useState<BlockHeader | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const { token, isAuthenticated } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  const fetchBlockHeader = useCallback(async () => {
    // Only fetch data if user is authenticated and has a token
    if (!isAuthenticated || !token) {
      // User not authenticated, skip block header fetch
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
      
      const response = await fetch('/api/block-headers/current', {
        headers,
      });
      
      if (response.ok) {
        const data: BlockHeader = await response.json();
        setBlockHeader(data);
        setIsConnected(true);
        setError(null);
      } else if (response.status === 404) {
        // No block header found yet - this is normal on startup
        setBlockHeader(null);
        setIsConnected(true);
        setError(null);
      } else {
        console.error('Failed to load block header:', response.status);
        setError('Failed to load block header');
        setIsConnected(false);
      }
    } catch (err) {
      console.error('Failed to fetch block header:', err);
      setError('Failed to load block header');
      setIsConnected(false);
    } finally {
      setIsLoading(false);
    }
  }, [token, isAuthenticated]);

  const refresh = useCallback(() => {
    fetchBlockHeader();
  }, [fetchBlockHeader]);

  useEffect(() => {
    // Load initial data immediately
    fetchBlockHeader();

    // Set up polling interval
    pollingIntervalRef.current = setInterval(() => {
      fetchBlockHeader();
    }, POLLING_INTERVAL);

    // Cleanup on unmount
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
      }
    };
  }, [fetchBlockHeader]);

  return { 
    blockHeader,
    error, 
    isLoading,
    isConnected,
    refresh, // Manual refresh function
  };
}