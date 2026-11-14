import { describe, expect, it } from 'vitest'
import type { ProductSku } from '@/api/types'
import { buildSkuChoices, initialSkuSelection, resolveSkuChoice, skuChoiceRows } from '@/utils/skuChoices'
const sku = (id: number, spec_values: Record<string, unknown>, extra: Partial<ProductSku> = {}): ProductSku => ({ id, spec_values, sku_code: `P${id}`, price_amount: '8.00', is_active: true, ...extra })

describe('actual SKU combinations', () => {
  it('offers only existing combinations and resolves the original SKU object', () => {
    const rows = [sku(17, { region: 'A', period: '2–10 days' }), sku(29, { region: 'A', period: '11–30 days' }), sku(41, { region: 'B', period: '2–10 days' }, { price_amount: '19.00', upstream_stock: 0 })]
    const model = buildSkuChoices(rows, 'en-US')!
    expect(model.dimensions).toEqual(['period', 'region'])
    expect(skuChoiceRows(model, 1, ['"11–30 days"']).map((row) => row.sku.id)).toEqual([29])
    expect(resolveSkuChoice(model, ['"11–30 days"', '"B"'])).toBeNull()
    expect(resolveSkuChoice(model, ['"2–10 days"', '"B"'])).toBe(rows[2])
    expect(resolveSkuChoice(model, ['"2–10 days"'])).toBeNull()
    expect(resolveSkuChoice(model, ['"2–10 days"', '"B"', 'extra'])).toBeNull()
    expect(skuChoiceRows(model, -1, [])).toEqual([])
  })
  it('preserves interval wording and omits common attributes from the choice sequence', () => {
    const model = buildSkuChoices([sku(1, { 周期: '2-10天', 当前用途: '新购或续费' }), sku(2, { 周期: '11-30天', 当前用途: '新购或续费' })], 'zh-CN')!
    expect(model.dimensions).toEqual(['周期'])
    expect(model.values[0]?.map((v) => v.label)).toEqual(['2-10天', '11-30天'])
    expect(resolveSkuChoice(model, ['"11-30天"'])?.id).toBe(2)
  })
  it('PRV-08 treats translated objects as one value with locale-independent selection', () => {
    const rows = [sku(1, { 'zh-CN': '短期', 'en-US': 'Short' }), sku(2, { 'en-US': 'Long', 'zh-CN': '长期' })]
    const cn = buildSkuChoices(rows, 'zh-CN')!
    const en = buildSkuChoices(rows, 'en-US')!
    expect(cn.dimensions).toEqual([''])
    expect(cn.values[0]?.map((v) => v.label)).toEqual(['短期', '长期'])
    expect(en.values[0]?.map((v) => v.label)).toEqual(['Short', 'Long'])
    expect(cn.rows.map((r) => r.values)).toEqual(en.rows.map((r) => r.values))
    expect(resolveSkuChoice(en, cn.rows[1]!.values)?.id).toBe(2)
  })
  it('falls back for missing, duplicated, inconsistent and ambiguous specifications', () => {
    for (const rows of [
      [sku(1, {}), sku(2, {})],
      [sku(1, { period: '30 days' }), sku(2, { period: '30 days' }, { price_amount: '2.00' })],
      [sku(1, { period: '30 days' }), sku(2, { region: 'A' })],
      [sku(1, { period: ' ' }), sku(2, { period: '30 days' })],
      [sku(1, { period: 30 }), sku(2, { period: '30' })],
      [sku(1, { period: { arbitrary: 'nested' } }), sku(2, { period: '30 days' })],
      [sku(1, { period: 'A' }), sku(1, { period: 'B' })],
      [sku(Number.MAX_SAFE_INTEGER + 1, { period: 'A' }), sku(2, { period: 'B' })],
    ]) expect(buildSkuChoices(rows, 'en-US')).toBeNull()
  })
  it('keeps nested translated values atomic and excludes inactive variants', () => {
    const model = buildSkuChoices([sku(1, { period: { 'zh-CN': '短期', 'en-US': 'Short' } }), sku(2, { period: { 'zh-CN': '长期', 'en-US': 'Long' } }), sku(3, {}, { is_active: false })], 'en-US')!
    expect(model.dimensions).toEqual(['period'])
    expect(model.values[0]?.map((v) => v.label)).toEqual(['Short', 'Long'])
    expect(model.rows.map((r) => r.sku.id)).toEqual([1, 2])
  })
  it('requires explicit selection among multiple SKUs, including after a removal', () => {
    const rows = [sku(7, { period: 'A' }, { price_amount: '1.00' }), sku(9, { period: 'B' }, { price_amount: '30.00' })]
    expect(initialSkuSelection(rows, 0, false)).toBe(0)
    expect(initialSkuSelection(rows, 9, true)).toBe(9)
    expect(initialSkuSelection(rows, 6, true)).toBe(0)
    expect(initialSkuSelection(rows, 9, false)).toBe(0)
    expect(initialSkuSelection([rows[0]!], 0, false)).toBe(7)
    expect(initialSkuSelection([], 7, true)).toBe(0)
    expect(initialSkuSelection([sku(0, {period:'A'})], 0, false)).toBe(0)
    expect(initialSkuSelection([sku(Number.MAX_SAFE_INTEGER + 1, {period:'A'})], 0, false)).toBe(0)
  })
})
