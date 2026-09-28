import { computed } from 'vue'
import type { LocalizedText } from '@/api/types'
import { useAppStore } from '@/stores/app'
import { formatMoney } from '@/utils/money'
import { localizedText } from '@/utils/localized'

/** Localized field resolution and price formatting (`"12.34 CNY"`). */
export function useLocalized() {
  const appStore = useAppStore()
  const getLocalizedText = (value: LocalizedText | Record<string, unknown> | string | null | undefined): string =>
    localizedText(value, appStore.locale)
  const siteCurrency = computed(() => appStore.currency)
  const formatPrice = (amount: string | number | null | undefined, currency?: string | null): string =>
    formatMoney(amount, currency === undefined ? siteCurrency.value : currency)
  return { getLocalizedText, siteCurrency, formatPrice }
}
