import { useState, useEffect } from 'react';
import { useLocale } from 'next-intl';
import { formatRelativeTime } from '@/lib/wallet-time';

export function useRelativeTime(timestamp: number | undefined, updateInterval = 60000) {
  const locale = useLocale();
  const [relativeTime, setRelativeTime] = useState<string>('');

  useEffect(() => {
    if (!timestamp) return;

    const updateTime = () => {
      setRelativeTime(formatRelativeTime(timestamp, locale));
    };

    // Initial update
    updateTime();

    // Set up interval for updates
    const interval = setInterval(updateTime, updateInterval);

    return () => clearInterval(interval);
  }, [timestamp, updateInterval, locale]);

  return relativeTime;
}
