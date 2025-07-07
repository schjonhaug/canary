'use client';

import { useBlockHeaders } from '@/hooks/useBlockHeaders';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';

export function BlockStatus() {
  const { blockHeader, connected, reconnecting, error } = useBlockHeaders(
    process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3000'
  );

  const formatTimestamp = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  const formatTimeAgo = (timestamp: number) => {
    const now = Date.now();
    const diff = now - (timestamp * 1000);
    
    const rtf = new Intl.RelativeTimeFormat('en', { numeric: 'auto' });
    
    if (diff < 60000) return rtf.format(-Math.floor(diff / 1000), 'second');
    if (diff < 3600000) return rtf.format(-Math.floor(diff / 60000), 'minute');
    if (diff < 86400000) return rtf.format(-Math.floor(diff / 3600000), 'hour');
    return rtf.format(-Math.floor(diff / 86400000), 'day');
  };

  const truncateHash = (hash: string) => {
    return `${hash.slice(0, 8)}...${hash.slice(-8)}`;
  };

  const getConnectionStatus = () => {
    if (reconnecting) return { text: 'Reconnecting...', variant: 'secondary' as const };
    if (!connected || error) return { text: 'Disconnected', variant: 'destructive' as const };
    return { text: 'Connected', variant: 'default' as const };
  };

  const connectionStatus = getConnectionStatus();

  return (
    <Card className="w-full">
      <CardContent className="p-4">
        <div className="flex items-center justify-between space-x-4">
          <div className="flex items-center space-x-3">
            <div className="flex items-center space-x-2">
              <span className="text-sm font-medium">Blockchain:</span>
              <Badge variant={connectionStatus.variant}>
                {connectionStatus.text}
              </Badge>
            </div>
            
            {blockHeader && (
              <>
                <div className="h-4 w-px bg-border" />
                <div className="flex items-center space-x-4 text-sm">
                  <div>
                    <span className="text-muted-foreground">Block:</span>{' '}
                    <span className="font-mono">{blockHeader.height.toLocaleString()}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Hash:</span>{' '}
                    <span className="font-mono">{truncateHash(blockHeader.hash)}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Time:</span>{' '}
                    <span className="font-mono">{formatTimeAgo(blockHeader.timestamp)}</span>
                  </div>
                </div>
              </>
            )}
          </div>

          {error && (
            <div className="text-sm text-destructive">
              {error}
            </div>
          )}
        </div>

        {blockHeader && (
          <div className="mt-2 text-xs text-muted-foreground">
            Last updated: {formatTimestamp(blockHeader.timestamp)}
          </div>
        )}
      </CardContent>
    </Card>
  );
}