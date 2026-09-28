import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, h, nextTick } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { configAPI, productAPI } from '@/api/catalog'
import { guestOrderAPI } from '@/api/order'
import type { Product, ProductSku } from '@/api/types'
import { useCart, cartLineAvailableStock } from '@/composables/useCart'
import { useCheckout } from '@/composables/useCheckout'
import { useGuestOrderDetail } from '@/composables/useGuestOrderDetail'
import { usePayment } from '@/composables/usePayment'
import i18n, { detectLocale, setI18nLocale } from '@/i18n'
import { useAppStore } from '@/stores/app'
import { useCartStore, type CartItem } from '@/stores/cart'
import { checkoutItemAvailableStock, checkoutItemStockExceeded } from '@/utils/checkoutStock'
import { clearGuestOrderAuth, loadGuestOrderAuth } from '@/utils/guestOrderAuth'
import { localizedText } from '@/utils/localized'

const route = vi.hoisted(() => ({ query: {} as Record<string, string>, params: { order_no: 'DJ1' }, fullPath: '/checkout' }))
vi.mock('vue-router', async (original) => ({ ...await original<object>(), useRoute: () => route, useRouter: () => ({ push: vi.fn(), replace: vi.fn() }) }))
const wrappers: VueWrapper[] = []
function harness<T>(use: () => T): T {
  let result!: T
  wrappers.push(mount(defineComponent({ setup() { result = use(); return () => h('div') } }), { global: { plugins: [i18n] } }))
  return result
}
const line = (extra: Partial<CartItem> = {}): CartItem => ({ productId: 7, skuId: 1, slug: 'p', title: { 'zh-CN': '商品' }, priceAmount: '10.00', quantity: 5, fulfillmentType: 'auto', ...extra })
const sku = (extra: Partial<ProductSku> = {}): ProductSku => ({ id: 1, sku_code: 'A', price_amount: '12.00', is_active: true, auto_stock_available: 3, ...extra })
const product = (extra: Partial<Product> = {}): Product => ({ id: 7, slug: 'p', title: {}, price_amount: '12.00', purchase_type: 'guest', fulfillment_type: 'auto', skus: [sku()], ...extra })
const response = <T>(data: T) => ({ status_code: 0, msg: 'ok', data })

beforeEach(async () => {
  localStorage.clear(); sessionStorage.clear(); clearGuestOrderAuth()
  setActivePinia(createPinia())
  route.query = {}; route.fullPath = '/checkout'
  useAppStore().config = {}
  await setI18nLocale('zh-CN')
})
afterEach(() => {
  wrappers.splice(0).forEach((w) => w.unmount())
  vi.restoreAllMocks(); vi.useRealTimers(); clearGuestOrderAuth()
})

describe('FE-02 / FE-19 cart refresh and checkout', () => {
  it('refreshes the persisted price when opening the cart', async () => {
    const cart = useCartStore(); cart.addItem(line(), 5)
    vi.spyOn(productAPI, 'detail').mockResolvedValue(response(product()))
    const page = harness(useCart)
    await flushPromises()
    expect(cart.items[0]?.priceAmount).toBe('12.00')
    expect(page.totalAmount.value).toBe('60.00')
    await nextTick()
    expect(JSON.parse(localStorage.getItem('cart_items') || '[]')[0].priceAmount).toBe('12.00')
  })
  it('disables checkout and displays the stock warning after stock drops from 5 to 3', async () => {
    useCartStore().addItem(line({ skuAutoStockAvailable: 5 }), 5)
    vi.spyOn(productAPI, 'detail').mockResolvedValue(response(product()))
    const preview = vi.spyOn(guestOrderAPI, 'preview')
    const page = harness(useCheckout)
    await flushPromises()
    expect(page.canSubmit.value).toBe(false)
    expect(page.itemStockExceeded(useCartStore().items[0]!)).toBe(true)
    expect(page.itemStockHint(useCartStore().items[0]!)).toContain('3')
    expect(preview).not.toHaveBeenCalled()
  })
  it.each(['auto', 'upstream', 'manual'])('keeps unknown %s stock unlimited but respects explicit zero', (fulfillmentType) => {
    const unknown = line({ fulfillmentType })
    expect(checkoutItemAvailableStock(unknown)).toBeNull()
    expect(cartLineAvailableStock(unknown)).toBeNull()
    expect(checkoutItemStockExceeded(unknown)).toBe(false)
    expect(checkoutItemAvailableStock({ ...unknown, skuStockStatus: 'out_of_stock' })).toBe(0)
    const soldOut = { ...unknown, skuAutoStockAvailable: 0, skuUpstreamStock: 0, skuManualStockTotal: 0, skuStockSnapshotAt: '2026-09-27', skuStockEnforced: true }
    expect(checkoutItemAvailableStock(soldOut)).toBe(0)
    expect(checkoutItemStockExceeded(soldOut)).toBe(true)
    expect(checkoutItemStockExceeded({ ...unknown, maxPurchaseQuantity: 2 })).toBe(true)
  })
})

it('FE-14 allows guest detail authentication when both storage getters throw', async () => {
  vi.spyOn(window, 'sessionStorage', 'get').mockImplementation(() => { throw new DOMException('Denied', 'SecurityError') })
  vi.spyOn(window, 'localStorage', 'get').mockImplementation(() => { throw new DOMException('Denied', 'SecurityError') })
  const detail = vi.spyOn(guestOrderAPI, 'detail').mockResolvedValue(response({ order_no: 'DJ1', status: 'completed' } as Awaited<ReturnType<typeof guestOrderAPI.detail>>['data']))
  const page = harness(useGuestOrderDetail)
  await flushPromises()
  expect(page.viewState.value).toBe('auth')
  page.auth.value = { email: 'guest@example.com', order_password: 'secret123' }
  await page.handleAuthSubmit()
  expect(detail).toHaveBeenCalledWith('DJ1', page.auth.value)
  expect(page.viewState.value).toBe('detail')
  expect(loadGuestOrderAuth()).toEqual(page.auth.value)
})

it('FE-10 corrects a clock ten minutes ahead so a payment still has five minutes left', async () => {
  vi.useFakeTimers()
  const serverNow = Date.parse('2026-09-27T00:00:00Z')
  vi.setSystemTime(serverNow + 600_000)
  vi.spyOn(configAPI, 'get').mockResolvedValue(response({ server_time: serverNow }))
  await useAppStore().loadConfig(true)
  const page = harness(usePayment)
  page.order.value = { status: 'pending_payment', expires_at: new Date(serverNow + 300_000).toISOString() } as NonNullable<typeof page.order.value>
  await nextTick()
  expect(page.countdownExpired.value).toBe(false)
  expect(page.countdownText.value).toBe('05:00')
})

describe('FE-16 / FE-25 locale detection and catalog text', () => {
  it.each([['en-GB', 'en-US'], ['zh-HK', 'zh-TW'], ['zh-Hant', 'zh-TW'], ['zh-SG', 'zh-CN'], ['fr-FR', 'zh-CN']])('maps browser %s to %s and uses the same catalog locale', async (browser, expected) => {
    vi.spyOn(navigator, 'language', 'get').mockReturnValue(browser)
    const locale = detectLocale()
    expect(locale).toBe(expected)
    await setI18nLocale(locale)
    expect(i18n.global.locale.value).toBe(expected)
    const texts: Record<string, string> = { 'zh-CN': '简体', 'zh-TW': '繁體', 'en-US': 'English' }
    expect(localizedText(texts, locale)).toBe(texts[expected])
  })
  it('prefers the saved language to the browser language', () => {
    localStorage.setItem('locale', 'zh-TW')
    vi.spyOn(navigator, 'language', 'get').mockReturnValue('en-US')
    expect(detectLocale()).toBe('zh-TW')
  })
})

it('FE-02 clears removed wholesale tiers and preserves the price for invalid or zero prices', async () => {
  const { refreshCartStockSnapshots } = await import('@/utils/cartStock')
  const cart = useCartStore()
  cart.addItem(line({ wholesalePrices: [{ min_quantity: 3, unit_price: '8.00' }] }), 5)
  vi.spyOn(productAPI, 'detail').mockResolvedValue(response(product({ skus: [sku({ price_amount: '0' })] })))
  await refreshCartStockSnapshots(cart)
  expect(cart.items[0]?.priceAmount).toBe('10.00')
  expect(cart.items[0]?.wholesalePrices).toBeUndefined()
})

it('FE-04 preserves withdrawal success and clears the form when every subsequent refresh fails', async () => {
  const { resellerAPI } = await import('@/api/reseller')
  const { useResellerWithdraws } = await import('@/composables/reseller/useResellerWithdraws')
  const { useConfirmDialog } = await import('@/composables/useConfirmDialog')
  const apply = vi.spyOn(resellerAPI, 'applyWithdraw').mockResolvedValue(response({} as Awaited<ReturnType<typeof resellerAPI.applyWithdraw>>['data']))
  vi.spyOn(resellerAPI, 'dashboard').mockRejectedValue(new Error('offline'))
  vi.spyOn(resellerAPI, 'balanceAccounts').mockRejectedValue(new Error('offline'))
  vi.spyOn(resellerAPI, 'withdraws').mockRejectedValue(new Error('offline'))
  const page = harness(useResellerWithdraws)
  Object.assign(page.form, { amount: '1.00', currency: 'CNY', channel: 'alipay', account: 'test' })
  const submitting = page.submit()
  useConfirmDialog().handleConfirm()
  await submitting
  expect(apply).toHaveBeenCalledOnce()
  expect(page.alert.value?.tone).toBe('success')
  expect(page.form.amount).toBe('')
  expect(page.submittingWithdraw.value).toBe(false)
})

it('FE-08 immediate purchase leaves existing cart lines and storage unchanged', async () => {
  const { ref } = await import('vue')
  const { useProductPurchase } = await import('@/composables/useProductPurchase')
  const { useBuyNowStore } = await import('@/stores/buyNow')
  const cart = useCartStore(); cart.addItem(line(), 2)
  const before = JSON.stringify(cart.items)
  const stored = localStorage.getItem('cart_items')
  const page = harness(() => useProductPurchase(ref(product())))
  page.syncSelectedSku(false)
  page.quantity.value = 2
  expect(page.buyNow()).toBe(true)
  expect(useBuyNowStore().item?.quantity).toBe(2)
  expect(JSON.stringify(cart.items)).toBe(before)
  expect(localStorage.getItem('cart_items')).toBe(stored)
})

it('FE-05 stops active payment polling when switching methods and does not restore the old payment', async () => {
  vi.useFakeTimers()
  vi.spyOn(window, 'scrollTo').mockImplementation(() => {})
  const { paymentAPI } = await import('@/api/order')
  const capture = vi.spyOn(paymentAPI, 'capture').mockResolvedValue(response({ status: 'pending' } as Awaited<ReturnType<typeof paymentAPI.capture>>['data']))
  const page = harness(usePayment)
  page.order.value = { order_no: 'DJ1', status: 'pending_payment', expires_at: new Date(Date.now() + 300_000).toISOString() } as NonNullable<typeof page.order.value>
  page.cachedPayment.value = { payment_id: 1, channel_id: 1, status: 'pending', interaction_mode: 'qr', pay_url: 'https://pay.example.com' }
  page.restoreCachedPayment()
  await Promise.resolve()
  expect(page.pollingActive.value).toBe(true)
  page.handleChangePaymentMethod()
  expect(page.pollingActive.value).toBe(false)
  expect(page.paymentResult.value).toBeNull()
  const calls = capture.mock.calls.length
  await vi.advanceTimersByTimeAsync(6000)
  expect(capture).toHaveBeenCalledTimes(calls)
  expect(page.paymentResult.value).toBeNull()
  expect(page.submitting.value).toBe(false)
})

it('FE-25 sends the selected language in API request headers', async () => {
  const { api } = await import('@/api/client')
  await setI18nLocale('zh-TW')
  const fetch = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify(response({})), { headers: { 'content-type': 'application/json' } }))
  await api.get('/public/config')
  expect(new Headers(fetch.mock.calls[0]?.[1]?.headers).get('X-Lang')).toBe('zh-TW')
})
