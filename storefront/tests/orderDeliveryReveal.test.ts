import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { Order } from '@/api/types'
import { OrderBody } from '@/components/order/OrderBody'
import { useOrderDisplayHelpers } from '@/composables/useOrderDisplayHelpers'
import { useOrderProcessing } from '@/composables/useOrderProcessing'
import i18n from '@/i18n'

const order = (status = 'fulfilling', extra: Partial<Order> = {}): Order => ({ order_no: 'DJ1', status, currency: 'CNY', total_amount: '10.00', created_at: '', ...extra })
const delivered = (no = 'DJ1-1', payload = 'CARD-123'): Order => order('delivered', { order_no: no, fulfillment: { type: 'auto', status: 'delivered', payload } })
const wrappers: VueWrapper[] = []
const calls: Array<{ no: string | undefined; options: ScrollIntoViewOptions }> = []
const originalScroll = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'scrollIntoView')
function page(initial = order()) {
  const current = ref(initial)
  const wrapper = mount(defineComponent({ setup() {
    const helpers = useOrderDisplayHelpers(current)
    return () => h(OrderBody, { order: current.value, helpers })
  } }), { attachTo: document.body, global: { plugins: [i18n], stubs: { RouterLink: { template: '<a><slot /></a>' } } } })
  wrappers.push(wrapper)
  return { current, wrapper }
}
const settle = () => vi.advanceTimersByTimeAsync(50)

beforeEach(() => {
  vi.useFakeTimers()
  setActivePinia(createPinia())
  calls.length = 0
  document.body.style.overflow = ''
  vi.spyOn(document, 'hidden', 'get').mockReturnValue(false)
  vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false })))
  Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', { configurable: true, value: function (this: HTMLElement, options: ScrollIntoViewOptions) {
    calls.push({ no: this.dataset.orderDelivery, options })
  } })
})
afterEach(() => {
  wrappers.splice(0).forEach((wrapper) => wrapper.unmount())
  if (originalScroll) Object.defineProperty(HTMLElement.prototype, 'scrollIntoView', originalScroll)
  else Reflect.deleteProperty(HTMLElement.prototype, 'scrollIntoView')
  document.body.style.overflow = ''
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('FE-ORDER-REVEAL-01 delivery visibility', () => {
  it('reveals an initially delivered order and respects reduced motion', async () => {
    vi.mocked(window.matchMedia).mockReturnValue({ matches: true } as MediaQueryList)
    page(delivered('DJ1'))
    await settle()
    expect(calls).toEqual([{ no: 'DJ1', options: { behavior: 'instant', block: 'start', inline: 'nearest' } }])
  })

  it('reveals new child content once and ignores unchanged polling replacements', async () => {
    const { current } = page()
    await settle()
    expect(calls).toHaveLength(0)
    current.value = order('completed', { children: [delivered()] })
    await settle()
    expect(calls.map((row) => row.no)).toEqual(['DJ1-1'])
    current.value = order('completed', { children: [delivered()], updated_at: '2026-10-30T10:00:00Z' } as Partial<Order>)
    await settle()
    expect(calls).toHaveLength(1)
  })

  it('selects the first simultaneous delivery, then follows a later child delivery', async () => {
    const { current } = page(order('partially_delivered', { children: [delivered('A'), delivered('B'), order('fulfilling', { order_no: 'C' })] }))
    await settle()
    expect(calls.map((row) => row.no)).toEqual(['A'])
    current.value = order('completed', { children: [delivered('A'), delivered('B'), delivered('C')] })
    await settle()
    expect(calls.map((row) => row.no)).toEqual(['A', 'C'])
  })

  it('waits for actual payload or structured manual delivery rather than an empty delivered status', async () => {
    const { current } = page(delivered('DJ1', ''))
    await settle()
    expect(calls).toHaveLength(0)
    current.value = order('completed', { fulfillment: { type: 'manual', status: 'delivered', delivery_data: { note: '已开通', entries: [{ key: '账号', value: 'buyer@example.test' }] } } })
    await settle()
    expect(calls.map((row) => row.no)).toEqual(['DJ1'])
  })

  it('reveals newly added delivery instructions without jumping on the next identical response', async () => {
    const { current, wrapper } = page(delivered('DJ1'))
    await settle()
    current.value = delivered('DJ1')
    current.value.items = [{ title: {}, quantity: 1, unit_price: '10.00', total_price: '10.00', instructions: { 'zh-CN': '<p>使用方法</p><img src=x onerror="alert(1)">' } }]
    await settle()
    expect(calls).toHaveLength(2)
    expect(wrapper.text()).toContain('使用方法')
    // DLV-02: auto-reveal must keep using the existing sanitized instructions renderer.
    expect(wrapper.find('img[onerror]').exists()).toBe(false)
    current.value = { ...current.value }
    await settle()
    expect(calls).toHaveLength(2)
  })

  it('waits for the processing overlay to unlock scrolling and for a hidden page to return', async () => {
    document.body.style.overflow = 'hidden'
    page(delivered('DJ1'))
    await settle()
    expect(calls).toHaveLength(0)
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(true)
    document.body.style.overflow = ''
    await settle()
    expect(calls).toHaveLength(0)
    vi.spyOn(document, 'hidden', 'get').mockReturnValue(false)
    document.dispatchEvent(new Event('visibilitychange'))
    await settle()
    expect(calls).toHaveLength(1)
  })

  it('resets on an order change and cancels pending reveals on unmount', async () => {
    const { current, wrapper } = page(delivered('DJ1'))
    await settle()
    current.value = delivered('DJ2')
    await settle()
    expect(calls.map((row) => row.no)).toEqual(['DJ1', 'DJ2'])
    current.value = delivered('DJ3')
    wrapper.unmount()
    await settle()
    expect(calls).toHaveLength(2)
  })

  it('DLV-05 keeps truncated delivery download and copy controls outside the scrolling content', async () => {
    const { wrapper } = page(order('completed', { fulfillment: { type: 'auto', status: 'delivered', payload: 'CARD-1', payload_line_count: 3000, payload_truncated: true } }))
    await settle()
    const content = wrapper.get('[data-delivery-content]')
    expect(content.find('button').exists()).toBe(false)
    expect(wrapper.findAll('button').some((button) => button.text().includes('下载'))).toBe(true)
    expect(content.text()).toContain('CARD-1')
    expect(content.text()).toContain('3000')
  })

  it('lets a partially delivered order expose its cards while the other children keep processing', async () => {
    const current = ref(order('fulfilling', { children: [order('fulfilling', { order_no: 'A' }), order('fulfilling', { order_no: 'B' })] }))
    let processing!: ReturnType<typeof useOrderProcessing>
    const wrapper = mount(defineComponent({ setup() { processing = useOrderProcessing(current); return () => h('div') } }))
    wrappers.push(wrapper)
    expect(processing.open.value).toBe(true)
    current.value = order('partially_delivered', { children: [delivered('A'), order('fulfilling', { order_no: 'B' })] })
    await settle()
    expect(processing.open.value).toBe(false)
  })
})
