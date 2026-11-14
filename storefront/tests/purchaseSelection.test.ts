import { afterEach, describe, expect, it } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount, type VueWrapper } from '@vue/test-utils'
import { createPinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { Product, ProductSku } from '@/api/types'
import { useProductPurchase } from '@/composables/useProductPurchase'
import i18n from '@/i18n'
const wrappers: VueWrapper[] = []
afterEach(() => { wrappers.splice(0).forEach((w) => w.unmount()); localStorage.clear() })
const sku = (id: number): ProductSku => ({ id, sku_code: `P${id}`, price_amount: '8.00', is_active: true, auto_stock_available: 5 })
function fixture(skus: ProductSku[]) {
  const product = ref<Product>({ id: 1, slug: 'fixture', title: { 'zh-CN': 'Fixture' }, price_amount: '8.00', purchase_type: 'guest', fulfillment_type: 'auto', stock_display_mode: 'exact', stock_status: 'in_stock', skus })
  let engine: ReturnType<typeof useProductPurchase> | undefined
  const view = defineComponent({ setup() { engine = useProductPurchase(product); engine.syncSelectedSku(false); return () => h('div') } })
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: view }] })
  wrappers.push(mount(view, { global: { plugins: [createPinia(), i18n, router] } }))
  return { engine: engine!, product }
}
describe('purchase engine SKU selection', () => {
  it('requires an explicit active SKU and preserves the actual chosen variant', () => {
    const { engine } = fixture([sku(7), sku(9)])
    expect(engine.selectedSkuId.value).toBe(0)
    expect(engine.canPurchase.value).toBe(false)
    engine.selectSku(9)
    expect(engine.selectedSku.value?.id).toBe(9)
    expect(engine.canPurchase.value).toBe(true)
    engine.selectSku(999)
    expect(engine.selectedSkuId.value).toBe(0)
    expect(engine.canPurchase.value).toBe(false)
  })
  it('cannot purchase a malformed or ambiguous SKU through product-level fallback', () => {
    for (const variants of [[sku(Number.MAX_SAFE_INTEGER + 1)], [sku(0)], [sku(7), sku(7)]]) {
      const { engine } = fixture(variants)
      expect(engine.canPurchase.value).toBe(false)
      engine.selectSku(variants[0]!.id)
      expect(engine.selectedSkuId.value).toBe(0)
      expect(engine.canPurchase.value).toBe(false)
    }
  })
  it('keeps a sole valid SKU usable and clears a removed multi-SKU selection', () => {
    const only = fixture([sku(7)]).engine
    expect(only.selectedSkuId.value).toBe(7)
    expect(only.canPurchase.value).toBe(true)
    const { engine, product } = fixture([sku(7), sku(9)])
    engine.selectSku(9)
    product.value = { ...product.value, skus: [sku(7), sku(11)] }
    engine.syncSelectedSku(true)
    expect(engine.selectedSkuId.value).toBe(0)
    expect(engine.canPurchase.value).toBe(false)
  })
})
