import { describe, expect, it } from 'vitest'
import type { AdminSiteConnection } from '@/api/types'
import {
  buildCategoryDisplayList,
  buildSiteConnectionPayload,
  canCancelProcurement,
  canReapplyMarkup,
  canRetryProcurement,
  chunk,
  emptySiteConnectionForm,
  groupProductsByCategory,
  localCostAmount,
  normalizeUpstreamRefundRecords,
  parseProcurementStats,
  parseUpstreamResult,
  PROCUREMENT_STATUSES,
  priceRange,
  procurementProfit,
  procurementStatusTone,
  relativeTimeParts,
  retryIntervalsToText,
  siteConnectionToForm,
  skuStockLevel,
  validateProtocolConfig,
  type UpstreamCategory,
  type UpstreamProduct,
} from './integrationUtils'

describe('site connection form', () => {
  it('builds the payload with numeric fields and JSON-string intervals', () => {
    const form = {
      ...emptySiteConnectionForm('dujiao-next'),
      name: 'up',
      config: { base_url: 'http://x', api_key: 'k' },
      retry_intervals: ' 10, abc,0, 20 ',
      exchange_rate: '' as const,
      auto_sync_price: 'true' as const,
    }
    const payload = buildSiteConnectionPayload(form)
    expect(payload.base_url).toBe('http://x')
    expect(payload.api_key).toBe('k')
    expect(payload.api_secret).toBe('')
    expect(payload.config).toEqual({ base_url: 'http://x', api_key: 'k' })
    expect(payload.extra).toEqual({})
    expect(payload.retry_intervals).toBe('[10,20]')
    expect(payload.exchange_rate).toBe(1)
    expect(payload.retry_max).toBe(3)
    expect(payload.price_markup_percent).toBe(0)
    expect(payload.auto_sync_price).toBe(true)
  })

  it('sends adapter-specific fields as extra and reads them back (acg-faka currency)', () => {
    const def = {
      fields: ['base_url', 'api_key', 'api_secret', 'currency'].map((key) => ({ key, label: {}, kind: 'text' as const, required: false })),
    }
    const form = { ...emptySiteConnectionForm('acg-faka'), config: { base_url: 'https://acg.example', api_key: '1024', api_secret: 'K', currency: 'USD', stale: 'x' } }
    const payload = buildSiteConnectionPayload(form, def)
    expect(payload.api_key).toBe('1024')
    expect(payload.extra).toEqual({ currency: 'USD' })
    const conn = { id: 2, name: 'acg', protocol: 'acg-faka', base_url: 'https://acg.example', api_key: '1024', extra: { currency: 'USD' } } as unknown as AdminSiteConnection
    expect(siteConnectionToForm(conn).config).toEqual({ currency: 'USD', base_url: 'https://acg.example', api_key: '1024', api_secret: '' })
  })

  it('keeps masked secret extra fields blank on edit (the backend keeps the stored value)', () => {
    const def = {
      fields: [
        { key: 'base_url', label: {}, kind: 'url' as const, required: true },
        { key: 'api_secret', label: {}, kind: 'secret' as const, required: true },
        { key: 'token', label: {}, kind: 'secret' as const, required: true },
        { key: 'tenant', label: {}, kind: 'text' as const, required: false },
      ],
    }
    // GET masks secret configuration values as ""
    const conn = { id: 3, name: 'acme', protocol: 'acme', base_url: 'https://acme.example', api_key: 'k', extra: { tenant: 'eu', token: '' } } as unknown as AdminSiteConnection
    const form = siteConnectionToForm(conn)
    expect(form.config.token).toBe('')
    const messages = { required: 'required', url: 'url' }
    // editing: an empty secret means "unchanged" and is valid; creating requires it
    expect(validateProtocolConfig(def, form.config, messages, { editing: true })).toEqual({})
    expect(validateProtocolConfig(def, form.config, messages)).toEqual({ api_secret: 'required', token: 'required' })
    const payload = buildSiteConnectionPayload(form, def)
    expect(payload.extra).toEqual({ token: '', tenant: 'eu' })
    expect(payload.api_secret).toBe('')
  })

  it('reads retry intervals from array, JSON string or plain text', () => {
    expect(retryIntervalsToText([1, 2])).toBe('1,2')
    expect(retryIntervalsToText('[30,60]')).toBe('30,60')
    expect(retryIntervalsToText('5,6')).toBe('5,6')
    expect(retryIntervalsToText(undefined)).toBe('30,60,120')
  })

  it('maps a connection into the form (string exchange rate → number)', () => {
    const conn = { id: 1, name: 'a', base_url: 'u', api_key: 'k', exchange_rate: '7.2', auto_sync_price: true, retry_intervals: '[5]' } as unknown as AdminSiteConnection
    const form = siteConnectionToForm(conn)
    expect(form.config).toEqual({ base_url: 'u', api_key: 'k', api_secret: '' })
    expect(form.protocol).toBe('dujiao-next')
    expect(form.exchange_rate).toBe(7.2)
    expect(form.auto_sync_price).toBe('true')
    expect(form.retry_intervals).toBe('5')
  })

  it('offers markup re-application only with a markup or non-1 exchange rate', () => {
    expect(canReapplyMarkup({ price_markup_percent: 0, exchange_rate: '1.00' })).toBe(false)
    expect(canReapplyMarkup({ price_markup_percent: 5 })).toBe(true)
    expect(canReapplyMarkup({ price_markup_percent: 0, exchange_rate: '7' })).toBe(true)
  })
})

describe('upstream import helpers', () => {
  const p = (id: number, category_id?: number, prices: string[] = []): UpstreamProduct => ({
    id,
    title: { 'zh-CN': `p${id}` },
    price_amount: '9.00',
    is_active: true,
    category_id,
    skus: prices.map((price, i) => ({ id: i, price_amount: price, stock_status: i === 0 ? 'in_stock' : 'out_of_stock', is_active: true })),
  })

  it('computes price ranges like the original', () => {
    expect(priceRange(['1.00', '3.50', '0'])).toBe('1 ~ 3.5')
    expect(priceRange(['2.00', '2'])).toBe('2')
    expect(priceRange([], '9.00')).toBe('9.00')
    expect(priceRange([])).toBe('-')
  })

  it('parses both upstream result shapes', () => {
    expect(parseUpstreamResult([p(1)]).total).toBe(1)
    const r = parseUpstreamResult({ items: [p(1), p(2)], total: 10, mapped_ids: [2] })
    expect(r.items).toHaveLength(2)
    expect(r.total).toBe(10)
    expect(r.mappedIds).toEqual([2])
    expect(parseUpstreamResult(null)).toEqual({ items: [], total: 0 })
  })

  it('summarises SKU stock', () => {
    expect(skuStockLevel(p(1)).level).toBe('none')
    expect(skuStockLevel(p(1, 0, ['1'])).level).toBe('all')
    expect(skuStockLevel(p(1, 0, ['1', '2']))).toEqual({ level: 'partial', inStock: 1, total: 2 })
  })

  it('groups by category, skips empty ones and appends uncategorized', () => {
    const cats: UpstreamCategory[] = [
      { id: 1, parent_id: 0, slug: 'a', name: { 'zh-CN': 'A' }, icon: '', sort_order: 0 },
      { id: 2, parent_id: 1, slug: 'b', name: { 'zh-CN': 'B' }, icon: '', sort_order: 0 },
      { id: 3, parent_id: 0, slug: 'c', name: { 'zh-CN': 'C' }, icon: '', sort_order: 0 },
    ]
    const products = [p(1, 2), p(2, 2), p(3)]
    const list = buildCategoryDisplayList(cats, groupProductsByCategory(products), new Set([1]), (n) => n['zh-CN'] ?? '', 'none')
    expect(list.map((x) => [x.category.id, x.path, x.productCount, x.nonMappedCount])).toEqual([
      [2, 'A / B', 2, 1],
      [0, 'none', 1, 1],
    ])
  })

  it('chunks arrays', () => {
    expect(chunk([1, 2, 3, 4, 5], 3)).toEqual([[1, 2, 3], [4, 5]])
  })
})

describe('procurement helpers', () => {
  it('computes local cost and profit', () => {
    const order = {
      local_sell_amount: '100.00',
      upstream_refunded_amount: '10',
      connection: { exchange_rate: 2 },
      local_order: { status: 'paid', refunded_amount: '5.00', items: [{ title: {}, quantity: 2, cost_price: '30', total_amount: '100' }] },
    }
    expect(localCostAmount(order)).toBe('60.00')
    // 100 + 10*2 - 60 - 5
    expect(procurementProfit(order)).toBe('55.00')
    expect(procurementProfit({ local_sell_amount: '0.00' })).toBeNull()
  })

  it('parses stats with an "other" remainder', () => {
    expect(parseProcurementStats({ total: 10, by_status: { pending: 2, failed: 1, fulfilled: 3 } })).toEqual({
      total: 10,
      pending: 2,
      failed: 1,
      rejected: 0,
      fulfilled: 3,
      other: 4,
    })
    expect(parseProcurementStats(undefined).total).toBe(0)
  })

  it('surfaces manual review with retry and cancel actions', () => {
    expect(PROCUREMENT_STATUSES).toContain('manual_review')
    expect(procurementStatusTone('manual_review')).toBe('warning')
    expect(canRetryProcurement('manual_review')).toBe(true)
    expect(canCancelProcurement('manual_review')).toBe(true)
    expect(canRetryProcurement('accepted')).toBe(false)
    expect(canCancelProcurement('fulfilled')).toBe(false)
  })

  it('normalizes refund records', () => {
    expect(normalizeUpstreamRefundRecords([{ type: ' manual ', amount: 1.5, created_at: 'x' }, null])).toEqual([
      { id: '1', type: 'manual', amount: '1.5', currency: '', remark: '', createdAt: 'x' },
      { id: '2', type: '', amount: '', currency: '', remark: '', createdAt: '' },
    ])
    expect(normalizeUpstreamRefundRecords(undefined)).toEqual([])
  })

  it('buckets relative time', () => {
    const now = Date.parse('2026-01-02T00:00:00Z')
    expect(relativeTimeParts('2026-01-01T23:59:30Z', now)?.key).toBe('justNow')
    expect(relativeTimeParts('2026-01-01T23:30:00Z', now)).toEqual({ key: 'minutesAgo', n: 30 })
    expect(relativeTimeParts('2026-01-01T20:00:00Z', now)).toEqual({ key: 'hoursAgo', n: 4 })
    expect(relativeTimeParts('2025-12-30T00:00:00Z', now)).toEqual({ key: 'daysAgo', n: 3 })
    expect(relativeTimeParts('bad', now)).toBeNull()
  })
})
