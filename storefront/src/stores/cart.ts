import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import type { LocalizedText, ManualFormSchema, WholesalePrice } from '@/api/types'

export interface CartItem {
  productId: number
  skuId?: number
  skuCode?: string
  skuSpecValues?: Record<string, unknown> | null
  skuManualStockTotal?: number
  skuManualStockLocked?: number
  skuManualStockSold?: number
  skuAutoStockAvailable?: number
  skuUpstreamStock?: number
  skuStockStatus?: string
  skuStockDisplayMode?: string
  skuStockDisplay?: string
  skuStockRangeMin?: number
  skuStockRangeMax?: number
  skuStockQuantityHidden?: boolean
  skuStockEnforced?: boolean
  skuStockSnapshotAt?: string
  slug: string
  title: LocalizedText
  priceAmount: string
  wholesalePrices?: WholesalePrice[]
  image?: string
  quantity: number
  minPurchaseQuantity?: number
  maxPurchaseQuantity?: number
  purchaseType?: string
  fulfillmentType?: string
  manualFormSchema?: ManualFormSchema | null
  paymentChannelIds?: number[]
}

const STORAGE_KEY = 'cart_items'

export const normalizeSkuId = (value: unknown): number => {
  const n = Number(value)
  if (!Number.isFinite(n)) return 0
  const i = Math.trunc(n)
  return i > 0 ? i : 0
}

export const cartIdentity = (item: Pick<CartItem, 'productId' | 'skuId'>) => `${item.productId}:${normalizeSkuId(item.skuId)}`

const optStock = (value: unknown, allowUnlimited = false): number | undefined => {
  if (value === undefined || value === null || value === '') return undefined
  const n = Number(value)
  if (!Number.isFinite(n)) return undefined
  const i = Math.floor(n)
  if (allowUnlimited && i === -1) return -1
  return Math.max(i, 0)
}
const optBool = (value: unknown): boolean | undefined =>
  value === undefined || value === null || value === '' ? undefined : Boolean(value)
const optString = (value: unknown): string | undefined => {
  if (value === undefined || value === null) return undefined
  const text = String(value).trim()
  return text || undefined
}
const optLimit = (value: unknown): number | undefined => {
  if (value === undefined || value === null || value === '') return undefined
  const n = Number(value)
  if (!Number.isFinite(n)) return undefined
  const i = Math.floor(n)
  return i > 0 ? i : undefined
}

/** Clamp quantity into [min(≥1), max] (max ≤ 0 = unlimited). */
export const clampCartQuantity = (quantity: number, max?: number, min?: number): number => {
  const lower = min && min > 0 ? min : 1
  const normalized = Math.max(lower, Math.floor(Number(quantity) || lower))
  if (!max || max <= 0) return normalized
  return Math.min(normalized, max)
}

/** Normalises every optional numeric/boolean/string field of a cart line. */
export const normalizeCartItem = (raw: CartItem): CartItem => ({
  ...raw,
  productId: Math.trunc(Number(raw.productId)),
  skuId: normalizeSkuId(raw.skuId),
  skuManualStockTotal: optStock(raw.skuManualStockTotal, true),
  skuManualStockLocked: optStock(raw.skuManualStockLocked),
  skuManualStockSold: optStock(raw.skuManualStockSold),
  skuAutoStockAvailable: optStock(raw.skuAutoStockAvailable),
  skuUpstreamStock: optStock(raw.skuUpstreamStock, true),
  skuStockStatus: optString(raw.skuStockStatus),
  skuStockDisplayMode: optString(raw.skuStockDisplayMode),
  skuStockDisplay: optString(raw.skuStockDisplay),
  skuStockRangeMin: optStock(raw.skuStockRangeMin),
  skuStockRangeMax: optStock(raw.skuStockRangeMax),
  skuStockQuantityHidden: optBool(raw.skuStockQuantityHidden),
  skuStockEnforced: optBool(raw.skuStockEnforced),
  skuStockSnapshotAt: optString(raw.skuStockSnapshotAt),
  minPurchaseQuantity: optLimit(raw.minPurchaseQuantity),
  maxPurchaseQuantity: optLimit(raw.maxPurchaseQuantity),
})

export const loadCartItems = (): CartItem[] => {
  let raw: string | null
  try {
    raw = localStorage.getItem(STORAGE_KEY)
  } catch {
    return []
  }
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter((row): row is CartItem => {
        if (!row || typeof row !== 'object') return false
        const id = Number((row as { productId?: unknown }).productId)
        return Number.isFinite(id) && id > 0
      })
      .map((row) => normalizeCartItem(row))
  } catch {
    return []
  }
}

export const useCartStore = defineStore('cart', () => {
  const items = ref<CartItem[]>(loadCartItems())
  const totalItems = computed(() => items.value.reduce((sum, item) => sum + item.quantity, 0))

  const persist = () => {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(items.value))
    } catch {
      // storage unavailable
    }
  }

  const find = (productId: number, skuId?: number) =>
    items.value.find((entry) => cartIdentity(entry) === `${Math.trunc(Number(productId))}:${normalizeSkuId(skuId)}`)

  const addItem = (item: CartItem, quantity = 1) => {
    const normalized = normalizeCartItem({
      ...item,
      skuStockSnapshotAt: optString(item.skuStockSnapshotAt) || new Date().toISOString(),
    })
    const qty = clampCartQuantity(quantity, normalized.maxPurchaseQuantity, normalized.minPurchaseQuantity)
    const existing = find(normalized.productId, normalized.skuId)
    if (existing) {
      const nextQty = clampCartQuantity(existing.quantity + qty, normalized.maxPurchaseQuantity, normalized.minPurchaseQuantity)
      Object.assign(existing, { ...normalized, quantity: nextQty })
    } else {
      items.value.push({ ...normalized, quantity: qty })
    }
    persist()
  }

  const updateQuantity = (productId: number, quantity: number, skuId?: number) => {
    const target = find(productId, skuId)
    if (!target) return
    target.quantity = clampCartQuantity(quantity, target.maxPurchaseQuantity, target.minPurchaseQuantity)
    persist()
  }

  const patchItem = (productId: number, skuId: number | undefined, patch: Partial<CartItem>) => {
    const target = find(productId, skuId)
    if (!target) return
    Object.assign(target, normalizeCartItem({ ...target, ...patch }))
    persist()
  }

  const removeItem = (productId: number, skuId?: number) => {
    const identity = `${Math.trunc(Number(productId))}:${normalizeSkuId(skuId)}`
    items.value = items.value.filter((entry) => cartIdentity(entry) !== identity)
    persist()
  }

  /** Re-insert a removed line at its old index (undo). */
  const restoreItem = (item: CartItem, index: number) => {
    if (find(item.productId, item.skuId)) return
    const next = [...items.value]
    next.splice(Math.min(Math.max(index, 0), next.length), 0, item)
    items.value = next
    persist()
  }

  const clear = () => {
    items.value = []
    persist()
  }

  return { items, totalItems, addItem, updateQuantity, patchItem, removeItem, restoreItem, clear }
})
