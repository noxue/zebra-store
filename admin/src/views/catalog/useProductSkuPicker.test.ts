import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useProductSkuPicker } from './useProductSkuPicker'
const getProducts = vi.hoisted(() => vi.fn())
vi.mock('@/api/admin', () => ({ adminAPI: { getProducts } }))
describe('card import product picker', () => {
  beforeEach(() => vi.resetAllMocks())
  it('includes supplier products for local cards and excludes manual-only products', async () => {
    getProducts.mockResolvedValue({ data: [
      { id: 1, title: { 'zh-CN': '本地商品' }, fulfillment_type: 'auto' },
      { id: 2, title: { 'zh-CN': '上游商品' }, fulfillment_type: 'upstream' },
      { id: 3, title: { 'zh-CN': '人工商品' }, fulfillment_type: 'manual' },
    ] })
    const picker = useProductSkuPicker()
    await picker.loadOptions()
    expect(picker.options.value.map((item) => item.id)).toEqual([1, 2])
    expect(getProducts.mock.calls[0]![0]).not.toHaveProperty('fulfillment_type')
  })
})
