import { useEffect, useState, useRef } from 'react';
import { BlockHeader, BlockHeaderState } from '../types';

export function useBlockHeaders(apiUrl: string = 'http://localhost:3000') {
  const [state, setState] = useState<BlockHeaderState>({
    blockHeader: null,
    connected: false,
    reconnecting: false,
    error: null,
  });

  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);

  // Fetch initial block header from REST endpoint
  const fetchInitialBlockHeader = async () => {
    try {
      const response = await fetch(`${apiUrl}/api/block-headers/current`);
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

  const connect = () => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
    }

    setState(prev => ({ ...prev, reconnecting: true, error: null }));

    const eventSource = new EventSource(`${apiUrl}/api/block-headers/stream`);
    eventSourceRef.current = eventSource;

    eventSource.onopen = () => {
      setState(prev => ({ ...prev, connected: true, reconnecting: false, error: null }));
    };

    eventSource.onmessage = (event) => {
      try {
        const blockHeader: BlockHeader = JSON.parse(event.data);
        setState(prev => ({ ...prev, blockHeader, connected: true, reconnecting: false, error: null }));
      } catch (error) {
        console.error('Failed to parse block header:', error);
        setState(prev => ({ ...prev, error: 'Failed to parse block header data' }));
      }
    };

    eventSource.onerror = (error) => {
      console.error('EventSource error:', error);
      setState(prev => ({ ...prev, connected: false, error: 'Connection lost' }));
      
      // Close the connection and attempt to reconnect after 5 seconds
      eventSource.close();
      reconnectTimeoutRef.current = setTimeout(() => {
        connect();
      }, 5000);
    };
  };

  useEffect(() => {
    // Fetch initial data first, then connect to SSE
    fetchInitialBlockHeader().then(() => {
      connect();
    });

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