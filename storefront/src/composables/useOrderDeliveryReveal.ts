import { onBeforeUnmount, onMounted, watch, type Ref } from 'vue'
import type { Order } from '@/api/types'
import { fulfillmentDeliveryLines } from '@/utils/orderPayment'

/** Only delivery content changes should move the page, never a polling timestamp. */
function deliveries(order: Order): Array<{ no: string; signature: string }> {
  return [...(order.children || []), order].flatMap((row) => {
    const f = row.fulfillment
    if (f?.status !== 'delivered') return []
    const instructions = (row.items || []).map((item) => item.instructions)
    const hasInstructions = instructions.some((text) => Object.values(text || {}).some((value) => value?.trim()))
    if (!f.payload?.trim() && !fulfillmentDeliveryLines(f).some((line) => line.trim()) && !hasInstructions) return []
    return [{ no: row.order_no, signature: JSON.stringify([f.payload, f.delivery_data, instructions]) }]
  })
}

/** Reveal the first new delivery after rendering and after the wait overlay unlocks scrolling. */
export function useOrderDeliveryReveal(order: () => Order, root: Ref<HTMLElement | null>) {
  const revealed = new Map<string, string>()
  let frame: number | undefined
  let observer: MutationObserver | undefined
  let disposed = false

  const reveal = () => {
    if (disposed || document.hidden || document.body.style.overflow === 'hidden') return
    const ready = deliveries(order())
    const changed = new Set(ready.filter((row) => revealed.get(row.no) !== row.signature).map((row) => row.no))
    const target = Array.from(root.value?.querySelectorAll<HTMLElement>('[data-order-delivery]') || [])
      .find((element) => changed.has(element.dataset.orderDelivery || ''))
    if (!target) return
    target.scrollIntoView?.({
      block: 'start',
      inline: 'nearest',
      behavior: window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth',
    })
    // Simultaneous child deliveries share one reveal; later deliveries can reveal themselves.
    ready.forEach((row) => revealed.set(row.no, row.signature))
  }
  const schedule = () => {
    if (frame !== undefined) cancelAnimationFrame(frame)
    frame = requestAnimationFrame(() => {
      // Let router scroll restoration and sibling modal updates finish first.
      frame = requestAnimationFrame(reveal)
    })
  }

  watch(() => order().order_no, () => revealed.clear(), { flush: 'sync' })
  watch(() => deliveries(order()), schedule, { immediate: true, flush: 'post' })
  onMounted(() => {
    observer = new MutationObserver(schedule)
    observer.observe(document.body, { attributes: true, attributeFilter: ['style'] })
    document.addEventListener('visibilitychange', schedule)
  })
  onBeforeUnmount(() => {
    disposed = true
    if (frame !== undefined) cancelAnimationFrame(frame)
    observer?.disconnect()
    document.removeEventListener('visibilitychange', schedule)
  })
}
