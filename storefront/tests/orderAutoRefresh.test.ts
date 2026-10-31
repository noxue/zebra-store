import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, h, reactive } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { guestOrderAPI, paymentAPI, userOrderAPI } from '@/api/order'
import type { Order } from '@/api/types'
import { useGuestOrderDetail } from '@/composables/useGuestOrderDetail'
import { useOrderDetail } from '@/composables/useOrderDetail'
import i18n from '@/i18n'
import { clearGuestOrderAuth, saveGuestOrderAuth } from '@/utils/guestOrderAuth'
import { OrderBody } from '@/components/order/OrderBody'

let route = reactive({ params: { order_no: 'DJ1' } })
vi.mock('vue-router', async (original) => ({ ...await original<object>(), useRoute: () => route, useRouter: () => ({ push: vi.fn() }) }))
const wrappers: VueWrapper[] = []
const response = (data: Order) => ({ status_code: 0, msg: 'ok', data })
const order = (status = 'fulfilling', extra: Partial<Order> = {}): Order => ({ order_no: 'DJ1', status, currency: 'CNY', total_amount: '10.00', created_at: '', ...extra })
const credentials = { email: 'guest@example.test', order_password: 'secret123' }

function harness<Use extends () => ReturnType<typeof useOrderDetail> | ReturnType<typeof useGuestOrderDetail>>(use: Use) {
  let page!: ReturnType<Use>
  const wrapper = mount(defineComponent({ setup() {
    page = use() as ReturnType<Use>
    return () => page.order.value ? h(OrderBody, { order: page.order.value, helpers: page.helpers }) : h('div')
  } }), { global: { plugins: [i18n], stubs: { RouterLink: { template: '<a><slot /></a>' } } } })
  wrappers.push(wrapper)
  return { page, wrapper }
}

beforeEach(() => {
  vi.useFakeTimers()
  setActivePinia(createPinia())
  route = reactive({ params: { order_no: 'DJ1' } })
  clearGuestOrderAuth()
  vi.spyOn(document, 'hidden', 'get').mockReturnValue(false)
  vi.spyOn(paymentAPI, 'latest').mockRejectedValue(new Error('no payment'))
  vi.spyOn(guestOrderAPI, 'latestPayment').mockRejectedValue(new Error('no payment'))
})
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  vi.restoreAllMocks()
  vi.useRealTimers()
  clearGuestOrderAuth()
})

describe.each(['member', 'guest'] as const)('FE-ORDER-LIVE-01 %s delivery updates', (mode) => {
  const setup = () => {
    if (mode === 'guest') saveGuestOrderAuth(credentials)
    const api = mode === 'guest' ? guestOrderAPI : userOrderAPI
    const detail = vi.spyOn(api, 'detail')
    const use = mode === 'guest' ? useGuestOrderDetail : useOrderDetail
    return { detail, use }
  }

  it('updates child status, card secrets and instructions without reloading the page', async () => {
    const { detail, use } = setup()
    detail.mockResolvedValueOnce(response(order('fulfilling', { children: [order('fulfilling', { order_no: 'DJ1-1' })] })))
      .mockResolvedValue(response(order('delivered', { children: [order('delivered', {
        order_no: 'DJ1-1', fulfillment: { type: 'auto', status: 'delivered', payload: 'CARD-LIVE-123' },
        items: [{ title: { 'zh-CN': '测试商品' }, quantity: 1, unit_price: '10.00', total_price: '10.00', instructions: { 'zh-CN': '<p>交付使用说明 LIVE</p>' } }],
      })] })))
    const { page, wrapper } = harness(use)
    await flushPromises()
    expect(page.order.value?.status).toBe('fulfilling')
    expect(wrapper.text()).not.toContain('CARD-LIVE-123')
    await vi.advanceTimersByTimeAsync(3_000)
    expect(page.loading.value).toBe(false)
    expect(page.order.value?.status).toBe('delivered')
    expect(wrapper.text()).toContain('CARD-LIVE-123')
    expect(wrapper.text()).toContain('交付使用说明 LIVE')
    if (mode === 'guest') expect(detail).toHaveBeenLastCalledWith('DJ1', credentials, { silentBusinessError: true })
    await vi.advanceTimersByTimeAsync(30_000)
    expect(detail).toHaveBeenCalledTimes(2)
  })

  it('keeps existing content on a network failure and retries until delivered', async () => {
    const { detail, use } = setup()
    detail.mockResolvedValueOnce(response(order())).mockRejectedValueOnce(new Error('offline')).mockResolvedValue(response(order('delivered')))
    const { page } = harness(use)
    await flushPromises()
    await vi.advanceTimersByTimeAsync(3_000)
    expect(page.order.value?.status).toBe('fulfilling')
    expect(page.loading.value).toBe(false)
    if ('authError' in page) expect(page.authError.value).toBe('')
    await vi.advanceTimersByTimeAsync(3_000)
    expect(page.order.value?.status).toBe('delivered')
  })

  it('pauses while hidden, refreshes on return and cleans up on unmount', async () => {
    const { detail, use } = setup()
    detail.mockResolvedValue(response(order('partially_delivered')))
    const { wrapper } = harness(use)
    await flushPromises()
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(true)
    document.dispatchEvent(new Event('visibilitychange'))
    await vi.advanceTimersByTimeAsync(30_000)
    expect(detail).toHaveBeenCalledTimes(1)
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(false)
    document.dispatchEvent(new Event('visibilitychange'))
    await flushPromises()
    expect(detail).toHaveBeenCalledTimes(2)
    wrapper.unmount()
    window.dispatchEvent(new Event('focus'))
    await vi.advanceTimersByTimeAsync(30_000)
    expect(detail).toHaveBeenCalledTimes(2)
  })

  it('ignores a delayed response for the previous route', async () => {
    const { detail, use } = setup()
    let resolveOld!: (value: ReturnType<typeof response>) => void
    detail.mockResolvedValueOnce(response(order()))
      .mockImplementationOnce(() => new Promise<ReturnType<typeof response>>((resolve) => { resolveOld = resolve }))
      .mockResolvedValue(response(order('delivered', { order_no: 'DJ2' })))
    const { page } = harness(use)
    await flushPromises()
    await vi.advanceTimersByTimeAsync(3_000)
    route.params.order_no = 'DJ2'
    await flushPromises()
    expect(page.order.value?.order_no).toBe('DJ2')
    resolveOld(response(order('delivered')))
    await flushPromises()
    expect(page.order.value?.order_no).toBe('DJ2')
  })
})

it('FE-ORDER-LIVE-01 does not overlap slow requests or stop while an active child remains', async () => {
  let resolve!: (value: ReturnType<typeof response>) => void
  const detail = vi.spyOn(userOrderAPI, 'detail').mockResolvedValueOnce(response(order('partially_refunded', { children: [order()] })))
    .mockImplementationOnce(() => new Promise((done) => { resolve = done }))
    .mockResolvedValue(response(order('completed')))
  const { page } = harness(useOrderDetail)
  await flushPromises()
  await vi.advanceTimersByTimeAsync(15_000)
  window.dispatchEvent(new Event('focus'))
  expect(detail).toHaveBeenCalledTimes(2)
  expect(page.loading.value).toBe(false)
  resolve(response(order('partially_delivered')))
  await flushPromises()
  await vi.advanceTimersByTimeAsync(3_000)
  expect(page.order.value?.status).toBe('completed')
  expect(detail).toHaveBeenCalledTimes(3)
})

it('FE-ORDER-LIVE-01 clears guest credentials without allowing an in-flight response to restore the order', async () => {
  saveGuestOrderAuth(credentials)
  let resolve!: (value: ReturnType<typeof response>) => void
  const detail = vi.spyOn(guestOrderAPI, 'detail').mockResolvedValueOnce(response(order()))
    .mockImplementationOnce(() => new Promise((done) => { resolve = done }))
  const { page } = harness(useGuestOrderDetail)
  await flushPromises()
  await vi.advanceTimersByTimeAsync(3_000)
  page.clearAuth()
  resolve(response(order('delivered')))
  await flushPromises()
  expect(page.order.value).toBeNull()
  expect(page.viewState.value).toBe('auth')
  await vi.advanceTimersByTimeAsync(30_000)
  expect(detail).toHaveBeenCalledTimes(2)
})

it.each(['delivered', 'completed', 'canceled', 'expired', 'refunded'])('FE-ORDER-LIVE-01 leaves %s orders idle', async (status) => {
  const detail = vi.spyOn(userOrderAPI, 'detail').mockResolvedValue(response(order(status)))
  harness(useOrderDetail)
  await flushPromises()
  await vi.advanceTimersByTimeAsync(30_000)
  expect(detail).toHaveBeenCalledTimes(1)
})

it('FE-ORDER-LIVE-01 follows payment and asynchronous refund transitions', async () => {
  const detail = vi.spyOn(userOrderAPI, 'detail').mockResolvedValueOnce(response(order('pending_payment')))
    .mockResolvedValueOnce(response(order('paid')))
    .mockResolvedValueOnce(response(order('delivered', { refund_state: 'processing' })))
    .mockResolvedValue(response(order('refunded', { refund_state: 'succeeded' })))
  const { page } = harness(useOrderDetail)
  await flushPromises()
  await vi.advanceTimersByTimeAsync(9_000)
  expect(page.order.value?.status).toBe('refunded')
  await vi.advanceTimersByTimeAsync(9_000)
  expect(detail).toHaveBeenCalledTimes(4)
})

it('FE-ORDER-LIVE-01 waits for the initial payment lookup before scheduling background requests', async () => {
  const detail = vi.spyOn(userOrderAPI, 'detail').mockResolvedValue(response(order('pending_payment')))
  let resolve!: (value: Awaited<ReturnType<typeof paymentAPI.latest>>) => void
  vi.spyOn(paymentAPI, 'latest').mockImplementationOnce(() => new Promise((done) => { resolve = done }))
  const { page } = harness(useOrderDetail)
  await flushPromises()
  await vi.advanceTimersByTimeAsync(15_000)
  window.dispatchEvent(new Event('focus'))
  expect(detail).toHaveBeenCalledTimes(1)
  resolve({ status_code: 0, msg: 'ok', data: { provider_type: 'manual_qr' } })
  await flushPromises()
  expect(page.hasManualPayment.value).toBe(true)
  await vi.advanceTimersByTimeAsync(3_000)
  expect(detail).toHaveBeenCalledTimes(2)
})
