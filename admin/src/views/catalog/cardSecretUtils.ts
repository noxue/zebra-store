// Pure helpers shared by 卡密库存 / 卡密导入 / 卡密导出 (ported from the original views).
import type { AdminCardSecret, AdminCardSecretBatch, AdminProduct, AdminProductSKU } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { formatSkuSpecValues } from '@/utils/sku'

export const CARD_SECRET_PAGE_SIZE_OPTIONS = [10, 20, 50, 100, 200]

export type CardSecretStatus = 'available' | 'reserved' | 'used'
export const CARD_SECRET_STATUSES: CardSecretStatus[] = ['available', 'reserved', 'used']

export const parsePositiveInteger = (value: unknown) => {
  const parsed = Number(value)
  if (!Number.isFinite(parsed) || parsed <= 0) return 0
  return Math.floor(parsed)
}

export const buildSkuLabel = (sku: Pick<AdminProductSKU, 'id' | 'sku_code' | 'spec_values'> | null | undefined, locale?: string) => {
  const skuCode = String(sku?.sku_code || '').trim()
  const specText = formatSkuSpecValues(sku?.spec_values, locale)
  if (skuCode && specText) return `${skuCode} · ${specText}`
  if (skuCode) return skuCode
  if (specText) return specText
  if (sku?.id) return `#${sku.id}`
  return '-'
}

export const buildProductLabel = (product: Pick<AdminProduct, 'id' | 'title'> | null | undefined) => {
  const id = Number(product?.id || 0)
  const name = getLocalizedText(product?.title || {})
  if (id > 0 && name) return `#${id} ${name}`
  if (id > 0) return `#${id}`
  return name || '-'
}

export const buildBatchLabel = (batch: Partial<AdminCardSecretBatch> | null | undefined) => {
  const id = Number(batch?.id || 0)
  const batchNo = String(batch?.batch_no || '').trim()
  if (id > 0 && batchNo) return `#${id} ${batchNo}`
  if (id > 0) return `#${id}`
  return batchNo || '-'
}

export const resolveSecretBatchLabel = (secret: Pick<AdminCardSecret, 'batch' | 'batch_id'>) => {
  if (secret.batch) return buildBatchLabel(secret.batch)
  if (secret.batch_id) return `#${secret.batch_id}`
  return '-'
}

/** Split pasted secrets: one per line, trimmed, blanks dropped. */
export const parseSecretLines = (raw: string) =>
  String(raw || '')
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean)

/** De-duplicated positive integer ids. */
export const normalizeIds = (ids: unknown[]) =>
  Array.from(new Set(ids.map((item) => Number(item)).filter((item) => Number.isFinite(item) && item > 0).map((item) => Math.floor(item))))

export const statusTone = (status: string) => (status === 'available' ? 'success' : status === 'reserved' ? 'warning' : 'neutral')
