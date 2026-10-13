import DOMPurify from 'dompurify'
import type { Fulfillment, Order } from '@/api/types'
import { localizedText } from '@/utils/localized'
import { fulfillmentDeliveryLines, isFulfillmentTruncated } from '@/utils/orderPayment'

export interface OrderExportLabels {
  orderNo: string
  keys: string
  instructions: string
}

const plainText = (html: string): string => {
  const withBreaks = html.replace(/<br\s*\/?\s*>/gi, '\n').replace(/<\/(?:p|div|li|h[1-6])\s*>/gi, '\n')
  const safe = DOMPurify.sanitize(withBreaks, { ALLOWED_TAGS: [], ALLOWED_ATTR: [] })
  const element = document.createElement('div')
  element.innerHTML = safe
  return (element.textContent || '').trim()
}

/** DLV-02: export only delivery data already visible to this authenticated buyer. */
export async function orderExportText(
  order: Order,
  locale: string,
  labels: OrderExportLabels,
  downloadFulfillment: (orderNo: string) => Promise<Blob>,
): Promise<string> {
  const lines = [`${labels.orderNo}: ${order.order_no}`]
  const deliveries = order.children?.length ? order.children : [order]
  for (const child of deliveries) {
    const fulfillment: Fulfillment | null | undefined = child.fulfillment
    if (fulfillment?.status !== 'delivered') continue
    if (deliveries.length > 1) lines.push('', `${labels.orderNo}: ${child.order_no}`)
    const structured = fulfillmentDeliveryLines(fulfillment)
    const payload = isFulfillmentTruncated(fulfillment)
      ? await downloadFulfillment(child.order_no).then((blob) => blob.text())
      : structured.length ? structured.join('\n') : fulfillment.payload || ''
    lines.push('', `${labels.keys}:`, payload.trim())
    const instructions = [...new Set((child.items || [])
      .map((item) => plainText(localizedText(item.instructions, locale)))
      .filter(Boolean))]
    lines.push('', `${labels.instructions}:`, instructions.join('\n\n'))
  }
  return `${lines.join('\n').trimEnd()}\n`
}

export const hasDeliveredOrder = (order: Order): boolean =>
  (order.children?.length ? order.children : [order]).some((child) => child.fulfillment?.status === 'delivered')
