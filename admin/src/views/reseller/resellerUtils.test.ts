import { describe, expect, it } from 'vitest'
import type { AdminResellerProductSettingDetail, AdminResellerSiteConfig } from '@/api/types'
import {
  buildSiteConfigPayload,
  createBlankSiteConfigForm,
  isNegativeBalance,
  pricingModeKey,
  pricingValue,
  processorName,
  queryString,
  resolveResellerUserId,
  seedPreviewEntries,
  settingFormFromRule,
  siteConfigToForm,
  siteConfigUserLabel,
  skuLabel,
  validateMarkupRange,
} from './resellerUtils'

describe('validateMarkupRange', () => {
  it('accepts empty values as zero', () => {
    expect(validateMarkupRange('', '')).toBe('')
  })
  it('rejects negatives and non-numbers', () => {
    expect(validateMarkupRange('-1', '0')).toBe('admin.resellerProfiles.actions.markupInvalid')
    expect(validateMarkupRange('abc', '0')).toBe('admin.resellerProfiles.actions.markupInvalid')
  })
  it('rejects default above a positive max, but max 0 means unlimited', () => {
    expect(validateMarkupRange('20', '10')).toBe('admin.resellerProfiles.actions.markupRangeInvalid')
    expect(validateMarkupRange('20', '0')).toBe('')
    expect(validateMarkupRange('10', '10')).toBe('')
  })
})

describe('row helpers', () => {
  it('resolves the reseller user id from profile or nested user', () => {
    expect(resolveResellerUserId({ profile: { id: 1, user_id: 7 } })).toBe(7)
    expect(
      resolveResellerUserId({
        profile: { id: 1, user_id: 0, user: { id: 9 } },
      }),
    ).toBe(9)
    expect(resolveResellerUserId({})).toBe(0)
  })
  it('reads the first query value', () => {
    expect(queryString(['3', '4'])).toBe('3')
    expect(queryString(' 5 ')).toBe('5')
    expect(queryString(undefined)).toBe('')
  })
  it('names the withdraw processor', () => {
    expect(processorName({ processor: { username: 'root' }, processed_by: 1 })).toBe('root')
    expect(processorName({ processor: 'ops' })).toBe('ops')
    expect(processorName({ processed_by: 4 })).toBe('4')
    expect(processorName({})).toBe('-')
  })
  it('flags positive negative-balance amounts', () => {
    expect(isNegativeBalance('0.00')).toBe(false)
    expect(isNegativeBalance('3.10')).toBe(true)
  })
})

describe('product settings', () => {
  it('maps pricing modes to i18n keys and display values', () => {
    expect(pricingModeKey('markup_percent')).toBe('markupPercent')
    expect(pricingModeKey('weird')).toBe('unknown')
    const base = {
      markup_percent: '5.00',
      fixed_markup_amount: '1.50',
      fixed_price_amount: '9.90',
    }
    expect(pricingValue({ ...base, pricing_mode: 'markup_percent' })).toBe('5.00%')
    expect(pricingValue({ ...base, pricing_mode: 'fixed_markup' })).toBe('+1.50')
    expect(pricingValue({ ...base, pricing_mode: 'fixed_price' })).toBe('9.90')
    expect(pricingValue({ ...base, pricing_mode: 'inherit' })).toBe('-')
  })
  it('labels skus by spec values, then code, then id', () => {
    expect(skuLabel({ id: 1, sku_code: 'A', spec_values: { c: 'Red', s: ' L ' } })).toBe('Red / L')
    expect(skuLabel({ id: 1, sku_code: 'A', spec_values: {} })).toBe('A')
    expect(skuLabel({ id: 3, sku_code: '', spec_values: {} })).toBe('#3')
  })
  it('builds a form from a rule with defaults', () => {
    expect(settingFormFromRule(undefined, 5)).toEqual({
      sku_id: 5,
      is_listed: true,
      pricing_mode: 'inherit',
      markup_percent: '0.00',
      fixed_markup_amount: '0.00',
      fixed_price_amount: '0.00',
      sort_order: 0,
    })
    const f = settingFormFromRule(
      {
        product_id: 1,
        sku_id: 0,
        is_listed: false,
        pricing_mode: 'fixed_price',
        markup_percent: '',
        fixed_markup_amount: 0,
        fixed_price_amount: '12.00',
        sort_order: 2,
      },
      0,
    )
    expect(f.is_listed).toBe(false)
    expect(f.fixed_price_amount).toBe('12.00')
    expect(f.markup_percent).toBe('0.00')
    expect(f.fixed_markup_amount).toBe('0')
  })
  it('seeds preview entries from saved effective prices', () => {
    const detail = {
      product: {
        id: 1,
        slug: 'p',
        title: {},
        price_amount: '10.00',
        is_active: true,
      },
      product_setting: {
        product_id: 1,
        sku_id: 0,
        is_listed: true,
        pricing_mode: 'inherit',
        markup_percent: 0,
        fixed_markup_amount: 0,
        fixed_price_amount: 0,
        sort_order: 0,
        effective_price_amount: '10.00',
      },
      skus: [
        {
          id: 11,
          sku_code: 'a',
          spec_values: {},
          base_price_amount: '10.00',
          is_active: true,
          effective_price_amount: '11.00',
        },
        {
          id: 12,
          sku_code: 'b',
          spec_values: {},
          base_price_amount: '10.00',
          is_active: true,
        },
      ],
    } as AdminResellerProductSettingDetail
    expect(seedPreviewEntries(detail)).toEqual({
      0: { effective: '10.00', valid: true, errorCode: '' },
      11: { effective: '11.00', valid: true, errorCode: '' },
    })
  })
})

describe('site config form', () => {
  it('round-trips a config into a trimmed payload and drops empty footer links', () => {
    const row = {
      id: 1,
      reseller_id: 2,
      site_name: ' Shop ',
      logo: '',
      favicon: '',
      announcement: { enabled: true, type: '', title: { 'zh-CN': 'Hi' } },
      support: { email: ' a@b.c ' },
      seo: {},
      nav_config: { builtin: { blog: false } },
      footer_links: [
        { name: { 'en-US': 'Docs' }, url: '' },
        { name: {}, url: '  ' },
      ],
      created_at: '',
      updated_at: '',
    } as AdminResellerSiteConfig
    const form = siteConfigToForm(row)
    expect(form.announcement.title).toEqual({
      'zh-CN': 'Hi',
      'zh-TW': '',
      'en-US': '',
    })
    expect(form.nav_config.builtin).toEqual({
      blog: false,
      notice: true,
      about: true,
    })
    const payload = buildSiteConfigPayload(form)
    expect(payload.site_name).toBe('Shop')
    expect(payload.announcement?.type).toBe('info')
    expect(payload.support?.email).toBe('a@b.c')
    expect(payload.footer_links).toHaveLength(1)
    expect(payload.nav_config).toEqual({
      builtin: { blog: false, notice: true, about: true },
      custom_items: [],
    })
  })
  it('blank form has all nav items enabled', () => {
    expect(createBlankSiteConfigForm().nav_config.builtin).toEqual({
      blog: true,
      notice: true,
      about: true,
    })
  })
  it('labels the owner by email, name or id', () => {
    expect(
      siteConfigUserLabel({
        reseller_id: 3,
        profile: { id: 3, user_id: 8, user: { id: 8, email: 'x@y.z' } },
      }),
    ).toBe('x@y.z')
    expect(siteConfigUserLabel({ reseller_id: 3, profile: { id: 3, user_id: 8 } })).toBe('#8')
    expect(siteConfigUserLabel({ reseller_id: 3 })).toBe('#3')
  })
})
