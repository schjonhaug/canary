import { useLocale } from 'next-intl'
import { formatBitcoinAmount, formatTransactionAmount, formatDateTime, formatBtcAmount } from '@/lib/utils'
import { formatFiatAmount } from '@/lib/currencies'

/**
 * Hook that provides locale-aware formatting functions.
 * Uses the user's preferred locale from next-intl for consistent formatting
 * across the application, matching the backend's ICU4X-based formatting.
 */
export function useFormatters() {
  const locale = useLocale()

  return {
    formatBitcoinAmount: (sats: number | null | undefined) =>
      formatBitcoinAmount(sats, locale),
    formatTransactionAmount: (sats: number | null | undefined, eventType?: 'send' | 'receive') =>
      formatTransactionAmount(sats, eventType, locale),
    formatDateTime: (dateTime: string | number) =>
      formatDateTime(dateTime, locale),
    formatFiatAmount: (amount: number, currencyCode: string) =>
      formatFiatAmount(amount, currencyCode, locale),
    formatBtcAmount: (btc: number) =>
      formatBtcAmount(btc, locale),
    formatNumber: (num: number) => num.toLocaleString(locale),
    locale,
  }
}
