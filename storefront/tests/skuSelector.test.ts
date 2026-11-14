import { afterEach, describe, expect, it } from 'vitest'
import { computed, nextTick, ref } from 'vue'
import { createPinia } from 'pinia'
import { mount, type VueWrapper } from '@vue/test-utils'
import type { ProductSku } from '@/api/types'
import { PriceBlock, SkuSelector, type PurchaseEngine } from '@/components/product/PurchaseParts'
import { initialSkuSelection } from '@/utils/skuChoices'
import { formatSkuSpecValues } from '@/utils/sku'
import i18n, { setI18nLocale } from '@/i18n'

const wrappers: VueWrapper[] = []
afterEach(async () => { wrappers.splice(0).forEach((wrapper) => wrapper.unmount()); await setI18nLocale('zh-CN') })
const sku = (id: number, spec_values: Record<string, unknown>, price = '8.00', stock = 3): ProductSku => ({ id, spec_values, sku_code: `PLAN-${id}`, price_amount: price, upstream_stock: stock, is_active: true })
function fixture(initial: ProductSku[]) {
  const rows = ref(initial)
  const selected = ref(0)
  const engine = {
    activeSkus: computed(() => rows.value),
    selectedSkuId: selected,
    requiresSkuSelection: computed(() => rows.value.filter((row) => row.is_active).length > 1 && selected.value === 0),
    priceDisplay: computed(() => ({price: rows.value.find((row) => row.id === selected.value)?.price_amount ?? '8.00', original: null, tag: null, isMember: false})),
    saveAmount: ref(null),
    skuPurchasable: (row: ProductSku) => row.upstream_stock !== 0,
    skuDisplayText: (row: ProductSku) => formatSkuSpecValues(row.spec_values, i18n.global.locale.value),
    skuStockTone: (row: ProductSku) => row.upstream_stock === 0 ? 'danger' : 'success',
    skuStockText: (row: ProductSku) => row.upstream_stock === 0 ? 'Sold out' : 'Available',
    selectSku: (id: number) => { selected.value = rows.value.some((row) => row.id === id && row.is_active) ? id : 0 },
    syncSelectedSku: (keep = true) => { selected.value = initialSkuSelection(rows.value, selected.value, keep) },
  } as unknown as PurchaseEngine
  const wrapper = mount(SkuSelector, { props: { engine }, global: { plugins: [createPinia(), i18n] } })
  wrappers.push(wrapper)
  return { wrapper, rows, selected, engine }
}
const option = (wrapper: VueWrapper, label: string) => wrapper.findAll('fieldset button').find((button) => button.text() === label)!

describe('guided SKU selector', () => {
  it('requires real sequential choices and clears later choices when the first changes', async () => {
    const { wrapper, selected } = fixture([sku(17, { period: 'Short', region: 'A' }), sku(29, { period: 'Long', region: 'A' }, '12.50'), sku(41, { period: 'Short', region: 'B' })])
    expect(selected.value).toBe(0)
    expect(wrapper.findAll('fieldset')).toHaveLength(2)
    expect(wrapper.text()).toContain('请先选择上一项规格')
    await option(wrapper, 'Long').trigger('click')
    expect(selected.value).toBe(0)
    expect(wrapper.findAll('fieldset')[1]!.text()).toContain('A')
    expect(wrapper.findAll('fieldset')[1]!.text()).not.toContain('B')
    await option(wrapper, 'A').trigger('click')
    expect(selected.value).toBe(29)
    expect(wrapper.text()).toContain('12.50')
    expect(option(wrapper, 'A').attributes('aria-pressed')).toBe('true')
    await option(wrapper, 'Short').trigger('click')
    expect(selected.value).toBe(0)
    expect(option(wrapper, 'A').attributes('aria-pressed')).toBe('false')
    await option(wrapper, 'B').trigger('click')
    expect(selected.value).toBe(41)
  })
  it('withholds the final price until a package is explicitly chosen', async () => {
    const { wrapper, engine } = fixture([sku(1, { period: 'Short' }, '8.00'), sku(2, { period: 'Long' }, '25.00')])
    const price = mount(PriceBlock, { props: { engine }, global: { plugins: [createPinia(), i18n] } })
    wrappers.push(price)
    expect(price.text()).toContain('选择套餐后显示价格')
    expect(price.text()).not.toContain('8.00')
    await option(wrapper, 'Long').trigger('click')
    expect(price.text()).toContain('25.00')
    expect(price.text()).not.toContain('选择套餐后显示价格')
  })
  it('shows a sold-out combination as disabled without selecting a different package', async () => {
    const { wrapper, selected } = fixture([sku(1, { period: 'Short' }), sku(2, { period: 'Long' }, '1.00', 0)])
    const unavailable = wrapper.findAll('fieldset button').find((button) => button.text().startsWith('Long'))!
    expect(unavailable.attributes('disabled')).toBeDefined()
    expect(selected.value).toBe(0)
    await option(wrapper, 'Short').trigger('click')
    expect(selected.value).toBe(1)
  })
  it('disambiguates duplicate specs using actual package cards and codes', async () => {
    const { wrapper, selected } = fixture([sku(1, { period: '30天' }, '20.00'), sku(2, { period: '30天' }, '5.00')])
    expect(wrapper.findAll('fieldset')).toHaveLength(0)
    expect(wrapper.text()).toContain('PLAN-1')
    expect(wrapper.text()).toContain('PLAN-2')
    expect(selected.value).toBe(0)
    const card = wrapper.findAll('button').find((button) => button.text().includes('PLAN-1'))!
    await card.trigger('click')
    expect(selected.value).toBe(1)
  })
  it('preserves a selection across language changes and clears changed specs or removed SKUs', async () => {
    const { wrapper, rows, selected } = fixture([sku(1, { 'zh-CN': '短期', 'en-US': 'Short' }), sku(2, { 'zh-CN': '长期', 'en-US': 'Long' })])
    await option(wrapper, '长期').trigger('click')
    expect(selected.value).toBe(2)
    await setI18nLocale('en-US'); await nextTick()
    expect(selected.value).toBe(2)
    expect(option(wrapper, 'Long').attributes('aria-pressed')).toBe('true')
    rows.value = [sku(1, { period: 'A' }), sku(2, { period: 'Changed' })]
    await nextTick(); expect(selected.value).toBe(0)
    await option(wrapper, 'Changed').trigger('click'); expect(selected.value).toBe(2)
    rows.value = [sku(1, { period: 'A' })]
    await nextTick(); expect(selected.value).toBe(1)
  })
})
