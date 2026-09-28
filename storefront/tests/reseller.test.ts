import { describe, expect, it } from 'vitest'
import type { ResellerDomainData } from '@/api/types'
import { buildApplySteps } from '@/composables/reseller/useResellerApply'
import { isLikelyDomain } from '@/composables/reseller/useResellerDomains'
import { buildResellerLedgerParams } from '@/composables/reseller/useResellerLedger'
import { buildResellerOrderParams } from '@/composables/reseller/useResellerOrderList'
import { buildResellerProductSettingListParams } from '@/composables/reseller/useResellerProductSettings'
import { buildUrl } from '@/api/client'
import { validateWithdrawForm } from '@/composables/reseller/useResellerWithdraws'
import {
  canRenderResellerConsoleModule,
  formatResellerConsoleAmount,
  getResellerConsoleState,
  pickPrimaryDomain,
  resolveResellerConsoleModule,
} from '@/utils/reseller/console'
import { exceedsAvailable, getResellerFinanceStatusView, ledgerAmountDisplay, pickPrimaryResellerBalance } from '@/utils/reseller/finance'
import { getResellerManagementState } from '@/utils/reseller/management'
import {
  buildResellerProductSettingPayload,
  countListedSkus,
  estimateResellerPrice,
  normalizeResellerProductSettingsPagination,
  summarizeProductEffectivePrice,
} from '@/utils/reseller/productSettings'
import { siteConfigFormToPayload, siteConfigToForm } from '@/utils/reseller/siteConfig'

const domain = (over: Partial<ResellerDomainData>): ResellerDomainData => ({
  id: 1,
  domain: 'a.example.com',
  type: 'custom',
  verification_status: 'verified',
  status: 'active',
  is_primary: false,
  created_at: '',
  updated_at: '',
  ...over,
})

describe('console state', () => {
  it('not opened snapshot disables modules but keeps apply', () => {
    const state = getResellerConsoleState({ opened: false, can_apply: false, domains: [] })
    expect(state.profileStatus).toBe('not_opened')
    expect(state.modules.apply.enabled).toBe(true)
    expect(state.modules.orders.enabled).toBe(false)
    expect(canRenderResellerConsoleModule('/reseller/orders', state)).toBe(false)
    expect(canRenderResellerConsoleModule('/reseller', state)).toBe(true)
    expect(canRenderResellerConsoleModule('/reseller/apply', state)).toBe(true)
  })

  it('active profile unlocks every module', () => {
    const state = getResellerConsoleState({
      opened: true,
      can_apply: false,
      domains: [],
      profile: { id: 1, status: 'active', default_markup_percent: '10', max_markup_percent: '0', settlement_status: 'normal', created_at: '', updated_at: '' },
    })
    expect(state.modules.withdraws.enabled).toBe(true)
    expect(state.modules.apply.enabled).toBe(false)
    expect(canRenderResellerConsoleModule('/reseller/orders/DJ1', state)).toBe(true)
  })

  it('resolves module from path', () => {
    expect(resolveResellerConsoleModule('/reseller/')).toBe('dashboard')
    expect(resolveResellerConsoleModule('/reseller/orders/X?y=1')).toBe('orders')
    expect(resolveResellerConsoleModule('/reseller/unknown')).toBe('dashboard')
  })

  it('management state', () => {
    expect(getResellerManagementState({ opened: false, can_apply: true }).statusKey).toBe('notOpened')
    expect(getResellerManagementState({ opened: true, profile: { status: 'rejected' } }).statusKey).toBe('rejected')
  })

  it('apply stepper states', () => {
    expect(buildApplySteps('not_opened')).toEqual(['current', 'pending', 'pending'])
    expect(buildApplySteps('pending_review')).toEqual(['done', 'current', 'pending'])
    expect(buildApplySteps('rejected')).toEqual(['done', 'failed', 'pending'])
    expect(buildApplySteps('active')).toEqual(['done', 'done', 'done'])
  })
})

describe('domains', () => {
  it('picks primary active+verified domain', () => {
    const list = [domain({ id: 1, domain: 'x.com', status: 'pending_review' }), domain({ id: 2, domain: 'y.com' }), domain({ id: 3, domain: 'z.com', is_primary: true })]
    expect(pickPrimaryDomain(list)?.domain).toBe('z.com')
    expect(pickPrimaryDomain([domain({ status: 'disabled' })])).toBeNull()
  })
  it('validates hostnames loosely', () => {
    expect(isLikelyDomain('shop.example.com')).toBe(true)
    expect(isLikelyDomain('not a domain')).toBe(false)
  })
})

describe('pricing', () => {
  it('estimates price by mode', () => {
    expect(estimateResellerPrice('100.00', { sku_id: 0, pricing_mode: 'inherit' }, '10')).toBe('110.00')
    expect(estimateResellerPrice('98.00', { sku_id: 0, pricing_mode: 'markup_percent', markup_percent: '12.5' })).toBe('110.25')
    expect(estimateResellerPrice('98.00', { sku_id: 0, pricing_mode: 'fixed_markup', fixed_markup_amount: '2.5' })).toBe('100.50')
    expect(estimateResellerPrice('98.00', { sku_id: 0, pricing_mode: 'fixed_price', fixed_price_amount: '120' })).toBe('120.00')
    expect(estimateResellerPrice('abc', { sku_id: 0 })).toBeNull()
  })

  it('payload zeroes fields of unused modes', () => {
    const payload = buildResellerProductSettingPayload([
      { sku_id: 0, pricing_mode: 'fixed_markup', markup_percent: '5', fixed_markup_amount: '3.00', fixed_price_amount: '9' },
      { sku_id: 2, is_listed: false },
    ])
    expect(payload.settings[0]).toEqual({
      sku_id: 0,
      is_listed: true,
      pricing_mode: 'fixed_markup',
      markup_percent: '0.00',
      fixed_markup_amount: '3.00',
      fixed_price_amount: '0.00',
      sort_order: 0,
    })
    expect(payload.settings[1].pricing_mode).toBe('inherit')
    expect(payload.settings[1].is_listed).toBe(false)
  })

  it('summarizes effective price range over listed skus', () => {
    const detail = {
      product_setting: undefined,
      skus: [
        { id: 1, sku_code: 'a', spec_values: {}, base_price_amount: '1', is_active: true, effective_price_amount: '12.00' },
        { id: 2, sku_code: 'b', spec_values: {}, base_price_amount: '1', is_active: true, effective_price_amount: '8.00' },
        { id: 3, sku_code: 'c', spec_values: {}, base_price_amount: '1', is_active: false, effective_price_amount: '99.00' },
      ],
    }
    expect(summarizeProductEffectivePrice(detail)).toBe('8.00 - 12.00')
    expect(countListedSkus(detail)).toBe(2)
  })

  it('normalizes pagination', () => {
    expect(normalizeResellerProductSettingsPagination({ page: '2', page_size: 10, total: 0, total_page: 0 })).toEqual({ page: 2, page_size: 10, total: 0, total_page: 1 })
    expect(normalizeResellerProductSettingsPagination(null).page).toBe(1)
  })
})

describe('finance', () => {
  it('formats amounts', () => {
    expect(formatResellerConsoleAmount('1234.5', 'USD')).toBe('1,234.50 USD')
    expect(formatResellerConsoleAmount('')).toBe('-')
  })
  it('ledger display sign by type', () => {
    expect(ledgerAmountDisplay({ type: 'order_profit', amount: '5', currency: 'CNY' })).toBe('+5.00 CNY')
    expect(ledgerAmountDisplay({ type: 'withdraw_lock', amount: '5', currency: 'CNY' })).toBe('−5.00 CNY')
    expect(ledgerAmountDisplay({ type: 'manual_adjust', amount: '-2', currency: 'CNY' })).toBe('−2.00 CNY')
  })
  it('primary balance is the largest available', () => {
    const b = pickPrimaryResellerBalance([
      { currency: 'CNY', available_amount: '10' },
      { currency: 'USD', available_amount: '30' },
    ])
    expect(b?.currency).toBe('USD')
    expect(pickPrimaryResellerBalance([])).toBeNull()
  })
  it('finance status view', () => {
    expect(getResellerFinanceStatusView({ status: 'pending_review' })).toEqual({ namespace: 'profileStatusMap', key: 'pendingReview', badgeTone: 'warning' })
    expect(getResellerFinanceStatusView({ status: 'active', settlement_status: 'normal' }).badgeTone).toBe('success')
  })
  it('withdraw validation', () => {
    expect(exceedsAvailable('10', 5)).toBe(true)
    expect(exceedsAvailable('5', 5)).toBe(false)
    expect(exceedsAvailable('10', null)).toBe(false)
    expect(validateWithdrawForm({ amount: '', currency: 'CNY', channel: 'a', account: 'b' })).toBe('withdrawAmountRequired')
    expect(validateWithdrawForm({ amount: '1', currency: 'CNY', channel: '', account: 'b' })).toBe('withdrawChannelRequired')
    expect(validateWithdrawForm({ amount: '1', currency: 'CNY', channel: 'a', account: 'b' })).toBeNull()
  })
})

describe('filters', () => {
  it('order params drop empty/all values', () => {
    expect(buildResellerOrderParams({ order_no: ' DJ1 ', status: 'all', created_from: '', created_to: '2026-01-01' })).toEqual({
      order_no: 'DJ1',
      status: undefined,
      created_from: undefined,
      created_to: '2026-01-01',
    })
  })
  it('ledger params parse order id', () => {
    expect(buildResellerLedgerParams({ type: 'order_profit', status: 'all', order_id: '12' })).toEqual({ type: 'order_profit', status: undefined, order_id: 12 })
    expect(buildResellerLedgerParams({ type: 'all', status: 'all', order_id: 'x' }).order_id).toBeUndefined()
  })
})

describe('site config form', () => {
  it('round trips config and drops empty links', () => {
    const form = siteConfigToForm({
      id: 1,
      updated_at: '',
      site_name: ' Shop ',
      footer_links: [{ name: { 'zh-CN': '链接', 'zh-TW': '', 'en-US': '' }, url: 'https://a' }],
      nav_config: { builtin: { blog: false }, custom_items: [] },
    })
    expect(form.nav_config.builtin).toEqual({ blog: false, notice: true, about: true })
    expect(form.announcement.type).toBe('info')
    form.footer_links.push({ name: { 'zh-CN': '', 'zh-TW': '', 'en-US': '' }, url: ' ' })
    const payload = siteConfigFormToPayload(form)
    expect(payload.site_name).toBe('Shop')
    expect(payload.footer_links).toHaveLength(1)
  })
})

describe('product settings list query', () => {
  it('sends the trimmed keyword under the backend `keyword` key', () => {
    const params = buildResellerProductSettingListParams(2, 20, '  netflix ')
    expect(params).toEqual({ page: 2, page_size: 20, keyword: 'netflix' })
    expect(buildUrl('', '/reseller/product-settings', params)).toBe('/reseller/product-settings?page=2&page_size=20&keyword=netflix')
  })
  it('omits an empty keyword', () => {
    expect(buildResellerProductSettingListParams(1, 20, '   ')).toEqual({ page: 1, page_size: 20 })
  })
  it('serialises the configured filter used by the dashboard checklist', () => {
    expect(buildUrl('', '/reseller/product-settings', { configured: 'configured', page: 1, page_size: 1 })).toBe(
      '/reseller/product-settings?configured=configured&page=1&page_size=1',
    )
  })
})
