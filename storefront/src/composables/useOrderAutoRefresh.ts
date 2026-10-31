import { onBeforeUnmount, onMounted, watch, type Ref } from 'vue'
import type { Order } from '@/api/types'

// Keep delivery progress responsive without repeatedly submitting purchases upstream.
const REFRESH_INTERVAL_MS = 3_000
const ACTIVE_STATUSES = new Set(['pending_payment', 'paid', 'fulfilling', 'partially_delivered'])
const ACTIVE_REFUNDS = new Set(['checking', 'processing', 'syncing'])

function needsRefresh(order: Order | null): boolean {
  if (!order) return false
  if (ACTIVE_REFUNDS.has(order.refund_state || '')) return true
  if (['canceled', 'expired', 'refunded'].includes(order.status)) return false
  return ACTIVE_STATUSES.has(order.status) || Boolean(order.children?.some(needsRefresh))
}

/** Refresh the full detail, including child deliveries; leave finished orders idle. */
export function useOrderAutoRefresh(order: Ref<Order | null>, loading: Ref<boolean>, refresh: () => Promise<void>) {
  let timer: ReturnType<typeof setTimeout> | undefined
  let stopped = false
  let refreshing = false

  const clearTimer = () => {
    clearTimeout(timer)
    timer = undefined
  }
  const schedule = () => {
    clearTimer()
    if (!stopped && !loading.value && !refreshing && !document.hidden && needsRefresh(order.value)) {
      timer = setTimeout(() => void run(), REFRESH_INTERVAL_MS)
    }
  }
  const run = async () => {
    clearTimer()
    if (stopped || loading.value || refreshing || document.hidden || !needsRefresh(order.value)) return
    refreshing = true
    try {
      await refresh()
    } finally {
      refreshing = false
      schedule()
    }
  }
  const resume = () => {
    if (document.hidden) clearTimer()
    else void run()
  }

  watch([order, loading], schedule, { deep: true })
  onMounted(() => {
    document.addEventListener('visibilitychange', resume)
    window.addEventListener('focus', resume)
    schedule()
  })
  onBeforeUnmount(() => {
    stopped = true
    clearTimer()
    document.removeEventListener('visibilitychange', resume)
    window.removeEventListener('focus', resume)
  })
}
