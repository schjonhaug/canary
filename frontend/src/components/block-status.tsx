'use client';

import { useBlockHeaders } from '@/hooks/useBlockHeaders';
import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { useRelativeTime } from '@/hooks/useRelativeTime';

export function BlockStatus() {
  const { blockHeader, connected, reconnecting, error } = useBlockHeaders();
  const blockHeaderTime = useRelativeTime(blockHeader?.timestamp);

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
                    <span className="text-muted-foreground">Block height:</span>{' '}
                    <span className="font-mono">{blockHeader.height.toLocaleString()}</span>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Time:</span>{' '}
                    <span>{blockHeaderTime}</span>
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

      </CardContent>
    </Card>
  );
}