import { computed, ref, type Ref } from 'vue'
import DOMPurify from 'dompurify'
import { useI18n } from 'vue-i18n'
import type { Fulfillment, Order, OrderItem } from '@/api/types'
import { useLocalized } from '@/composables/useLocalized'
import { toast } from '@/composables/useToast'
import { useAppStore } from '@/stores/app'
import { copyText } from '@/utils/clipboard'
import { formatDateTime } from '@/utils/format'
import { fulfillmentStatusLabel, fulfillmentTypeLabel } from '@/utils/fulfillment'
import { getImageUrl } from '@/utils/image'
import {
  fulfillmentDeliveryLines,
  hasPositive,
  isFulfillmentTruncated,
  itemDiscountTotal,
  itemPaidAmount,
  manualSubmissionRows,
  resolveChildStatus,
} from '@/utils/orderPayment'
import { buildSkuDisplayTextFromSnapshot } from '@/utils/sku'

const INSTRUCTION_SANITIZE = {
  ALLOWED_TAGS: ['p', 'br', 'strong', 'em', 'u', 's', 'code', 'pre', 'blockquote', 'ul', 'ol', 'li', 'a', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'span', 'div', 'img', 'hr'],
  ALLOWED_ATTR: ['href', 'target', 'rel', 'src', 'alt', 'title'],
  FORBID_ATTR: ['style', 'class', 'id'],
  ALLOW_DATA_ATTR: false,
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|tel:|#|\/(?!\/))/i,
}

export type OrderDisplayHelpers = ReturnType<typeof useOrderDisplayHelpers>

/** Display helpers shared by OrderDetail and GuestOrderDetail. */
export function useOrderDisplayHelpers(order: Ref<Order | null>) {
  const { t } = useI18n()
  const appStore = useAppStore()
  const { getLocalizedText, formatPrice } = useLocalized()
  const fulfillmentCopied = ref(false)
  let copiedTimer: ReturnType<typeof setTimeout> | undefined

  const showTimeCard = computed(() => Boolean(order.value && (order.value.paid_at || order.value.expires_at || order.value.canceled_at)))
  const showRefundRecordsCard = computed(() => ['refunded', 'partially_refunded'].includes(String(order.value?.status || '')))
  const refundRecords = computed(() => (Array.isArray(order.value?.refund_records) ? order.value.refund_records : []))

  const money = (amount: string | undefined | null, currency?: string) => formatPrice(amount, currency ?? order.value?.currency ?? null)
  const discountMoney = (amount: string | undefined | null, currency?: string) =>
    hasPositive(amount) ? `-${money(amount, currency)}` : money(amount, currency)

  const instructionBlocks = (items: OrderItem[] | null | undefined): Array<{ title: string; html: string }> => {
    if (!Array.isArray(items)) return []
    const seen = new Set<string>()
    const blocks: Array<{ title: string; html: string }> = []
    for (const item of items) {
      const html = String(getLocalizedText(item.instructions) || '').trim()
      if (!html || seen.has(html)) continue
      seen.add(html)
      blocks.push({ title: getLocalizedText(item.title), html: DOMPurify.sanitize(html, INSTRUCTION_SANITIZE) })
    }
    return blocks
  }

  const handleCopyFulfillment = async (fulfillment: Fulfillment | null | undefined) => {
    const lines = fulfillmentDeliveryLines(fulfillment)
    const text = lines.length > 0 ? lines.join('\n') : fulfillment?.payload || ''
    if (!text) return
    try {
      await copyText(text)
      fulfillmentCopied.value = true
      clearTimeout(copiedTimer)
      copiedTimer = setTimeout(() => {
        fulfillmentCopied.value = false
      }, 1500)
    } catch {
      toast.error(t('zs.copyFailed'))
    }
  }

  return {
    showTimeCard,
    showRefundRecordsCard,
    refundRecords,
    money,
    discountMoney,
    hasPositive,
    formatDate: formatDateTime,
    getLocalizedText,
    fulfillmentTypeText: (type?: string) => fulfillmentTypeLabel(t, type),
    fulfillmentStatusText: (status?: string) => fulfillmentStatusLabel(t, status),
    resolvedChildStatus: (child: Order) => resolveChildStatus(child, order.value),
    isFulfillmentTruncated,
    fulfillmentDeliveryLines,
    refundReasonText: (remark?: string) => String(remark || '').trim() || t('orderDetail.refundRecordReasonEmpty'),
    orderItemImage: (item: OrderItem) => getImageUrl(String(item.sku_snapshot?.image || '').trim()),
    orderItemSkuText: (item: OrderItem) => buildSkuDisplayTextFromSnapshot(item.sku_snapshot, { locale: appStore.locale, fallback: t('productDetail.skuFallback') }),
    itemDiscountTotal,
    itemPaidAmount,
    manualRows: (item: OrderItem) => manualSubmissionRows(item.manual_form_submission, item.manual_form_schema_snapshot, appStore.locale),
    instructionBlocks,
    fulfillmentCopied,
    handleCopyFulfillment,
  }
}
