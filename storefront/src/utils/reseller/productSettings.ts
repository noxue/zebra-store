import type {
  Pagination,
} from '@/api/client'
import type {
  ResellerProductSettingData,
  ResellerProductSettingDetailData,
  ResellerProductSettingPayloadItem,
  ResellerProductSettingUpdatePayload,
} from '@/api/types'
import { amountToCents, centsToAmount } from '@/utils/money'
import {
  RESELLER_PRICING_MODE_FIXED_MARKUP,
  RESELLER_PRICING_MODE_FIXED_PRICE,
  RESELLER_PRICING_MODE_INHERIT,
  RESELLER_PRICING_MODE_MARKUP_PERCENT,
} from './constants'

export interface ResellerProductSettingFormItem {
  sku_id: number
  is_listed?: boolean
  pricing_mode?: string
  markup_percent?: string
  fixed_markup_amount?: string
  fixed_price_amount?: string
  sort_order?: number
}

type DetailLike = Pick<ResellerProductSettingDetailData, 'product_setting'> & { skus?: ResellerProductSettingDetailData['skus'] }

const moneyOrZero = (value?: string) => String(value || '').trim() || '0.00'
const isRecord = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v)
const positiveInt = (value: unknown, fallback: number) => {
  const n = Number(value)
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : fallback
}

export const isResellerProductSettingDetail = (value: unknown): value is ResellerProductSettingDetailData =>
  isRecord(value) && isRecord(value.product) && Number.isFinite(Number(value.product.id)) && Array.isArray(value.skus)

export const normalizeResellerProductSettingsPagination = (value: unknown, fallback: Pagination = { page: 1, page_size: 20, total: 0, total_page: 1 }): Pagination => {
  if (!isRecord(value)) return { ...fallback }
  const total = Number(value.total)
  return {
    page: positiveInt(value.page, fallback.page),
    page_size: positiveInt(value.page_size, fallback.page_size),
    total: Number.isFinite(total) && total >= 0 ? Math.floor(total) : fallback.total,
    total_page: positiveInt(value.total_page, fallback.total_page || 1),
  }
}

export const getResellerPricingModeLabelKey = (mode?: string): string => {
  if (mode === RESELLER_PRICING_MODE_INHERIT) return 'inherit'
  if (mode === RESELLER_PRICING_MODE_MARKUP_PERCENT) return 'markupPercent'
  if (mode === RESELLER_PRICING_MODE_FIXED_MARKUP) return 'fixedMarkup'
  if (mode === RESELLER_PRICING_MODE_FIXED_PRICE) return 'fixedPrice'
  return 'unknown'
}

export const normalizeResellerProductSettingForm = (raw: ResellerProductSettingFormItem): ResellerProductSettingPayloadItem => ({
  sku_id: Number(raw.sku_id || 0),
  is_listed: raw.is_listed !== false,
  pricing_mode: raw.pricing_mode || RESELLER_PRICING_MODE_INHERIT,
  markup_percent: moneyOrZero(raw.markup_percent),
  fixed_markup_amount: moneyOrZero(raw.fixed_markup_amount),
  fixed_price_amount: moneyOrZero(raw.fixed_price_amount),
  sort_order: Number(raw.sort_order || 0),
})

export const formFromSetting = (setting: ResellerProductSettingData | undefined, skuId: number): ResellerProductSettingPayloadItem =>
  normalizeResellerProductSettingForm({
    sku_id: skuId,
    is_listed: setting?.is_listed,
    pricing_mode: setting?.pricing_mode,
    markup_percent: setting?.markup_percent,
    fixed_markup_amount: setting?.fixed_markup_amount,
    fixed_price_amount: setting?.fixed_price_amount,
    sort_order: setting?.sort_order,
  })

/** Only the value field of the selected pricing mode is sent; others are zeroed. */
export const buildResellerProductSettingPayload = (items: ResellerProductSettingFormItem[]): ResellerProductSettingUpdatePayload => ({
  settings: items.map((item) => {
    const n = normalizeResellerProductSettingForm(item)
    return {
      sku_id: n.sku_id,
      is_listed: n.is_listed,
      pricing_mode: n.pricing_mode,
      markup_percent: n.pricing_mode === RESELLER_PRICING_MODE_MARKUP_PERCENT ? moneyOrZero(n.markup_percent) : '0.00',
      fixed_markup_amount: n.pricing_mode === RESELLER_PRICING_MODE_FIXED_MARKUP ? moneyOrZero(n.fixed_markup_amount) : '0.00',
      fixed_price_amount: n.pricing_mode === RESELLER_PRICING_MODE_FIXED_PRICE ? moneyOrZero(n.fixed_price_amount) : '0.00',
      sort_order: n.sort_order,
    }
  }),
})

/**
 * Local estimate of the reseller price for a rule (the backend preview is
 * authoritative). `inherit` uses `defaultMarkupPercent`. Returns null on bad input.
 */
export const estimateResellerPrice = (basePrice: string, form: ResellerProductSettingFormItem, defaultMarkupPercent = '0'): string | null => {
  const base = amountToCents(basePrice)
  if (base === null) return null
  const mode = form.pricing_mode || RESELLER_PRICING_MODE_INHERIT
  const percentOf = (pct: string | undefined) => {
    const bp = amountToCents(pct || '0')
    if (bp === null) return null
    return base + Math.round((base * bp) / 10000)
  }
  if (mode === RESELLER_PRICING_MODE_INHERIT) {
    const v = percentOf(defaultMarkupPercent)
    return v === null ? null : centsToAmount(v)
  }
  if (mode === RESELLER_PRICING_MODE_MARKUP_PERCENT) {
    const v = percentOf(form.markup_percent)
    return v === null ? null : centsToAmount(v)
  }
  if (mode === RESELLER_PRICING_MODE_FIXED_MARKUP) {
    const add = amountToCents(form.fixed_markup_amount || '0')
    return add === null ? null : centsToAmount(base + add)
  }
  if (mode === RESELLER_PRICING_MODE_FIXED_PRICE) {
    const fixed = amountToCents(form.fixed_price_amount || '0')
    return fixed === null ? null : centsToAmount(fixed)
  }
  return null
}

const isSkuListed = (sku: { is_active?: boolean; setting?: { is_listed?: boolean } | null }) =>
  sku.is_active !== false && sku.setting?.is_listed !== false

export const countListedSkus = (detail?: DetailLike | null): number => {
  const skus = detail?.skus || []
  if (skus.length === 0) return detail?.product_setting?.is_listed === false ? 0 : 1
  return skus.filter(isSkuListed).length
}

export const countActiveSkus = (detail?: DetailLike | null): number => {
  const skus = detail?.skus || []
  if (skus.length === 0) return 1
  return skus.filter((sku) => sku.is_active !== false).length
}

const sortMoney = (items: string[]) =>
  [...items].sort((a, b) => {
    const an = Number(a)
    const bn = Number(b)
    if (Number.isFinite(an) && Number.isFinite(bn)) return an - bn
    return a.localeCompare(b)
  })

/** `"12.00"` or `"10.00 - 18.00"` over listed SKUs, falling back to product rule price. */
export const summarizeProductEffectivePrice = (detail?: DetailLike | null, fallback = '-'): string => {
  const values = new Set<string>()
  for (const sku of (detail?.skus || []).filter(isSkuListed)) {
    const v = String(sku.effective_price_amount || sku.setting?.effective_price_amount || '').trim()
    if (v) values.add(v)
  }
  if (values.size > 0) {
    const sorted = sortMoney([...values])
    return sorted.length === 1 ? sorted[0] : `${sorted[0]} - ${sorted[sorted.length - 1]}`
  }
  return String(detail?.product_setting?.effective_price_amount || '').trim() || fallback
}

export const getResellerRuleSourceLabelKey = (source?: string): string => {
  if (source === 'sku') return 'skuRule'
  if (source === 'product') return 'productRule'
  if (source === 'profile') return 'defaultRule'
  return 'unknownRule'
}
