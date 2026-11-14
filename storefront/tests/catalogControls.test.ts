import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, nextTick, ref } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { ApiError } from '@/api/client'
import { catalogFacetAPI } from '@/api/catalog'
import type { CatalogFacetOption } from '@/api/types'
import { useCatalogFacets } from '@/composables/useCatalogFacets'
import { CatalogFacetControls } from '@/components/product/CatalogFacetControls'
import { ProductBadges } from '@/components/product/ProductBadges'
import { CategorySidebar } from '@/components/product/CategorySidebar'
import { CatalogResponseError, catalogReadError } from '@/utils/catalogReadError'
import i18n from '@/i18n'
const wrappers: VueWrapper[] = []
const caps = { schema_version: 1, dimensions: ['country'] as const, revision_format: 'decimal_string' as const, supports_product_filters: true, supports_value_search: true }
const option = (value_id: string, text: string): CatalogFacetOption => ({ value_id, label: { 'zh-CN': text }, product_count: 1 })
const reply = (items: CatalogFacetOption[], page = 1, total = items.length, page_size = 20) => ({ status_code: 0, msg: 'ok', data: items, pagination: { page, page_size, total, total_page: Math.ceil(total / page_size) } })
function fixture() {
  let model!: ReturnType<typeof useCatalogFacets>
  const apply = vi.fn()
  const wrapper = mount(defineComponent({ setup() {
    model = useCatalogFacets(ref(null), ref(''))
    return () => h(CatalogFacetControls, { model, retry: model.retryCapabilities, mobile: true, onApply: apply })
  } }), { global: { plugins: [createPinia(), i18n] } })
  wrappers.push(wrapper)
  return { model, wrapper, apply }
}
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  vi.spyOn(catalogFacetAPI, 'capabilities').mockResolvedValue({ status_code: 0, msg: 'ok', data: { ...caps, dimensions: [...caps.dimensions] } })
  vi.spyOn(catalogFacetAPI, 'values').mockImplementation(async (params) => params.value_id ? reply([option(params.value_id, '原地区名称')], 1, 1, 1) : reply([option('OTHER', '其他地区')], params.page ?? 1, 21))
})
afterEach(() => { wrappers.splice(0).forEach((wrapper) => wrapper.unmount()); document.body.style.overflow = ''; vi.restoreAllMocks(); vi.useRealTimers() })

describe('catalog controls', () => {
  it('displays the exact selected label instead of an internal identifier and supports search, paging and clear', async () => {
    const { model, wrapper, apply } = fixture()
    model.restore({ facet_country: 'supplier:000Ab' }); await model.initialize(); await flushPromises()
    expect(wrapper.text()).toContain('原地区名称')
    expect(wrapper.text()).not.toContain('supplier:000Ab')
    expect(wrapper.find('label').attributes('for')).toBe(wrapper.find('input').attributes('id'))
    await wrapper.find('input').setValue('新地区'); await vi.advanceTimersByTimeAsync(300); await flushPromises()
    expect(catalogFacetAPI.values).toHaveBeenCalledWith(expect.objectContaining({ option_search: '新地区', page: 1 }))
    const more = wrapper.findAll('button').find((button) => button.text() === '加载更多')!
    await more.trigger('click'); await flushPromises()
    expect(catalogFacetAPI.values).toHaveBeenCalledWith(expect.objectContaining({ page: 2, dimension: 'country' }))
    await wrapper.get('button[aria-label="清除国家或地区"]').trigger('click')
    expect(model.filters.value).toEqual({})
    await wrapper.findAll('button').find((button) => button.text() === '查看商品')!.trigger('click')
    expect(apply).toHaveBeenCalledTimes(1)
  })
  it('keeps a zero-match option search available so the customer can clear it', async () => {
    const { model, wrapper } = fixture(); await model.initialize(); await flushPromises()
    vi.mocked(catalogFacetAPI.values).mockImplementation(async (params) => params.option_search ? reply([]) : reply([option('CA', '加拿大')]))
    await wrapper.find('input').setValue('无匹配'); await vi.advanceTimersByTimeAsync(300); await flushPromises()
    expect(wrapper.find('input').exists()).toBe(true)
    expect(wrapper.text()).toContain('未找到匹配选项')
    await wrapper.find('input').setValue(''); await vi.advanceTimersByTimeAsync(300); await flushPromises()
    expect(wrapper.text()).toContain('加拿大')
  })
  it('shows a typed failure and retry rather than an authoritative empty option list or raw response', async () => {
    vi.mocked(catalogFacetAPI.values).mockRejectedValue(new ApiError('<html>SYNTHETIC_PRIVATE_RESPONSE</html>', 429, 429))
    const { model, wrapper } = fixture(); await model.initialize(); await flushPromises()
    expect(wrapper.text()).toContain('请求过于频繁')
    expect(wrapper.text()).not.toContain('未找到匹配选项')
    expect(wrapper.text()).not.toContain('SYNTHETIC_PRIVATE_RESPONSE')
    vi.mocked(catalogFacetAPI.values).mockResolvedValue(reply([option('CA', '恢复地区')]))
    await wrapper.get('[role="alert"] button').trigger('click'); await flushPromises()
    expect(wrapper.text()).toContain('恢复地区')
    expect(wrapper.text()).not.toContain('请求过于频繁')
  })
  it('blocks unsupported selected links until they are explicitly cleared', async () => {
    vi.mocked(catalogFacetAPI.capabilities).mockRejectedValue(new ApiError('not found', 404, 404))
    const { model, wrapper } = fixture(); model.restore({ facet_country: 'US' }); await model.initialize(); await flushPromises()
    expect(wrapper.text()).toContain('当前无法使用这些筛选条件')
    await wrapper.findAll('button').find((button) => button.text() === '清除条件')!.trigger('click'); await nextTick()
    expect(model.blocked.value).toBe(false)
    expect(wrapper.text()).not.toContain('当前无法使用这些筛选条件')
  })
})

describe('mobile filter drawer lifecycle', () => {
  it('provides a named dialog, locks background scrolling and restores focus on Escape', async () => {
    const open = ref(false)
    document.body.style.overflow = 'scroll'
    const wrapper = mount(defineComponent({ setup() { return () => h(CategorySidebar, { categories: [], showDrawer: open.value, 'onUpdate:showDrawer': (value: boolean) => { open.value = value } }) } }), { attachTo: document.body, global: { plugins: [createPinia(), i18n] } })
    wrappers.push(wrapper)
    const trigger = wrapper.findAll('button').find((button) => button.text() === '筛选')!
    ;(trigger.element as HTMLButtonElement).focus()
    await trigger.trigger('click'); await flushPromises()
    const dialog = document.querySelector('[role="dialog"]')
    expect(dialog?.getAttribute('aria-label')).toBe('筛选')
    expect(dialog?.getAttribute('aria-modal')).toBe('true')
    expect(document.body.style.overflow).toBe('hidden')
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); await flushPromises()
    expect(open.value).toBe(false)
    expect(document.body.style.overflow).toBe('scroll')
    expect(document.activeElement).toBe(trigger.element)
  })
  it('removes the exact keyboard listener and background lock when leaving the page', async () => {
    const add = vi.spyOn(window, 'addEventListener')
    const remove = vi.spyOn(window, 'removeEventListener')
    const open = ref(false)
    const wrapper = mount(defineComponent({ setup() { return () => h(CategorySidebar, { categories: [], showDrawer: open.value, 'onUpdate:showDrawer': (value: boolean) => { open.value = value } }) } }), { attachTo: document.body, global: { plugins: [createPinia(), i18n] } })
    wrappers.push(wrapper)
    open.value = true; await flushPromises()
    const listener = add.mock.calls.find(([name]) => name === 'keydown')?.[1]
    expect(listener).toBeTypeOf('function')
    wrapper.unmount()
    expect(remove).toHaveBeenCalledWith('keydown', listener)
    expect(document.body.style.overflow).toBe('')
  })
})

it('keeps transport, rate limits, server, changed conditions and malformed responses distinct', () => {
  expect(catalogReadError(new ApiError('limited', 429, 200))).toBe('rateLimited')
  expect(catalogReadError(new ApiError('unavailable', 503, 503))).toBe('unavailable')
  expect(catalogReadError(new ApiError('forbidden', 403, 403))).toBe('accessDenied')
  expect(catalogReadError(new ApiError('changed', 400, 200))).toBe('conditionsChanged')
  expect(catalogReadError(new CatalogResponseError())).toBe('invalidResponse')
  expect(catalogReadError(new Error('offline'))).toBe('requestFailed')
})

it('labels upstream delivery without incorrectly promising manual handling', () => {
  const wrapper = mount(ProductBadges, { props: { product: { id: 1, slug: 'fixture', title: { 'zh-CN': 'Fixture' }, price_amount: '10.00', purchase_type: 'guest', fulfillment_type: 'upstream', stock_display_mode: 'exact', stock_status: 'in_stock' } }, global: { plugins: [createPinia(), i18n] } })
  wrappers.push(wrapper)
  expect(wrapper.text()).toContain('下单后交付')
  expect(wrapper.text()).not.toContain('人工交付')
})
