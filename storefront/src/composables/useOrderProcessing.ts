import { computed, onBeforeUnmount, ref, watch, type Ref } from 'vue'
import type { Order } from '@/api/types'
import { hasDeliveredOrder } from '@/utils/exportOrder'

// Offer recovery after one minute without delivery, independently of polling responses.
const LONG_WAIT_MS = 60_000
const PROCESSING_STATUSES = new Set(['paid', 'fulfilling', 'partially_delivered'])
const FINAL_STATUSES = new Set(['completed', 'delivered', 'canceled', 'expired', 'refunded'])

function isProcessing(order: Order): boolean {
  // A partially delivered purchase must show the cards already received.
  if (hasDeliveredOrder(order)) return false
  if (FINAL_STATUSES.has(order.status)) return false
  return PROCESSING_STATUSES.has(order.status) || Boolean(order.children?.some(isProcessing))
}

export function useOrderProcessing(order: Readonly<Ref<Order | null>>) {
  const dismissed = ref(false)
  const longWait = ref(false)
  const processingOrderNo = computed(() => order.value && isProcessing(order.value) ? order.value.order_no : '')
  const open = computed(() => Boolean(processingOrderNo.value) && !dismissed.value)
  let timer: ReturnType<typeof setTimeout> | undefined

  const continueWaiting = () => {
    clearTimeout(timer)
    longWait.value = false
    timer = setTimeout(() => { longWait.value = true }, LONG_WAIT_MS)
  }
  const dismiss = () => {
    dismissed.value = true
    clearTimeout(timer)
  }

  watch(processingOrderNo, (no) => {
    clearTimeout(timer)
    dismissed.value = false
    longWait.value = false
    if (no) continueWaiting()
  }, { immediate: true })
  onBeforeUnmount(() => clearTimeout(timer))

  return { open, longWait, continueWaiting, dismiss }
}
