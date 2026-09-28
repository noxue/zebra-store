import type { AdminProduct, AdminWholesalePrice } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { wholesaleTierScopeValue } from '@/utils/wholesalePricing'

/** Deduplicated positive integer ids. */
const positiveIntSet = (values: unknown[]): number[] =>
  Array.from(
    new Set(
      values
        .map((item) => Number(item))
        .filter((item) => Number.isFinite(item) && item > 0)
        .map((item) => Math.floor(item)),
    ),
  )

/** Coupon `scope_ref_ids` arrives as an array, a JSON string ("[1,2]") or a legacy comma list. */
export function normalizeScopeIDs(raw: unknown): number[] {
  if (Array.isArray(raw)) return positiveIntSet(raw)
  if (typeof raw === 'string') {
    const text = raw.trim()
    if (!text) return []
    try {
      const parsed: unknown = JSON.parse(text)
      if (Array.isArray(parsed)) return positiveIntSet(parsed)
    } catch {
      // legacy comma separated format
    }
    return positiveIntSet(text.split(/[,，\s]+/))
  }
  return []
}

export const PAYMENT_ROLES = ['guest', 'member'] as const

export function normalizePaymentRoles(raw: unknown): string[] {
  if (!Array.isArray(raw)) return []
  const allowed = new Set<string>(PAYMENT_ROLES)
  return Array.from(new Set(raw.map((item) => String(item ?? '').trim().toLowerCase()).filter((item) => allowed.has(item))))
}

export function normalizeMemberLevels(raw: unknown): number[] {
  return Array.isArray(raw) ? positiveIntSet(raw) : []
}

/** "#12 Product name" label for product pickers. */
export function buildProductLabel(product: Pick<AdminProduct, 'id' | 'title'>): string {
  const id = Number(product?.id || 0)
  const name = getLocalizedText(product?.title || {})
  if (id > 0 && name) return `#${id} ${name}`
  if (id > 0) return `#${id}`
  return name || '-'
}

/** Prepend placeholder products for referenced ids that are missing from the loaded options. */
export function ensureProductsInOptions(rows: AdminProduct[], ids: number[]): AdminProduct[] {
  if (!ids.length) return rows
  const exists = new Set(rows.map((item) => Number(item?.id || 0)))
  const out = rows.slice()
  ids.forEach((id) => {
    if (id <= 0 || exists.has(id)) return
    out.unshift({ id, title: { 'zh-CN': `#${id}`, 'zh-TW': `#${id}`, 'en-US': `#${id}` } } as unknown as AdminProduct)
    exists.add(id)
  })
  return out
}

/** Format product ids as "#1 Name, #2" using the loaded product options. */
export function formatProductScope(ids: number[], products: AdminProduct[]): string {
  if (!ids.length) return '-'
  return ids
    .map((id) => {
      const target = products.find((item) => Number(item?.id || 0) === id)
      const name = target ? getLocalizedText(target.title || {}) : ''
      return name ? `#${id} ${name}` : `#${id}`
    })
    .join(', ')
}

// --- Gift cards -------------------------------------------------------------

export interface GiftCardRedeemedUser {
  id?: number
  display_name?: string
  email?: string
}

export function formatRedeemedUser(user: GiftCardRedeemedUser | null | undefined): string {
  if (!user) return '-'
  const id = Number(user.id || 0)
  const displayName = String(user.display_name || '').trim()
  const email = String(user.email || '').trim()
  if (displayName && email) return `#${id} ${displayName} (${email})`
  if (displayName) return id > 0 ? `#${id} ${displayName}` : displayName
  if (email) return id > 0 ? `#${id} ${email}` : email
  return id > 0 ? `#${id}` : '-'
}

export type GiftCardDisplayStatus = 'active' | 'expired' | 'redeemed' | 'disabled' | string

/** Active cards past their expiry are displayed as "expired". */
export function giftCardDisplayStatus(card: { status?: string; is_expired?: boolean }): GiftCardDisplayStatus {
  const status = String(card?.status || '').toLowerCase()
  if (status === 'active' && card?.is_expired) return 'expired'
  return status
}

export type GiftCardValidation = { ok: true; quantity: number; amount: string } | { ok: false; error: 'invalidQuantity' | 'invalidAmount' | 'nameRequired' }

export function validateGiftCardGenerate(form: { name: string; quantity: number | string; amount: string | number }): GiftCardValidation {
  const quantity = Number(form.quantity)
  if (!Number.isFinite(quantity) || quantity <= 0 || quantity > 10000) return { ok: false, error: 'invalidQuantity' }
  const amount = String(form.amount ?? '').trim()
  const parsed = Number(amount)
  if (!amount || !Number.isFinite(parsed) || parsed <= 0) return { ok: false, error: 'invalidAmount' }
  if (!String(form.name || '').trim()) return { ok: false, error: 'nameRequired' }
  return { ok: true, quantity: Math.floor(quantity), amount }
}

// --- Promotions -------------------------------------------------------------

const parsePriceNumber = (value: unknown): number | null => {
  if (value === '' || value === null || value === undefined) return null
  const parsed = Number(value)
  if (!Number.isFinite(parsed) || parsed < 0) return null
  return parsed
}

/** Lowest active SKU price of a product, falling back to the product price. */
export function referenceUnitPrice(product: AdminProduct | null | undefined): number | null {
  if (!product) return null
  const skus = Array.isArray(product.skus) ? product.skus : []
  const prices = skus.filter((sku) => sku?.is_active !== false).map((sku) => parsePriceNumber(sku?.price_amount)).filter((p): p is number => p !== null)
  if (prices.length) return Math.min(...prices)
  return parsePriceNumber(product.price_amount)
}

/** A fixed discount that is >= the reference price may reduce the price to zero. */
export function fixedDiscountRisk(type: string, value: unknown, referencePrice: number | null): { discountValue: string; referencePrice: string } | null {
  if (String(type || '').trim() !== 'fixed') return null
  const discount = parsePriceNumber(value)
  if (discount === null || referencePrice === null) return null
  if (discount < referencePrice) return null
  return { discountValue: discount.toFixed(2), referencePrice: referencePrice.toFixed(2) }
}

// --- Wholesale prices -------------------------------------------------------

export interface WholesaleTierFormItem {
  sku_id: string
  sku_code: string
  min_quantity: number | string
  unit_price: number | string
}

/** Payload tier sent to the backend (numbers on the wire — the API rejects string amounts here). */
export interface WholesaleTierPayload {
  sku_id?: number
  sku_code?: string
  min_quantity: number
  unit_price: number
}

export const sortWholesaleTiers = <T extends { sku_id?: number | string; sku_code?: string; min_quantity?: number | string }>(tiers: T[]): T[] =>
  tiers.slice().sort((a, b) => {
    const scopeA = wholesaleTierScopeValue(a)
    const scopeB = wholesaleTierScopeValue(b)
    if (scopeA !== scopeB) return scopeA.localeCompare(scopeB)
    return Number(a.min_quantity || 0) - Number(b.min_quantity || 0)
  })

export const sortedProductTiers = (product: AdminProduct): AdminWholesalePrice[] =>
  sortWholesaleTiers(Array.isArray(product.wholesale_prices) ? product.wholesale_prices : [])

export type WholesaleTierError = { kind: 'invalidTier'; index: number } | { kind: 'duplicateQuantity'; skuId: string; quantity: number }

/**
 * Validate + normalise the tier form. `skuCodeById` resolves the code of an active SKU id.
 * Returns the sorted payload or the first error.
 */
export function normalizeWholesaleTiers(
  tiers: WholesaleTierFormItem[],
  skuCodeById: (id: number) => string,
): { ok: true; tiers: WholesaleTierPayload[] } | { ok: false; error: WholesaleTierError } {
  const seen = new Set<string>()
  const out: WholesaleTierPayload[] = []
  for (let index = 0; index < tiers.length; index++) {
    const item = tiers[index]!
    const minQuantity = Math.floor(Number(item.min_quantity))
    const unitPrice = Number(item.unit_price)
    if (item.min_quantity === '' || item.unit_price === '' || !Number.isFinite(minQuantity) || minQuantity <= 0 || !Number.isFinite(unitPrice) || unitPrice <= 0) {
      return { ok: false, error: { kind: 'invalidTier', index: index + 1 } }
    }
    const scope = wholesaleTierScopeValue({ sku_id: item.sku_id })
    let skuId = 0
    let skuCode = ''
    if (scope.startsWith('id:')) {
      skuId = Math.floor(Number(scope.slice(3)))
      skuCode = skuCodeById(skuId).trim()
    } else if (scope.startsWith('code:')) {
      skuCode = scope.slice(5).trim()
    }
    const key = `${scope}:${minQuantity}`
    if (seen.has(key)) return { ok: false, error: { kind: 'duplicateQuantity', skuId: item.sku_id, quantity: minQuantity } }
    seen.add(key)
    out.push({ sku_id: skuId || undefined, sku_code: skuCode || undefined, min_quantity: minQuantity, unit_price: unitPrice })
  }
  return { ok: true, tiers: sortWholesaleTiers(out) }
}
