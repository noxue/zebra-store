import type { MemberPrice, Product, ProductSku, WholesalePrice } from '@/api/types'
import { amountToCents, centsToAmount } from './money'

type WithWholesale = { wholesale_prices?: WholesalePrice[] | null } | null | undefined
type WithMemberPrices = { member_prices?: MemberPrice[] | null } | null | undefined
type PriceLike = { price_amount?: string; promotion_price_amount?: string } | null | undefined

const normalizeText = (value: unknown) => String(value ?? '').trim()
const normalizeSkuId = (value: unknown) => {
  const id = Math.trunc(Number(value || 0))
  return Number.isFinite(id) && id > 0 ? id : 0
}
const normalizeSkuCode = (value: unknown) => normalizeText(value).toLowerCase()

/** True when promotion_price_amount is a real discount over price_amount. */
export const hasPromotionPrice = (item: PriceLike): boolean => {
  if (!item) return false
  const original = amountToCents(item.price_amount)
  const promotion = amountToCents(item.promotion_price_amount)
  if (original === null || promotion === null) return false
  return promotion >= 0 && promotion < original
}

export const promotionSaveAmount = (item: PriceLike): string => {
  const original = amountToCents(item?.price_amount)
  const promotion = amountToCents(item?.promotion_price_amount)
  if (original === null || promotion === null || promotion >= original) return '0.00'
  return centsToAmount(original - promotion)
}

export const isUniversalWholesaleTier = (tier: WholesalePrice) =>
  normalizeSkuId(tier.sku_id) <= 0 && normalizeText(tier.sku_code) === ''

const tierMatchesSku = (tier: WholesalePrice, skuId?: number, skuCode?: string) => {
  if (isUniversalWholesaleTier(tier)) return true
  const tierSkuId = normalizeSkuId(tier.sku_id)
  const current = normalizeSkuId(skuId)
  if (tierSkuId > 0 && current > 0 && tierSkuId === current) return true
  const tierCode = normalizeSkuCode(tier.sku_code)
  return Boolean(tierCode && tierCode === normalizeSkuCode(skuCode))
}

const allWholesalePrices = (product: WithWholesale): WholesalePrice[] =>
  Array.isArray(product?.wholesale_prices) ? product.wholesale_prices : []

/** Tiers applicable to a SKU: SKU-specific tiers win over universal ones. */
export const getWholesalePrices = (product: WithWholesale, skuId?: number, skuCode?: string): WholesalePrice[] => {
  const tiers = allWholesalePrices(product)
  const hasSku = normalizeSkuId(skuId) > 0 || normalizeText(skuCode) !== ''
  if (!hasSku) return tiers
  const hasSpecific = tiers.some((tier) => !isUniversalWholesaleTier(tier) && tierMatchesSku(tier, skuId, skuCode))
  return tiers.filter((tier) =>
    hasSpecific ? !isUniversalWholesaleTier(tier) && tierMatchesSku(tier, skuId, skuCode) : isUniversalWholesaleTier(tier),
  )
}

export const hasWholesalePrices = (product: WithWholesale) => allWholesalePrices(product).length > 0

/**
 * Lowest wholesale unit price reached by `matchQuantity` (universal tiers)
 * or `lineQuantity` (SKU tiers); `null` when no tier beats `basePrice`.
 */
export const resolveWholesalePriceAmount = (
  product: WithWholesale,
  basePrice: string | undefined,
  matchQuantity: number,
  skuId?: number,
  skuCode?: string,
  lineQuantity?: number,
): string | null => {
  const base = amountToCents(basePrice)
  if (base === null || !Number.isFinite(matchQuantity) || matchQuantity <= 0) return null
  const hasSku = normalizeSkuId(skuId) > 0 || normalizeText(skuCode) !== ''
  const tiers = hasSku ? getWholesalePrices(product, skuId, skuCode) : allWholesalePrices(product).filter(isUniversalWholesaleTier)
  let matched: number | null = null
  for (const tier of tiers) {
    const minQuantity = Number(tier.min_quantity || 0)
    const cents = amountToCents(tier.unit_price)
    if (!Number.isFinite(minQuantity) || minQuantity <= 0 || cents === null) continue
    const qty = isUniversalWholesaleTier(tier) ? matchQuantity : Number(lineQuantity ?? matchQuantity)
    if (!Number.isFinite(qty) || qty < minQuantity) continue
    if (matched === null || cents < matched) matched = cents
  }
  if (matched === null || matched >= base) return null
  return centsToAmount(matched)
}

/** Member price: explicit override for sku/product, else level discount_rate (percent of price). */
export const resolveMemberPriceAmount = (
  product: WithMemberPrices,
  skuId: number | undefined,
  basePrice: string | undefined,
  memberLevelId: number | undefined,
  discountRate?: number,
): string | null => {
  const base = amountToCents(basePrice)
  const levelId = Number(memberLevelId || 0)
  if (base === null || base <= 0 || !Number.isFinite(levelId) || levelId <= 0) return null
  const prices = Array.isArray(product?.member_prices) ? product.member_prices : []
  const sid = Number(skuId || 0)
  const skuPrice = prices.find((p) => Number(p.member_level_id || 0) === levelId && Number(p.sku_id || 0) === sid)
  const productPrice = prices.find((p) => Number(p.member_level_id || 0) === levelId && Number(p.sku_id || 0) === 0)
  const override = amountToCents(skuPrice?.price_amount ?? productPrice?.price_amount)
  if (override !== null && override > 0) return override < base ? centsToAmount(override) : null
  const rate = Number(discountRate || 0)
  if (!Number.isFinite(rate) || rate <= 0 || rate >= 100) return null
  const member = Math.round((base * rate) / 100)
  return member < base ? centsToAmount(member) : null
}

export const isProductSoldOut = (product: Pick<Product, 'is_sold_out' | 'stock_status'> | null | undefined) =>
  Boolean(product?.is_sold_out || product?.stock_status === 'out_of_stock')

export const activeSkus = (product: Pick<Product, 'skus'> | null | undefined): ProductSku[] =>
  (Array.isArray(product?.skus) ? product.skus : []).filter((sku) => Boolean(sku?.is_active))

/** Display price for a product card: lowest active SKU price (promotion aware). */
export const productDisplayPrice = (product: Product): { price: string; original: string | null } => {
  if (hasPromotionPrice(product)) {
    return { price: product.promotion_price_amount as string, original: product.price_amount }
  }
  return { price: product.price_amount, original: null }
}
