import { describe, expect, it } from 'vitest'
import { cleanParams, formatMoney, formatPercent, getLocalizedText, hasPositiveAmount, toDateTimeLocal, toLocalizedForm, toMoneyString, toRFC3339 } from './format'

describe('money & format utils', () => {
  it('formats money like the original', () => {
    expect(formatMoney('12.30', 'CNY')).toBe('12.30 CNY')
    expect(formatMoney(5)).toBe('5')
    expect(formatMoney('')).toBe('-')
    expect(formatMoney(null, 'USD')).toBe('-')
  })

  it('normalizes money to two-decimal strings', () => {
    expect(toMoneyString('12.3')).toBe('12.30')
    expect(toMoneyString(0.1 + 0.2)).toBe('0.30')
    expect(toMoneyString(-1.005)).toBe('-1.01')
    expect(toMoneyString('abc')).toBe('0.00')
    expect(toMoneyString(undefined)).toBe('0.00')
    expect(toMoneyString(-0.001)).toBe('0.00')
  })

  it('detects positive amounts and percents', () => {
    expect(hasPositiveAmount('0.01')).toBe(true)
    expect(hasPositiveAmount('0')).toBe(false)
    expect(hasPositiveAmount('')).toBe(false)
    expect(formatPercent('12.5')).toBe('12.50%')
    expect(formatPercent('')).toBe('-')
  })

  it('picks localized text with fallbacks', () => {
    const v = { 'zh-CN': '简体', 'en-US': 'English' }
    expect(getLocalizedText(v, 'en-US')).toBe('English')
    expect(getLocalizedText(v, 'zh-TW')).toBe('简体')
    expect(getLocalizedText('plain', 'en-US')).toBe('plain')
    expect(getLocalizedText(null, 'en-US')).toBe('')
    expect(toLocalizedForm({ 'en-US': 'x', foo: 1 })).toEqual({ 'zh-CN': '', 'zh-TW': '', 'en-US': 'x' })
  })

  it('converts datetimes', () => {
    expect(toRFC3339('')).toBeUndefined()
    expect(toRFC3339('2026-01-02T03:04')).toBe(new Date('2026-01-02T03:04').toISOString())
    expect(toDateTimeLocal(new Date(2026, 0, 2, 3, 4).toISOString())).toBe('2026-01-02T03:04')
  })

  it('cleans list params', () => {
    expect(cleanParams({ a: '', b: '__all__', c: ' x ', d: 0, e: undefined, f: false })).toEqual({ c: 'x', d: 0, f: false })
  })
})

describe('labelSeparator', () => {
  it('QA-A23 uses a full-width colon only for Chinese locales', async () => {
    const { labelSeparator } = await import('./format')
    expect(labelSeparator('zh-CN')).toBe('：')
    expect(labelSeparator('zh-TW')).toBe('：')
    expect(labelSeparator('en-US')).toBe(': ')
  })
})
