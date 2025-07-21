import { useEffect, useState, useRef } from 'react';
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

  useEffect(() => {
    const baseUrl = apiUrl ?? getApiBaseUrl();

    // Fetch initial block header from REST endpoint
    const fetchInitialBlockHeader = async () => {
      try {
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

    // Set up SSE connection (simplified to match working dashboard pattern)
    const eventSource = new EventSource(`${baseUrl}/api/block-headers/stream`);
    eventSourceRef.current = eventSource;

    eventSource.onopen = () => {
      console.log('Block header stream connected');
      setState(prev => ({ ...prev, connected: true, reconnecting: false, error: null }));
    };

    eventSource.onmessage = (event) => {
      try {
        const blockHeader: BlockHeader = JSON.parse(event.data);
        setState(prev => ({ ...prev, blockHeader }));
      } catch (error) {
        console.error('Failed to parse block header:', error);
        setState(prev => ({ ...prev, error: 'Failed to parse block header data' }));
      }
    };

    eventSource.onerror = (error) => {
      console.error('Block header EventSource failed:', error);
      setState(prev => ({ ...prev, connected: false, error: 'Connection lost' }));
    };

    return () => {
      if (eventSourceRef.current) {
        eventSourceRef.current.close();
      }
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
      }
    };
  }, [apiUrl]);

  return state;
}