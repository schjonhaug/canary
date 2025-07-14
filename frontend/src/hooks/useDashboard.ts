import { useEffect, useState } from 'react';

interface WalletMetadata {
  id: number;
  name: string;
  descriptor: string;
  wallet_filename: string;
  created_at: string;
  balance_total: number | null;
  last_activity: string | null;
  contact_count: number | null;
}

interface TransactionEvent {
  id: number;
  wallet_id: number;
  wallet_name: string;
  event_type: 'send' | 'receive';
  amount_sats: number;
  is_confirmed: boolean;
  is_rbf: boolean;
  is_cpfp: boolean;
  balance_total: number | null;
  created_at: string;
}

interface DashboardUpdate {
  timestamp: number;
  wallets: WalletMetadata[];
  events: TransactionEvent[];
}

export function useDashboard() {
  const [wallets, setWallets] = useState<WalletMetadata[]>([]);
  const [events, setEvents] = useState<TransactionEvent[]>([]);
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isConnected, setIsConnected] = useState(false);

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
    isConnected 
  };
}