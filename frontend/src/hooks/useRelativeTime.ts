import { useState, useEffect } from 'react';
import { formatDistanceToNow, Locale } from 'date-fns';
import { useLocale } from 'next-intl';
import { enUS, nb, es, ptBR, de, fr, ja, da, sv } from 'date-fns/locale';

const localeMap: Record<string, Locale> = {
  'en-US': enUS,
  nb: nb,
  'es-419': es,
  'pt-BR': ptBR,
  'de-DE': de,
  'fr-FR': fr,
  ja: ja,
  da: da,
  sv: sv,
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