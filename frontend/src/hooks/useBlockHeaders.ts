import { useEffect, useState, useRef } from 'react';

export interface BlockHeader {
  height: number;
  hash: string;
  timestamp: number;
}

export interface BlockHeaderState {
  blockHeader: BlockHeader | null;
  connected: boolean;
  reconnecting: boolean;
  error: string | null;
}

export function useBlockHeaders(apiUrl: string = 'http://localhost:3000') {
  const [state, setState] = useState<BlockHeaderState>({
    blockHeader: null,
    connected: false,
    reconnecting: false,
    error: null,
  });

  const eventSourceRef = useRef<EventSource | null>(null);
  const reconnectTimeoutRef = useRef<NodeJS.Timeout | null>(null);

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
    connect();

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