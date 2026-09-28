import { describe, expect, it } from 'vitest'
import type { AdminProduct } from '@/api/types'
import {
  buildProductPayload,
  createManualFormField,
  createSKUFormItem,
  emptyProductForm,
  normalizeManualFormSchemaForSubmit,
  parseManualFormSchemaForEdit,
  parsePaymentChannelIDs,
  parseWholesaleQuery,
  productToForm,
  resolveManualStockMetrics,
  statusQueryValue,
  toSafeStockTotal,
  wholesaleQueryValue,
} from './productUtils'

const t = (key: string, params?: Record<string, unknown>) => (params ? `${key}:${JSON.stringify(params)}` : key)
const opts = { isCategorySelectable: () => true, messages: { categoryRequired: 'need-category', categoryLeaf: 'leaf-only' } }

describe('stock helpers', () => {
  it('keeps -1 as unlimited and clamps negatives', () => {
    expect(toSafeStockTotal(-1)).toBe(-1)
    expect(toSafeStockTotal(-5)).toBe(0)
    expect(toSafeStockTotal('12.7')).toBe(12)
    expect(toSafeStockTotal('x')).toBe(0)
  })

  it('aggregates active SKU stock; any unlimited SKU makes total unlimited', () => {
    const base = { manual_stock_total: 9, manual_stock_locked: 1, manual_stock_sold: 2 }
    expect(resolveManualStockMetrics({ ...base, skus: [] })).toEqual({ total: 9, locked: 1, sold: 2 })
    const sku = (total: number, active = true) => ({ is_active: active, manual_stock_total: total, manual_stock_locked: 1, manual_stock_sold: 3 })
    expect(resolveManualStockMetrics({ ...base, skus: [sku(5), sku(7), sku(100, false)] })).toEqual({ total: 12, locked: 2, sold: 6 })
    expect(resolveManualStockMetrics({ ...base, skus: [sku(5), sku(-1)] }).total).toBe(-1)
  })
})

describe('query mapping', () => {
  it('parses ?wholesale= aliases', () => {
    expect(parseWholesaleQuery('1')).toBe('enabled')
    expect(parseWholesaleQuery(['yes'])).toBe('enabled')
    expect(parseWholesaleQuery('disabled')).toBe('disabled')
    expect(parseWholesaleQuery(undefined)).toBe('all')
    expect(wholesaleQueryValue('enabled')).toBe('1')
    expect(wholesaleQueryValue('all')).toBeUndefined()
    expect(statusQueryValue('inactive')).toBe('0')
    expect(statusQueryValue('all')).toBeUndefined()
  })
})

describe('edit form mapping', () => {
  it('parses payment channel ids from JSON string or array', () => {
    expect(parsePaymentChannelIDs('[1,2,"x",0]')).toEqual([1, 2])
    expect(parsePaymentChannelIDs('')).toEqual([])
    expect(parsePaymentChannelIDs([3, -1])).toEqual([3])
  })

  it('round-trips the manual form schema', () => {
    const schema = {
      fields: [
        { key: 'qq', type: 'text', required: true, label: { 'zh-CN': 'QQ号' }, regex: '^\\d+$', max_len: 12 },
        { key: 'plan', type: 'select', options: ['A', 'B'] },
        { key: '', type: 'text' },
      ],
    }
    const parsed = parseManualFormSchemaForEdit(schema)
    expect(parsed.fields[0]!.label).toEqual({ 'zh-CN': 'QQ号', 'zh-TW': '', 'en-US': '' })
    expect(parsed.fields[1]!.options_text).toBe('A\nB')
    parsed.fields[1]!.options_text = 'A, B\nB\nC'
    const out = normalizeManualFormSchemaForSubmit('manual', parsed.fields)
    expect(out.fields).toEqual([
      { key: 'qq', type: 'text', required: true, label: { 'zh-CN': 'QQ号' }, regex: '^\\d+$', max_len: 12 },
      { key: 'plan', type: 'select', required: false, options: ['A', 'B', 'C'] },
    ])
    expect(normalizeManualFormSchemaForSubmit('auto', parsed.fields)).toEqual({ fields: [] })
    const numberField = { ...createManualFormField(), key: 'n', type: 'number', min: '0', max: '' }
    expect(normalizeManualFormSchemaForSubmit('manual', [numberField]).fields[0]).toEqual({ key: 'n', type: 'number', required: false, min: 0 })
  })

  it('maps an API product into the form', () => {
    const form = productToForm({
      id: 3,
      category_id: 2,
      slug: 's',
      title: { 'zh-CN': '标题' },
      seo_meta: { keywords: 'kw' } as unknown as AdminProduct['seo_meta'],
      price_amount: '12.50',
      min_purchase_quantity: 0,
      max_purchase_quantity: 5,
      payment_channel_ids: '[4]',
      skus: [{ id: 9, sku_code: ' A ', spec_values: { 'zh-CN': '标准' }, price_amount: '10.00', is_active: true }],
    } as unknown as AdminProduct)
    expect(form.title).toEqual({ 'zh-CN': '标题', 'zh-TW': '', 'en-US': '' })
    expect(form.seo_meta.keywords['zh-CN']).toBe('kw')
    expect(form.price_amount).toBe(12.5)
    expect(form.min_purchase_quantity).toBe('')
    expect(form.max_purchase_quantity).toBe(5)
    expect(form.payment_channel_ids).toEqual([4])
    expect(form.skus[0]).toMatchObject({ id: 9, sku_code: 'A', price_amount: 10, spec_values: { 'zh-CN': '标准', 'zh-TW': '', 'en-US': '' } })
  })
})

describe('buildProductPayload', () => {
  const baseForm = () => ({ ...emptyProductForm(), slug: ' demo ', category_id: 5, price_amount: 9.9 as number | '' })

  it('requires a category', () => {
    expect(() => buildProductPayload({ ...baseForm(), category_id: null }, t, opts)).toThrow('need-category')
    expect(() => buildProductPayload(baseForm(), t, { ...opts, isCategorySelectable: () => false })).toThrow('leaf-only')
  })

  it('builds a single-spec payload with numeric money like the original', () => {
    const payload = buildProductPayload({ ...baseForm(), min_purchase_quantity: 2, max_purchase_quantity: '', manual_stock_total: -1 }, t, opts)
    expect(payload).toMatchObject({
      slug: 'demo',
      category_id: 5,
      price_amount: 9.9,
      cost_price_amount: 0,
      min_purchase_quantity: 2,
      max_purchase_quantity: 0,
      manual_stock_total: -1,
      skus: [],
      payment_channel_ids: [],
      manual_form_schema: { fields: [] },
    })
  })

  it('takes price from the first active SKU and sums manual stock', () => {
    const skus = [
      { ...createSKUFormItem({ sku_code: 'A', price_amount: 20, manual_stock_total: 3 }), is_active: false },
      createSKUFormItem({ id: 7, sku_code: 'B', price_amount: 15, cost_price_amount: 5, manual_stock_total: 4 }),
      createSKUFormItem({ sku_code: 'C', price_amount: 30, manual_stock_total: 6 }),
    ]
    const payload = buildProductPayload({ ...baseForm(), skus }, t, opts)
    expect(payload.price_amount).toBe(15)
    expect(payload.cost_price_amount).toBe(5)
    expect(payload.manual_stock_total).toBe(10)
    expect(payload.skus[1]).toEqual({ id: 7, sku_code: 'B', spec_values: {}, price_amount: 15, cost_price_amount: 5, manual_stock_total: 4, is_active: true, sort_order: 0 })
    expect(payload.skus[0]!.id).toBeUndefined()
  })

  it('validates SKUs and purchase limits', () => {
    const f = baseForm()
    expect(() => buildProductPayload({ ...f, skus: [createSKUFormItem({ sku_code: '', price_amount: 1 })] }, t, opts)).toThrow('skuCodeRequired')
    expect(() => buildProductPayload({ ...f, skus: [createSKUFormItem({ sku_code: 'a', price_amount: 1 }), createSKUFormItem({ sku_code: 'A', price_amount: 1 })] }, t, opts)).toThrow(
      'skuCodeDuplicate',
    )
    expect(() => buildProductPayload({ ...f, skus: [createSKUFormItem({ sku_code: 'a', price_amount: 0 })] }, t, opts)).toThrow('skuPriceInvalid')
    expect(() => buildProductPayload({ ...f, skus: [{ ...createSKUFormItem({ sku_code: 'a', price_amount: 1 }), is_active: false }] }, t, opts)).toThrow('skuNeedActive')
    expect(() => buildProductPayload({ ...f, min_purchase_quantity: 5, max_purchase_quantity: 2 }, t, opts)).toThrow('purchaseLimitInvalid')
  })

  it('zeroes SKU manual stock for auto fulfilment', () => {
    const payload = buildProductPayload({ ...baseForm(), fulfillment_type: 'auto', skus: [createSKUFormItem({ sku_code: 'A', price_amount: 1, manual_stock_total: 8 })] }, t, opts)
    expect(payload.skus[0]!.manual_stock_total).toBe(0)
  })
})
