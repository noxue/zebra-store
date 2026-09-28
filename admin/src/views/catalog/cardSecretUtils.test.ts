import { describe, expect, it } from 'vitest'
import { buildBatchLabel, buildProductLabel, buildSkuLabel, normalizeIds, parseSecretLines, resolveSecretBatchLabel, statusTone } from './cardSecretUtils'

describe('cardSecretUtils', () => {
  it('builds SKU labels from code and localized spec', () => {
    expect(buildSkuLabel({ id: 1, sku_code: 'PRO', spec_values: { 'zh-CN': '专业版', 'en-US': 'Pro' } }, 'en-US')).toBe('PRO · Pro')
    expect(buildSkuLabel({ id: 2, sku_code: '', spec_values: {} })).toBe('#2')
    expect(buildSkuLabel(null)).toBe('-')
  })

  it('builds product and batch labels', () => {
    expect(buildProductLabel({ id: 3, title: { 'zh-CN': '点卡' } })).toBe('#3 点卡')
    expect(buildProductLabel(null)).toBe('-')
    expect(buildBatchLabel({ id: 4, batch_no: ' B1 ' })).toBe('#4 B1')
    expect(resolveSecretBatchLabel({ batch_id: 9 })).toBe('#9')
    expect(resolveSecretBatchLabel({})).toBe('-')
  })

  it('parses pasted secrets and ids', () => {
    expect(parseSecretLines(' a \r\n\nb\n  ')).toEqual(['a', 'b'])
    expect(normalizeIds([3, '3', 0, -1, 2.7, 'x'])).toEqual([3, 2])
    expect(statusTone('reserved')).toBe('warning')
  })
})
