import { useEffect, useState, useRef, useCallback } from 'react';
import { BlockHeader, BlockHeaderState } from '../types';
import { getApiBaseUrl } from '../lib/utils';

export function useBlockHeaders(apiUrl?: string) {
  const [state, setState] = useState<BlockHeaderState>({
    blockHeader: null,
    connected: false,
    reconnecting: false,
    error: null,
  });

  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const maxReconnectAttempts = 5;
  const baseReconnectDelay = 1000; // 1 second

  const connect = useCallback(() => {
    // Close existing connection if any
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
    }

    // Clear any existing reconnect timeout
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
    }

    console.log('Connecting to block header stream...');
    setState(prev => ({ ...prev, reconnecting: true, error: null }));

    // Connect directly to backend, bypassing Next.js API routes
    const backendUrl = process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3000';
    const eventSource = new EventSource(`${backendUrl}/api/block-headers/stream`);
    eventSourceRef.current = eventSource;

    // Set connection timeout
    const connectionTimeout = setTimeout(() => {
      console.warn('Block header stream connection timeout');
      eventSource.close();
      setState(prev => ({ 
        ...prev, 
        connected: false, 
        reconnecting: false, 
        error: 'Connection timeout' 
      }));
      
      // Attempt reconnection
      if (reconnectAttemptsRef.current < maxReconnectAttempts) {
        const delay = baseReconnectDelay * Math.pow(2, reconnectAttemptsRef.current);
        reconnectTimeoutRef.current = setTimeout(() => {
          reconnectAttemptsRef.current++;
          connect();
        }, delay);
      }
    }, 10000); // 10 second timeout

    eventSource.onopen = () => {
      console.log('Block header stream connected');
      clearTimeout(connectionTimeout);
      reconnectAttemptsRef.current = 0; // Reset reconnect attempts on successful connection
      setState(prev => ({ 
        ...prev, 
        connected: true, 
        reconnecting: false, 
        error: null 
      }));
    };

    eventSource.onmessage = (event) => {
      try {
        // Ignore ping messages (comments)
        if (event.data.startsWith(':') || event.data.trim() === '') {
          return;
        }
        
        const blockHeader: BlockHeader = JSON.parse(event.data);
        setState(prev => ({ ...prev, blockHeader }));
      } catch (error) {
        console.error('Failed to parse block header:', error);
        setState(prev => ({ ...prev, error: 'Failed to parse block header data' }));
      }
    };

    eventSource.onerror = (error) => {
      console.error('Block header EventSource failed:', error);
      clearTimeout(connectionTimeout);
      setState(prev => ({ ...prev, connected: false, reconnecting: false }));
      
      // Attempt reconnection
      if (reconnectAttemptsRef.current < maxReconnectAttempts) {
        const delay = baseReconnectDelay * Math.pow(2, reconnectAttemptsRef.current);
        reconnectTimeoutRef.current = setTimeout(() => {
          reconnectAttemptsRef.current++;
          connect();
        }, delay);
      } else {
        setState(prev => ({ ...prev, error: 'Failed to reconnect after multiple attempts' }));
      }
    };
  }, [apiUrl]);

  useEffect(() => {
    // Fetch initial block header from REST endpoint
    const fetchInitialBlockHeader = async () => {
      try {
        const baseUrl = apiUrl ?? getApiBaseUrl();
        const response = await fetch(`${baseUrl}/api/block-headers/current`);
        if (response.ok) {
          const blockHeader: BlockHeader = await response.json();
          setState(prev => ({ ...prev, blockHeader }));
        } else if (response.status !== 404) {
          // 404 is expected if no block header is stored yet
          console.warn('Failed to fetch initial block header:', response.status);
        }
      } catch (error) {
        console.error('Error fetching initial block header:', error);
      }
    };

    // Load initial data first
    fetchInitialBlockHeader();

    // Connect to SSE stream
    connect();

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
    };
  }, [connect, apiUrl]);

  return state;
}