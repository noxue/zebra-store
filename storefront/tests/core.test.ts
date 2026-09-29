import { beforeEach, describe, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { encodeGuestAuthorization } from '@/api/order'
import { buildUrl } from '@/api/client'
import { mergeMessages } from '@/i18n'
import { clampCartQuantity, useCartStore, type CartItem } from '@/stores/cart'
import { cartItemAvailableStock, cartItemPurchaseLimit, cartItemPurchaseMin } from '@/utils/cartStock'
import {
  clearGuestOrderAuth,
  createGeneratedGuestOrderAuth,
  ensureGuestOrderAuth,
  isGeneratedGuestOrderAuth,
  loadGuestOrderAuth,
  loadGuestOrderDraft,
  saveGuestOrderAuth,
  saveGuestOrderDraft,
} from '@/utils/guestOrderAuth'
import { localizedText } from '@/utils/localized'
import { addAmounts, amountToCents, centsToAmount, formatMoney, multiplyAmount } from '@/utils/money'
import {
  getWholesalePrices,
  hasPromotionPrice,
  promotionSaveAmount,
  resolveMemberPriceAmount,
  resolveWholesalePriceAmount,
} from '@/utils/productPricing'
import { resolveSkuStockDisplay } from '@/utils/publicStock'
import { buildSkuDisplayTextFromSnapshot } from '@/utils/sku'
import { formatCountdown } from '@/utils/format'

describe('money', () => {
  it('parses decimal strings to cents with half-up rounding', () => {
    expect(amountToCents('12.30')).toBe(1230)
    expect(amountToCents('0.005')).toBe(1)
    expect(amountToCents('-1.2')).toBe(-120)
    expect(amountToCents('abc')).toBeNull()
    expect(amountToCents('')).toBeNull()
  })
  it('formats cents back to two decimals', () => {
    expect(centsToAmount(7840)).toBe('78.40')
    expect(centsToAmount(-5)).toBe('-0.05')
  })
  it('formats prices with currency like the original storefront', () => {
    expect(formatMoney('78.4', 'CNY')).toBe('78.40 CNY')
    expect(formatMoney(98, 'USD')).toBe('98.00 USD')
    expect(formatMoney('', 'CNY')).toBe('-')
    expect(formatMoney(null)).toBe('-')
    expect(formatMoney('12.5', null)).toBe('12.50')
    expect(formatMoney('n/a', 'CNY')).toBe('n/a CNY')
  })
  it('adds and multiplies exactly', () => {
    expect(addAmounts('0.10', '0.20')).toBe('0.30')
    expect(multiplyAmount('78.40', 3)).toBe('235.20')
  })
})

describe('localizedText', () => {
  it('resolves locale then falls back zh-CN → zh-TW → en-US', () => {
    const v = { 'zh-CN': '中文', 'en-US': 'English' }
    expect(localizedText(v, 'en-US')).toBe('English')
    expect(localizedText(v, 'zh-TW')).toBe('中文')
    expect(localizedText({ 'en-US': 'Only' }, 'zh-CN')).toBe('Only')
    expect(localizedText({ 'zh-CN': '  ' }, 'zh-CN')).toBe('')
    expect(localizedText(null, 'zh-CN')).toBe('')
    expect(localizedText('plain', 'zh-CN')).toBe('plain')
  })
})

describe('guest authorization header', () => {
  it('encodes lower(email)\\npassword as base64url without padding', () => {
    // base64("g@test.com\npass1234") = "Z0B0ZXN0LmNvbQpwYXNzMTIzNA=="
    expect(encodeGuestAuthorization('  G@Test.com ', 'pass1234')).toBe('Guest Z0B0ZXN0LmNvbQpwYXNzMTIzNA')
  })
  it('uses url-safe alphabet for utf-8 input', () => {
    const header = encodeGuestAuthorization('用户@例子.com', '密码?>')
    expect(header.startsWith('Guest ')).toBe(true)
    expect(header).not.toMatch(/[+/=]/)
  })
})

describe('guest order auth storage', () => {
  beforeEach(() => {
    clearGuestOrderAuth()
    sessionStorage.clear()
    localStorage.clear()
  })
  it('persists credentials in localStorage across browser sessions', () => {
    saveGuestOrderAuth({ email: 'a@b.c', order_password: 'x' })
    expect(JSON.parse(localStorage.getItem('guest_order_auth') || '{}')).toEqual({ email: 'a@b.c', order_password: 'x' })
    expect(sessionStorage.getItem('guest_order_auth')).toBeNull()
  })
  it('creates and reuses a uuid browser identity', () => {
    const generated = ensureGuestOrderAuth()
    expect(isGeneratedGuestOrderAuth(generated)).toBe(true)
    expect(loadGuestOrderAuth()).toEqual(generated)
    expect(ensureGuestOrderAuth()).toEqual(generated)
  })
  it('recognizes only matching uuid email and password pairs', () => {
    const generated = createGeneratedGuestOrderAuth()
    expect(isGeneratedGuestOrderAuth(generated)).toBe(true)
    expect(isGeneratedGuestOrderAuth({ ...generated, order_password: 'different' })).toBe(false)
  })
  it('persists an incomplete checkout draft for refresh recovery', () => {
    saveGuestOrderDraft({ email: 'buyer@example.com', order_password: '' })
    expect(loadGuestOrderDraft()).toEqual({ email: 'buyer@example.com', order_password: '' })
    expect(JSON.parse(localStorage.getItem('guest_order_auth_draft') || '{}')).toEqual({ email: 'buyer@example.com', order_password: '' })
  })
})

const cartItem = (over: Partial<CartItem> = {}): CartItem => ({
  productId: 1,
  skuId: 1,
  slug: 'steam-100',
  title: { 'zh-CN': 'Steam' },
  priceAmount: '98.00',
  quantity: 1,
  fulfillmentType: 'auto',
  ...over,
})

describe('cart store', () => {
  beforeEach(() => {
    localStorage.clear()
    setActivePinia(createPinia())
  })
  it('merges identical product+sku lines and persists to cart_items', () => {
    const cart = useCartStore()
    cart.addItem(cartItem(), 2)
    cart.addItem(cartItem(), 3)
    expect(cart.items).toHaveLength(1)
    expect(cart.totalItems).toBe(5)
    const stored = JSON.parse(localStorage.getItem('cart_items') || '[]') as CartItem[]
    expect(stored[0].quantity).toBe(5)
  })
  it('keeps different skus separate and clamps to max purchase quantity', () => {
    const cart = useCartStore()
    cart.addItem(cartItem({ maxPurchaseQuantity: 10 }), 8)
    cart.addItem(cartItem({ maxPurchaseQuantity: 10 }), 8)
    cart.addItem(cartItem({ skuId: 2 }), 1)
    expect(cart.items.map((i) => i.quantity)).toEqual([10, 1])
  })
  it('updates, removes and restores lines', () => {
    const cart = useCartStore()
    cart.addItem(cartItem({ minPurchaseQuantity: 2 }), 2)
    cart.updateQuantity(1, 0, 1)
    expect(cart.items[0].quantity).toBe(2)
    const removed = cart.items[0]
    cart.removeItem(1, 1)
    expect(cart.items).toHaveLength(0)
    cart.restoreItem(removed, 0)
    expect(cart.items).toHaveLength(1)
  })
  it('drops invalid persisted rows when loading', () => {
    localStorage.setItem('cart_items', JSON.stringify([{ productId: 0 }, cartItem({ skuId: -3 })]))
    setActivePinia(createPinia())
    const cart = useCartStore()
    expect(cart.items).toHaveLength(1)
    expect(cart.items[0].skuId).toBe(0)
  })
  it('clamps quantity into [min, max]', () => {
    expect(clampCartQuantity(0)).toBe(1)
    expect(clampCartQuantity(5, 3)).toBe(3)
    expect(clampCartQuantity(1, 0, 4)).toBe(4)
  })
  it('derives purchase limits and available stock from snapshots', () => {
    expect(cartItemPurchaseLimit({ maxPurchaseQuantity: 0 })).toBeNull()
    expect(cartItemPurchaseMin({ minPurchaseQuantity: undefined })).toBe(1)
    expect(cartItemAvailableStock(cartItem({ skuStockEnforced: true, skuAutoStockAvailable: 3 }))).toBe(3)
    expect(cartItemAvailableStock(cartItem({ skuStockEnforced: false }))).toBeNull()
    expect(cartItemAvailableStock(cartItem({ skuStockEnforced: false, skuStockStatus: 'out_of_stock' }))).toBe(0)
  })
})

describe('product pricing', () => {
  it('detects promotion price and savings', () => {
    expect(hasPromotionPrice({ price_amount: '98.00', promotion_price_amount: '78.40' })).toBe(true)
    expect(hasPromotionPrice({ price_amount: '98.00', promotion_price_amount: '98.00' })).toBe(false)
    expect(promotionSaveAmount({ price_amount: '98.00', promotion_price_amount: '78.40' })).toBe('19.60')
  })
  it('prefers sku specific wholesale tiers over universal ones', () => {
    const product = {
      wholesale_prices: [
        { min_quantity: 5, unit_price: '90.00' },
        { sku_id: 2, min_quantity: 3, unit_price: '80.00' },
      ],
    }
    expect(getWholesalePrices(product, 2)).toEqual([{ sku_id: 2, min_quantity: 3, unit_price: '80.00' }])
    expect(getWholesalePrices(product, 1)).toEqual([{ min_quantity: 5, unit_price: '90.00' }])
    expect(resolveWholesalePriceAmount(product, '98.00', 5, 1)).toBe('90.00')
    expect(resolveWholesalePriceAmount(product, '98.00', 4, 1)).toBeNull()
    expect(resolveWholesalePriceAmount(product, '98.00', 3, 2, undefined, 3)).toBe('80.00')
  })
  it('resolves member price from override or level discount', () => {
    const product = { member_prices: [{ member_level_id: 1, sku_id: 0, price_amount: '20.00' }] }
    expect(resolveMemberPriceAmount(product, 5, '25.00', 1)).toBe('20.00')
    expect(resolveMemberPriceAmount({ member_prices: [] }, 5, '100.00', 2, 95)).toBe('95.00')
    expect(resolveMemberPriceAmount({ member_prices: [] }, 5, '100.00', 0, 95)).toBeNull()
    expect(resolveMemberPriceAmount({ member_prices: [] }, 5, '100.00', 2, 100)).toBeNull()
  })
})

describe('public stock display', () => {
  it('shows remaining for exact auto stock and out when sold out', () => {
    expect(resolveSkuStockDisplay({ fulfillment_type: 'auto', stock_display_mode: 'exact' }, { auto_stock_available: 3, stock_status: 'low_stock' })).toEqual({ kind: 'remaining', count: 3 })
    expect(resolveSkuStockDisplay({ fulfillment_type: 'auto' }, { auto_stock_available: 0, stock_status: 'out_of_stock' })).toEqual({ kind: 'out' })
  })
  it('uses ranges when quantity is hidden', () => {
    expect(
      resolveSkuStockDisplay({ fulfillment_type: 'manual', stock_display_mode: 'range' }, { stock_status: 'in_stock', stock_display: 'range_10_50', stock_range_min: 10, stock_range_max: 50 }),
    ).toEqual({ kind: 'range', min: 10, max: 50 })
  })
})

describe('misc helpers', () => {
  it('builds sku text from order snapshots', () => {
    expect(buildSkuDisplayTextFromSnapshot({ sku_code: 'basic', spec_values: { 'zh-CN': '基础版' } }, { locale: 'zh-CN' })).toBe('基础版')
    expect(buildSkuDisplayTextFromSnapshot({ sku_code: 'X', spec_values: { color: 'red' } })).toBe('color:red')
    expect(buildSkuDisplayTextFromSnapshot({ sku_code: 'DEFAULT', spec_values: {} }, { fallback: '-' })).toBe('-')
  })
  it('builds query strings skipping empty values', () => {
    expect(buildUrl('/api/v1', '/orders', { page: 1, status: '', order_no: undefined })).toBe('/api/v1/orders?page=1')
  })
  it('formats countdowns', () => {
    expect(formatCountdown(65)).toBe('01:05')
    expect(formatCountdown(3661)).toBe('01:01:01')
    expect(formatCountdown(-3)).toBe('00:00')
  })
  it('deep merges i18n additions', () => {
    expect(mergeMessages({ a: { b: 'x' } }, { a: { c: 'y' }, d: 'z' })).toEqual({ a: { b: 'x', c: 'y' }, d: 'z' })
  })
})
