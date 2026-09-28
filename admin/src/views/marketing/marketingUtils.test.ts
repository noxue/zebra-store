import { describe, expect, it } from 'vitest'
import type { AdminProduct } from '@/api/types'
import {
  buildProductLabel,
  ensureProductsInOptions,
  fixedDiscountRisk,
  formatRedeemedUser,
  giftCardDisplayStatus,
  normalizeMemberLevels,
  normalizePaymentRoles,
  normalizeScopeIDs,
  normalizeWholesaleTiers,
  referenceUnitPrice,
  validateGiftCardGenerate,
} from './marketingUtils'

const product = (id: number, extra: Partial<AdminProduct> = {}) => ({ id, title: { 'zh-CN': `P${id}` }, price_amount: '10.00', ...extra }) as unknown as AdminProduct

describe('coupon normalizers', () => {
  it('parses scope ids from arrays, JSON strings and comma lists', () => {
    expect(normalizeScopeIDs([3, '1', 1, -2, 'x'])).toEqual([3, 1])
    expect(normalizeScopeIDs('[1,2]')).toEqual([1, 2])
    expect(normalizeScopeIDs('4, 5，6 6')).toEqual([4, 5, 6])
    expect(normalizeScopeIDs('')).toEqual([])
    expect(normalizeScopeIDs(null)).toEqual([])
  })
  it('keeps only known payment roles and positive member levels', () => {
    expect(normalizePaymentRoles(['Guest', 'member', 'admin', 'guest'])).toEqual(['guest', 'member'])
    expect(normalizePaymentRoles('guest')).toEqual([])
    expect(normalizeMemberLevels([2, '2', 0, 3.7])).toEqual([2, 3])
  })
})

describe('product options', () => {
  it('builds labels and fills missing referenced ids', () => {
    expect(buildProductLabel(product(1))).toBe('#1 P1')
    const rows = ensureProductsInOptions([product(1)], [1, 9])
    expect(rows.map((r) => r.id)).toEqual([9, 1])
    expect(buildProductLabel(rows[0]!)).toBe('#9 #9')
  })
})

describe('gift cards', () => {
  it('derives display status and redeemed user text', () => {
    expect(giftCardDisplayStatus({ status: 'active', is_expired: true })).toBe('expired')
    expect(giftCardDisplayStatus({ status: 'Redeemed' })).toBe('redeemed')
    expect(formatRedeemedUser({ id: 3, display_name: 'A', email: 'a@x' })).toBe('#3 A (a@x)')
    expect(formatRedeemedUser({ id: 3, email: 'a@x' })).toBe('#3 a@x')
    expect(formatRedeemedUser(undefined)).toBe('-')
  })
  it('validates the generate form in the original order', () => {
    expect(validateGiftCardGenerate({ name: '', quantity: 0, amount: '' })).toEqual({ ok: false, error: 'invalidQuantity' })
    expect(validateGiftCardGenerate({ name: '', quantity: 2, amount: '0' })).toEqual({ ok: false, error: 'invalidAmount' })
    expect(validateGiftCardGenerate({ name: ' ', quantity: 2, amount: '5' })).toEqual({ ok: false, error: 'nameRequired' })
    expect(validateGiftCardGenerate({ name: 'x', quantity: 2.9, amount: ' 5.5 ' })).toEqual({ ok: true, quantity: 2, amount: '5.5' })
  })
})

describe('promotion risk', () => {
  it('uses the lowest active sku price as reference', () => {
    const p = product(1, { skus: [{ price_amount: '8', is_active: true }, { price_amount: '3', is_active: false }] as unknown as AdminProduct['skus'] })
    expect(referenceUnitPrice(p)).toBe(8)
    expect(referenceUnitPrice(product(2))).toBe(10)
    expect(fixedDiscountRisk('fixed', 8, 8)).toEqual({ discountValue: '8.00', referencePrice: '8.00' })
    expect(fixedDiscountRisk('fixed', 7, 8)).toBeNull()
    expect(fixedDiscountRisk('percent', 80, 8)).toBeNull()
  })
})

describe('wholesale tiers', () => {
  const codeOf = (id: number) => (id === 5 ? 'SKU5' : '')
  it('validates, resolves scope and sorts', () => {
    const r = normalizeWholesaleTiers(
      [
        { sku_id: 'id:5', sku_code: '', min_quantity: 10, unit_price: 8 },
        { sku_id: 'all', sku_code: '', min_quantity: 5, unit_price: '9.5' },
        { sku_id: 'all', sku_code: '', min_quantity: 2, unit_price: 9.8 },
      ],
      codeOf,
    )
    expect(r).toEqual({
      ok: true,
      tiers: [
        { sku_id: undefined, sku_code: undefined, min_quantity: 2, unit_price: 9.8 },
        { sku_id: undefined, sku_code: undefined, min_quantity: 5, unit_price: 9.5 },
        { sku_id: 5, sku_code: 'SKU5', min_quantity: 10, unit_price: 8 },
      ],
    })
  })
  it('reports invalid and duplicate tiers', () => {
    expect(normalizeWholesaleTiers([{ sku_id: 'all', sku_code: '', min_quantity: '', unit_price: 1 }], codeOf)).toEqual({ ok: false, error: { kind: 'invalidTier', index: 1 } })
    expect(
      normalizeWholesaleTiers(
        [
          { sku_id: 'code:A', sku_code: '', min_quantity: 3, unit_price: 1 },
          { sku_id: 'code:A', sku_code: '', min_quantity: 3, unit_price: 2 },
        ],
        codeOf,
      ),
    ).toEqual({ ok: false, error: { kind: 'duplicateQuantity', skuId: 'code:A', quantity: 3 } })
  })
})
