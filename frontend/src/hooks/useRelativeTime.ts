import { useState, useEffect } from 'react';
import { formatDistanceToNow } from 'date-fns';

export function useRelativeTime(timestamp: number | undefined, updateInterval = 60000) {
  const [relativeTime, setRelativeTime] = useState<string>('');

  useEffect(() => {
    if (!timestamp) return;

    const updateTime = () => {
      setRelativeTime(formatDistanceToNow(new Date(timestamp * 1000), { addSuffix: true }));
    };

    // Initial update
    updateTime();

    // Set up interval for updates
    const interval = setInterval(updateTime, updateInterval);

    return () => clearInterval(interval);
  }, [timestamp, updateInterval]);

  return relativeTime;
}