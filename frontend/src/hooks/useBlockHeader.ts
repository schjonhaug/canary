import { useEffect, useState, useRef, useCallback } from 'react';
import { BlockHeader } from '../types';
import { useAuth } from '../contexts/auth-context';
import { getApiBaseUrl } from '../lib/utils';

export function useBlockHeader() {
  const [blockHeader, setBlockHeader] = useState<BlockHeader | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [isConnected, setIsConnected] = useState(true);
  const { billingStatus } = useAuth();

  const pollingIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Get polling interval from billing status sync interval or default to 60 seconds
  const getPollingInterval = useCallback(() => {
    const syncIntervalSeconds = billingStatus?.limits?.sync_interval_seconds || 60;
    return syncIntervalSeconds * 1000; // Convert to milliseconds
  }, [billingStatus?.limits?.sync_interval_seconds]);

  const fetchBlockHeader = useCallback(async () => {
    setIsLoading(true);
    setError(null);

    try {
      // Use credentials: 'include' to send HttpOnly auth cookie
      const baseUrl = getApiBaseUrl();
      const response = await fetch(`${baseUrl}/api/block-headers/current`, {
        credentials: 'include',
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
  }, []);

  const refresh = useCallback(() => {
    fetchBlockHeader();
  }, [fetchBlockHeader]);

  useEffect(() => {
    // Load initial data immediately
    fetchBlockHeader();

    // Set up polling interval using dynamic interval
    const intervalMs = getPollingInterval();
    pollingIntervalRef.current = setInterval(() => {
      fetchBlockHeader();
    }, intervalMs);

    // Cleanup on unmount
    return () => {
      if (pollingIntervalRef.current) {
        clearInterval(pollingIntervalRef.current);
      }
    };
  }, [fetchBlockHeader, getPollingInterval]);

  return { 
    blockHeader,
    error, 
    isLoading,
    isConnected,
    refresh, // Manual refresh function
  };
}