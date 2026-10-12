import { describe, expect, it } from 'vitest'
import {
  buildCurrencyOptions,
  buildThemePayload,
  clampNumber,
  invalidThemeColors,
  isRichTextEmpty,
  joinAllowedEmailDomains,
  normalizeCurrency,
  normalizeFooterLinks,
  normalizeLocalizedField,
  normalizeSceneTemplate,
  normalizeSiteScripts,
  normalizeTheme,
  splitAllowedEmailDomains,
  splitNumericIDs,
  splitRecipients,
  toColorInputValue,
} from './settingsUtils'
import { applyTemplatePreset, COLOR_PRESETS, normalizeStorefrontTemplate, TEMPLATE_COLORS } from './themePresets'
import { useSiteSettings } from './useSiteSettings'

describe('settingsUtils: basic tab', () => {
  it('clamps numbers with fallback for empty/NaN', () => {
    expect(clampNumber('', 1, 10, 5)).toBe(5)
    expect(clampNumber('abc', 1, 10, 5)).toBe(5)
    expect(clampNumber(0, 1, 10, 5)).toBe(1)
    expect(clampNumber(99, 1, 10, 5)).toBe(10)
    expect(clampNumber('7', 1, 10, 5)).toBe(7)
  })

  it('normalizes localized fields', () => {
    expect(normalizeLocalizedField({ 'zh-CN': 'a', 'en-US': 3 })).toEqual({ 'zh-CN': 'a', 'zh-TW': '', 'en-US': '' })
    expect(normalizeLocalizedField(null)).toEqual({ 'zh-CN': '', 'zh-TW': '', 'en-US': '' })
  })

  it('splits allowed email domains (dedupe, strip @, lowercase, mixed separators)', () => {
    expect(splitAllowedEmailDomains('@Gmail.com, qq.com；gmail.com\nfoo.org')).toEqual(['gmail.com', 'qq.com', 'foo.org'])
    expect(joinAllowedEmailDomains(['a.com', '', ' b.com '])).toBe('a.com\nb.com')
    expect(joinAllowedEmailDomains('x')).toBe('')
  })

  it('normalizes site scripts and footer links', () => {
    expect(normalizeSiteScripts([{ name: 'ga', enabled: 'yes', position: 'body_end', code: 'x' }, { enabled: 0 }, 'junk'])).toEqual([
      { name: 'ga', enabled: true, position: 'body_end', code: 'x' },
      { name: '', enabled: false, position: 'head', code: '' },
    ])
    expect(normalizeFooterLinks([{ name: 'ICP', url: '/icp' }, { name: '  ', url: 'x' }])).toEqual([{ name: 'ICP', url: '/icp' }])
  })

  it('currency options start with CNY and are unique ISO codes', () => {
    const opts = buildCurrencyOptions()
    expect(opts[0]).toBe('CNY')
    expect(opts).toContain('USD')
    expect(new Set(opts).size).toBe(opts.length)
    expect(normalizeCurrency('usd')).toBe('USD')
    expect(normalizeCurrency('dollar')).toBe('CNY')
  })

  it('detects empty rich text but keeps image-only content', () => {
    expect(isRichTextEmpty('<p>&nbsp;</p>')).toBe(true)
    expect(isRichTextEmpty('<p><img src="a.png"></p>')).toBe(false)
    expect(isRichTextEmpty('<p>hi</p>')).toBe(false)
  })
})

describe('settingsUtils: theme', () => {
  it('keeps an intentional Sakura palette and resets colours to the selected template', () => {
    const settings = useSiteSettings()
    settings.loadSite({
      storefront_template: 'vault',
      theme: { ...TEMPLATE_COLORS.classic, default_mode: 'dark' },
    })
    expect(settings.form.theme.primary_color).toBe(TEMPLATE_COLORS.classic.primary_color)
    settings.resetThemeColors()
    expect(settings.form.theme.primary_color).toBe(TEMPLATE_COLORS.vault.primary_color)
    expect(settings.form.theme.default_mode).toBe('dark')
  })
  it('keeps Sakura with system mode when the selected vault has Sakura effects off', () => {
    const settings = useSiteSettings()
    settings.loadSite({
      storefront_template: 'vault',
      theme: { ...TEMPLATE_COLORS.classic, default_mode: 'system', effects: { sakura: false } },
    })
    expect(settings.form.theme.primary_color).toBe(TEMPLATE_COLORS.classic.primary_color)
  })
  it('keeps every storefront template selectable and gives vault its own dark palette', () => {
    expect(['classic', 'vault', 'aurora', 'atelier', 'noir'].map(normalizeStorefrontTemplate))
      .toEqual(['classic', 'vault', 'aurora', 'atelier', 'noir'])
    expect(normalizeStorefrontTemplate('unknown')).toBe('classic')
    const theme = normalizeTheme({})
    applyTemplatePreset(theme, 'vault')
    expect(theme.primary_color).toBe(TEMPLATE_COLORS.vault.primary_color)
    expect(theme.default_mode).toBe('dark')
    expect(theme.effects.sakura).toBe(false)
    theme.background_color = '#123456'
    applyTemplatePreset(theme, 'aurora')
    expect(theme.background_color).toBe('')
    expect(theme.default_mode).toBe('light')
    expect(COLOR_PRESETS).toHaveLength(8)
  })

  it('preserves independent page-section switches and optional colours', () => {
    const theme = normalizeTheme({
      show_home_hero: false, show_footer_quick_links: false, show_repository_link: false,
      background_color: '#123456', surface_color: '#fafafa', text_color: '#abcdef',
    })
    expect(theme.show_home_hero).toBe(false)
    expect(theme.show_footer_quick_links).toBe(false)
    expect(theme.show_repository_link).toBe(false)
    expect(buildThemePayload(theme, {})).toMatchObject({
      show_home_hero: false, show_footer_quick_links: false, show_repository_link: false,
      background_color: '#123456', surface_color: '#fafafa', text_color: '#abcdef',
    })
    theme.text_color = 'red'
    expect(invalidThemeColors(theme)).toEqual(['text_color'])
  })
  it('fills defaults for missing/invalid theme values', () => {
    const th = normalizeTheme({ primary_color: 'red', accent_color: '#0af', effects: { sakura: false }, default_mode: 'dark' })
    expect(th.primary_color).toBe('#ff5fa2')
    expect(th.secondary_color).toBe('#8b5cf6')
    expect(th.accent_color).toBe('#0af')
    expect(th.effects).toEqual({ sakura: false, sparkle: true })
    expect(th.default_mode).toBe('dark')
    expect(normalizeTheme(undefined).default_mode).toBe('system')
  })

  it('expands short hex for the colour input', () => {
    expect(toColorInputValue('#0AF', '#000000')).toBe('#00aaff')
    expect(toColorInputValue('nope', '#123456')).toBe('#123456')
  })

  it('reports invalid colours and preserves unknown theme keys on save', () => {
    const th = normalizeTheme({})
    th.primary_color = '#12'
    expect(invalidThemeColors(th)).toEqual(['primary_color'])
    th.primary_color = '#AABBCC'
    const payload = buildThemePayload(th, { custom_flag: 1, effects: { snow: true } })
    expect(payload.custom_flag).toBe(1)
    expect(payload.primary_color).toBe('#aabbcc')
    expect(payload.effects).toEqual({ snow: true, sakura: true, sparkle: true })
  })
})

describe('settingsUtils: notification center', () => {
  it('splits recipients by newline or comma', () => {
    expect(splitRecipients('a@x.com\n b@x.com , ,c')).toEqual(['a@x.com', 'b@x.com', 'c'])
  })
  it('splits numeric ids (positive ints, deduped)', () => {
    expect(splitNumericIDs('1\n2,2, -3, x, 4.5, 7')).toEqual([1, 2, 7])
  })
  it('normalizes scene templates per language', () => {
    expect(normalizeSceneTemplate({ 'zh-CN': { title: 't', body: 1 } })['zh-CN']).toEqual({ title: 't', body: '' })
  })
})
