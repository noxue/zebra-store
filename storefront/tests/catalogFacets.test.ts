import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { ApiError } from '@/api/client'
import { catalogFacetAPI } from '@/api/catalog'
import type { CatalogFacetCapabilities, CatalogFacetKey, CatalogFacetOption } from '@/api/types'
import { useCatalogFacets } from '@/composables/useCatalogFacets'
import { parseFacetQuery, selectFacet, withoutFacetQuery } from '@/utils/catalogFacets'

const wrappers: VueWrapper[] = []
const caps = (dimensions: CatalogFacetKey[] = ['country', 'city']): CatalogFacetCapabilities => ({ schema_version: 1, dimensions, revision_format: 'decimal_string', supports_product_filters: true, supports_value_search: true })
const option = (value_id: string, label: string): CatalogFacetOption => ({ value_id, label: { 'zh-CN': label }, product_count: 1 })
const response = (items: CatalogFacetOption[], page = 1, total = items.length, page_size = 20) => ({ status_code: 0, msg: 'ok', data: items, pagination: { page, page_size, total, total_page: Math.ceil(total / page_size) } })
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((r) => { resolve = r }); return { promise, resolve } }
function fixture() {
  const category = ref<number | null>(null)
  const search = ref('')
  let model!: ReturnType<typeof useCatalogFacets>
  const wrapper = mount(defineComponent({ setup() { model = useCatalogFacets(category, search); return () => h('div') } }))
  wrappers.push(wrapper)
  return { model, category, search, wrapper }
}
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
  vi.spyOn(catalogFacetAPI, 'capabilities').mockResolvedValue({ status_code: 0, msg: 'ok', data: caps() })
  vi.spyOn(catalogFacetAPI, 'values').mockImplementation(async (params) => response([], params.page ?? 1, 0, params.page_size ?? 20))
})
afterEach(() => { wrappers.splice(0).forEach((wrapper) => wrapper.unmount()); vi.restoreAllMocks(); vi.useRealTimers() })

describe('stable catalog URLs and parent choices', () => {
  it('preserves opaque case and leading zeros and keeps unrelated URL parameters', () => {
    expect(parseFacetQuery({ facet_country: 'supplier:000Ab', facet_city: 'city.01', search: 'hello' })).toEqual({ ok: true, filters: { facet_country: 'supplier:000Ab', facet_city: 'city.01' } })
    expect(withoutFacetQuery({ facet_country: '000Ab', facet_future: ['bad'], search: 'hello', campaign: 'x' })).toEqual({ search: 'hello', campaign: 'x' })
    for (const query of [{ facet_country: '' }, { facet_country: ' padded' }, { facet_country: ['A', 'B'] }, { facet_country: null }, { facet_future: 'A' }, { facet_city: 'a'.repeat(129) }, { facet_city: '城市' }]) expect(parseFacetQuery(query)).toEqual({ ok: false })
  })
  it('clears later dependent choices without changing preceding selections', () => {
    expect(selectFacet({ facet_family: 'ip', facet_ip_type: 'native', facet_line: 'NONE', facet_country: 'US', facet_city: 'city01' }, 'country', 'CA')).toEqual({ facet_family: 'ip', facet_ip_type: 'native', facet_line: 'NONE', facet_country: 'CA' })
    expect(selectFacet({ facet_family: 'ip', facet_country: 'US' }, 'family', 'sms')).toEqual({ facet_family: 'sms' })
    expect(selectFacet({ facet_family: 'sms', facet_country: 'US' }, 'country', null)).toEqual({ facet_family: 'sms' })
  })
})

describe('server-paged catalog options', () => {
  it('resolves an exact selected ID outside the first page and outside option search', async () => {
    vi.mocked(catalogFacetAPI.values).mockImplementation(async (params) => params.value_id ? response([option('000Ab', '原地区')], 1, 1, 1) : response([option('OTHER', '其他')]))
    const { model } = fixture()
    model.restore({ facet_country: '000Ab' })
    await model.initialize()
    expect(model.states.country.selected?.label).toEqual({ 'zh-CN': '原地区' })
    expect(model.states.country.items.map((item) => item.value_id)).toEqual(['OTHER'])
    model.optionSearch('country', '不包含原地区')
    await vi.advanceTimersByTimeAsync(300); await flushPromises()
    expect(catalogFacetAPI.values).toHaveBeenCalledWith(expect.objectContaining({ dimension: 'country', value_id: '000Ab', page: 1, page_size: 1 }))
    expect(model.states.country.selected?.value_id).toBe('000Ab')
    expect(model.states.country.missingSelection).toBe(false)
  })
  it('keeps parent options switchable while descendants from the old selection exist', async () => {
    const { model } = fixture()
    model.restore({ facet_country: 'US', facet_city: 'city:01' })
    await model.initialize()
    const country = vi.mocked(catalogFacetAPI.values).mock.calls.find(([params]) => params.dimension === 'country')?.[0]
    const city = vi.mocked(catalogFacetAPI.values).mock.calls.find(([params]) => params.dimension === 'city')?.[0]
    expect(country?.facet_city).toBeUndefined()
    expect(city?.facet_country).toBe('US')
    model.states.city.search = '旧城市搜索'
    model.choose('country', 'CA')
    expect(model.states.city.search).toBe('')
    expect(model.filters.value).toEqual({ facet_country: 'CA' })
  })
  it('appends actual server pages with identity deduplication and renamed labels', async () => {
    vi.mocked(catalogFacetAPI.values).mockImplementation(async (params) => params.page === 2 ? response([option('A', '改名'), option('B', '新地区')], 2, 21) : response([option('A', '原名')], 1, 21))
    const { model } = fixture(); await model.initialize()
    await model.fetchOptions('country', true)
    expect(model.states.country.items.map((item) => [item.value_id, item.label['zh-CN']])).toEqual([['A', '改名'], ['B', '新地区']])
    expect(model.states.country.page).toBe(2)
    expect(model.states.country.totalPages).toBe(2)
  })
  it('discards delayed data after a category change and sends the new scope', async () => {
    const delayed = deferred<ReturnType<typeof response>>()
    let first = true
    vi.mocked(catalogFacetAPI.values).mockImplementation(async (params) => {
      if (params.dimension === 'country' && first) { first = false; return delayed.promise }
      return response([option('NEW', '新范围')])
    })
    const { model, category } = fixture()
    const initializing = model.initialize(); await flushPromises()
    category.value = 22
    delayed.resolve(response([option('OLD', '旧范围')]))
    await initializing; await vi.advanceTimersByTimeAsync(300); await flushPromises()
    expect(model.states.country.items.map((item) => item.value_id)).toEqual(['NEW'])
    expect(catalogFacetAPI.values).toHaveBeenCalledWith(expect.objectContaining({ category_id: 22, dimension: 'country' }))
  })
  it('prevents an old search response from overriding a newer option search', async () => {
    const delayed = deferred<ReturnType<typeof response>>()
    vi.mocked(catalogFacetAPI.values).mockImplementation(async (params) => params.option_search === 'old' ? delayed.promise : response([option('NEW', '新搜索')]))
    const { model } = fixture(); await model.initialize()
    model.optionSearch('country', 'old'); await vi.advanceTimersByTimeAsync(300)
    model.optionSearch('country', 'new'); await vi.advanceTimersByTimeAsync(300); await flushPromises()
    delayed.resolve(response([option('OLD', '旧搜索')])); await flushPromises()
    expect(model.states.country.items.map((item) => item.value_id)).toEqual(['NEW'])
    expect(model.states.country.loading).toBe(false)
  })
  it('keeps failures distinct from a confirmed empty result and supports retry', async () => {
    const { model } = fixture(); await model.initialize()
    expect(model.states.country.error).toBeNull()
    expect(model.states.country.total).toBe(0)
    vi.mocked(catalogFacetAPI.values).mockRejectedValue(new ApiError('busy', 429, 429))
    await model.fetchOptions('country')
    expect(model.states.country.error).toBe('rateLimited')
    vi.mocked(catalogFacetAPI.values).mockResolvedValue(response([option('A', '恢复')]))
    await model.fetchOptions('country')
    expect(model.states.country.error).toBeNull()
    expect(model.states.country.items[0]?.value_id).toBe('A')
  })
  it('marks an unavailable exact selection only after a successful lookup', async () => {
    const { model } = fixture(); model.restore({ facet_country: 'gone' }); await model.initialize()
    expect(model.states.country.missingSelection).toBe(true)
    expect(model.states.country.error).toBeNull()
    vi.mocked(catalogFacetAPI.values).mockImplementation(async (params) => {
      if (params.value_id) throw new Error('lookup failed')
      return response([])
    })
    await model.fetchOptions('country')
    expect(model.states.country.error).toBe('requestFailed')
    expect(model.states.country.missingSelection).toBe(false)
  })
  it('rejects malformed pagination and exact labels without treating them as missing', async () => {
    const { model } = fixture(); model.restore({ facet_country: 'A' })
    vi.mocked(catalogFacetAPI.values).mockImplementation(async (params) => params.value_id ? response([{ ...option('A', 'Bad'), label: { 'zh-CN': 'bad\nlabel' } }], 1, 1, 1) : response([]))
    await model.initialize()
    expect(model.states.country.error).toBe('invalidResponse')
    expect(model.states.country.missingSelection).toBe(false)
    model.clear()
    vi.mocked(catalogFacetAPI.values).mockResolvedValue({ ...response([]), pagination: { page: 1, page_size: 20, total: -1, total_page: 0 } })
    await model.fetchOptions('country')
    expect(model.states.country.error).toBe('invalidResponse')
  })
  it('treats malformed option data as an error and cannot update after unmount', async () => {
    vi.mocked(catalogFacetAPI.values).mockResolvedValue(response([{ ...option('A', 'Bad'), product_count: 0 }]))
    const { model, wrapper } = fixture(); await model.initialize()
    expect(model.states.country.error).toBe('invalidResponse')
    const delayed = deferred<ReturnType<typeof response>>()
    vi.mocked(catalogFacetAPI.values).mockReturnValue(delayed.promise)
    const pending = model.fetchOptions('country'); wrapper.unmount()
    delayed.resolve(response([option('LATE', '已离开')]))
    await pending
    expect(model.states.country.items).toEqual([])
  })
})

describe('optional API support and URL errors', () => {
  it('preserves legacy browsing on a genuine 404 while blocking unsupported selected URLs', async () => {
    vi.mocked(catalogFacetAPI.capabilities).mockRejectedValue(new ApiError('not found', 404, 404))
    const { model } = fixture(); await model.initialize()
    expect(model.capabilityError.value).toBeNull()
    expect(model.blocked.value).toBe(false)
    model.restore({ facet_country: 'US' })
    expect(model.blocked.value).toBe(true)
    model.clear(); expect(model.blocked.value).toBe(false)
  })
  it('reports capability failures separately, rejects malformed URLs and unsupported dimensions', async () => {
    vi.mocked(catalogFacetAPI.capabilities).mockRejectedValue(new ApiError('failed', 500, 500))
    const { model } = fixture(); model.restore({ facet_country: 'US' }); await model.initialize()
    expect(model.capabilityError.value).toBe('unavailable')
    expect(model.blocked.value).toBe(true)
    vi.mocked(catalogFacetAPI.capabilities).mockResolvedValue({ status_code: 0, msg: 'ok', data: caps(['city']) })
    await model.retryCapabilities()
    expect(model.capabilityError.value).toBeNull()
    expect(model.blocked.value).toBe(true)
    model.restore({ facet_city: ['A', 'B'] })
    expect(model.routeError.value).toBe(true)
    expect(model.blocked.value).toBe(true)
    model.clear(); expect(model.routeError.value).toBe(false)
  })
})
