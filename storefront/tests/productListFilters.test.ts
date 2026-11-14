import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { createMemoryHistory, createRouter } from 'vue-router'
import { catalogFacetAPI, categoryAPI, productAPI } from '@/api/catalog'
import { ApiError } from '@/api/client'
import type { Product, ProductListParams } from '@/api/types'
import { useProductList } from '@/composables/useProductList'
import { CatalogListFeedback } from '@/components/product/CatalogListFeedback'
import i18n from '@/i18n'
const wrappers: VueWrapper[] = []
const product = (id: number): Product => ({ id, slug: `p${id}`, title: { 'zh-CN': `商品${id}` }, price_amount: '8.00', purchase_type: 'guest', fulfillment_type: 'auto', stock_display_mode: 'exact' })
const page = (params: ProductListParams = {}, id = 1) => ({ status_code: 0, msg: 'ok', data: [product(id)], pagination: { page: params.page ?? 1, page_size: params.page_size ?? 20, total: 45, total_page: 3 } })
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((r) => { resolve = r }); return { promise, resolve } }
async function fixture(url = '/products') {
  let list!: ReturnType<typeof useProductList>
  const empty = defineComponent({ render: () => h('div') })
  const router = createRouter({ history: createMemoryHistory(), routes: [
    { path: '/', name: 'home', component: empty }, { path: '/products', name: 'products', component: empty }, { path: '/categories/:slug', name: 'category-products', component: empty },
  ] })
  await router.push(url); await router.isReady()
  const wrapper = mount(defineComponent({ setup() { list = useProductList({ homeRouteName: 'products' }); return () => h(CatalogListFeedback, { list }) } }), { global: { plugins: [router, i18n] } })
  wrappers.push(wrapper)
  return { list, router, wrapper }
}
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  vi.spyOn(categoryAPI, 'list').mockResolvedValue({ status_code: 0, msg: 'ok', data: [{ id: 22, parent_id: 0, slug: 'ip', name: { 'zh-CN': 'IP' }, sort_order: 0 }] })
  vi.spyOn(productAPI, 'list').mockImplementation(async (params) => page(params))
  vi.spyOn(catalogFacetAPI, 'capabilities').mockResolvedValue({ status_code: 0, msg: 'ok', data: { schema_version: 1, revision_format: 'decimal_string', dimensions: ['family', 'project', 'country', 'city', 'ip_type', 'line'], supports_product_filters: true, supports_value_search: true } })
  vi.spyOn(catalogFacetAPI, 'values').mockImplementation(async (params) => ({ status_code: 0, msg: 'ok', data: params.value_id ? [{ value_id: params.value_id, label: { 'zh-CN': '已选地区' }, product_count: 1 }] : [], pagination: { page: params.page ?? 1, page_size: params.page_size ?? 20, total: params.value_id ? 1 : 0, total_page: params.value_id ? 1 : 0 } }))
})
afterEach(() => { wrappers.splice(0).forEach((wrapper) => wrapper.unmount()); vi.restoreAllMocks(); vi.useRealTimers() })
const tick = async () => { await flushPromises(); await vi.advanceTimersByTimeAsync(300); await flushPromises() }

describe('listing filters and real route synchronization', () => {
  it('restores exact IDs, keyword and page then updates the URL without loops or lost parameters', async () => {
    const { list, router } = await fixture('/products?facet_country=supplier%3A000Ab&search=hello&page=2&campaign=keep')
    await list.initialize()
    expect(productAPI.list).toHaveBeenCalledWith(expect.objectContaining({ facet_country: 'supplier:000Ab', search: 'hello', page: 2 }))
    expect(list.currentPage.value).toBe(2)
    list.facets.choose('country', 'CA'); await tick()
    expect(router.currentRoute.value.query).toEqual({ search: 'hello', campaign: 'keep', facet_country: 'CA' })
    expect(list.currentPage.value).toBe(1)
    expect(productAPI.list).toHaveBeenCalledTimes(2)
    expect(productAPI.list).toHaveBeenLastCalledWith(expect.objectContaining({ facet_country: 'CA', search: 'hello', page: 1 }))
  })
  it('restores browser navigation and preserves the URL page rather than resetting it', async () => {
    const { list, router } = await fixture('/products?facet_country=US')
    await list.initialize()
    await router.push('/products?facet_country=CA&page=3'); await tick()
    expect(list.facets.filters.value.facet_country).toBe('CA')
    expect(list.currentPage.value).toBe(3)
    expect(productAPI.list).toHaveBeenLastCalledWith(expect.objectContaining({ facet_country: 'CA', page: 3 }))
    router.back(); await tick()
    expect(list.facets.filters.value.facet_country).toBe('US')
    expect(list.currentPage.value).toBe(1)
    expect(productAPI.list).toHaveBeenLastCalledWith(expect.objectContaining({ facet_country: 'US', page: 1 }))
  })
  it('clears dependent facets when the category changes and only fetches the new scope', async () => {
    const { list, router } = await fixture('/products?facet_country=US&facet_city=city01')
    await list.initialize()
    list.selectCategory(22); await tick()
    expect(list.facets.filters.value).toEqual({})
    expect(router.currentRoute.value.path).toBe('/categories/ip')
    expect(productAPI.list).toHaveBeenLastCalledWith({ page: 1, page_size: 20, category_id: 22 })
    expect(productAPI.list).toHaveBeenCalledTimes(2)
  })
  it('discards an old product result after the keyword changes', async () => {
    const old = deferred<ReturnType<typeof page>>()
    vi.mocked(productAPI.list).mockImplementation(async (params) => params?.search === 'new' ? page(params, 2) : old.promise)
    const { list } = await fixture()
    const initializing = list.initialize(); await flushPromises()
    expect(list.ready.value).toBe(true)
    list.searchQuery.value = 'new'; await tick()
    expect(list.products.value.map((item) => item.id)).toEqual([2])
    old.resolve(page({}, 1)); await initializing; await flushPromises()
    expect(list.products.value.map((item) => item.id)).toEqual([2])
    expect(list.loading.value).toBe(false)
  })
  it('does not load an unfiltered catalog for invalid or unsupported selected URLs', async () => {
    const { list, router, wrapper } = await fixture('/products?facet_country=A&facet_country=B&campaign=keep')
    await list.initialize()
    expect(productAPI.list).not.toHaveBeenCalled()
    expect(list.blocked.value).toBe(true)
    expect(wrapper.text()).toContain('筛选链接不完整或无效')
    const clear = wrapper.findAll('button').find((button) => button.text() === '清除筛选条件')
    expect(clear).toBeDefined()
    await clear!.trigger('click'); await tick()
    expect(list.blocked.value).toBe(false)
    expect(router.currentRoute.value.query).toEqual({ campaign: 'keep' })
    expect(productAPI.list).toHaveBeenCalledTimes(1)
  })
  it('reports product errors with retry and does not present them as an empty shelf', async () => {
    vi.mocked(productAPI.list).mockRejectedValue(new Error('offline'))
    const { list, wrapper } = await fixture(); await list.initialize()
    expect(list.error.value).toBe('requestFailed')
    expect(wrapper.text()).toContain('商品加载失败')
    expect(wrapper.text()).not.toContain('暂无商品')
    vi.mocked(productAPI.list).mockResolvedValue(page())
    await wrapper.find('button').trigger('click'); await flushPromises()
    expect(list.error.value).toBeNull()
    expect(list.products.value[0]?.id).toBe(1)
  })
  it('recovers a failed capability check before fetching selected product filters', async () => {
    vi.mocked(catalogFacetAPI.capabilities).mockRejectedValue(new ApiError('offline', 500, 500))
    const { list } = await fixture('/products?facet_country=US'); await list.initialize()
    expect(list.blocked.value).toBe(true); expect(productAPI.list).not.toHaveBeenCalled()
    vi.mocked(catalogFacetAPI.capabilities).mockResolvedValue({ status_code: 0, msg: 'ok', data: { schema_version: 1, revision_format: 'decimal_string', dimensions: ['country'], supports_product_filters: true, supports_value_search: true } })
    await list.retryFilters()
    expect(list.blocked.value).toBe(false)
    expect(productAPI.list).toHaveBeenCalledWith(expect.objectContaining({ facet_country: 'US' }))
  })
  it('blocks missing categories and malformed page/keyword URLs without silently changing their meaning', async () => {
    for (const url of ['/categories/missing', '/products?search=A&search=B', '/products?page=18446744073709551615']) {
      const { list } = await fixture(url); await list.initialize()
      expect(list.blocked.value).toBe(true)
    }
    expect(productAPI.list).not.toHaveBeenCalled()
  })
})
