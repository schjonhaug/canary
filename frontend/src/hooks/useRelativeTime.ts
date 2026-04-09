import { useState, useEffect } from 'react';
import { formatDistance, Locale } from 'date-fns';
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

export function formatRelativeTime(timestamp: number, locale: string, now = Date.now()) {
  const dateFnsLocale = localeMap[locale] || enUS;
  return formatDistance(new Date(timestamp * 1000), new Date(now), {
    addSuffix: true,
    locale: dateFnsLocale
  });
}

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
