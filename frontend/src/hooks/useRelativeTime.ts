import { useState, useEffect } from 'react';
import { formatDistanceToNow, Locale } from 'date-fns';
import { useLocale } from 'next-intl';
import { enUS, nb, es, pt, de, fr, ja, da } from 'date-fns/locale';

const localeMap: Record<string, Locale> = {
  en: enUS,
  no: nb,
  es: es,
  pt: pt,
  de: de,
  fr: fr,
  ja: ja,
  da: da,
};

export function useRelativeTime(timestamp: number | undefined, updateInterval = 60000) {
  const locale = useLocale();
  const [relativeTime, setRelativeTime] = useState<string>('');

  useEffect(() => {
    if (!timestamp) return;

    const updateTime = () => {
      const dateFnsLocale = localeMap[locale] || enUS;
      setRelativeTime(formatDistanceToNow(new Date(timestamp * 1000), {
        addSuffix: true,
        locale: dateFnsLocale
      }));
    };

    // Initial update
    updateTime();

    // Set up interval for updates
    const interval = setInterval(updateTime, updateInterval);

    return () => clearInterval(interval);
  }, [timestamp, updateInterval, locale]);

  return relativeTime;
}