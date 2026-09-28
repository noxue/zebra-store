// Pure helpers for 站点设置 / 通知中心 (ported from views/admin/Settings.vue and its tab components).

export const SUPPORTED_LANGS = ['zh-CN', 'zh-TW', 'en-US'] as const
export type LangCode = (typeof SUPPORTED_LANGS)[number]
export type LocalizedField = Record<LangCode, string>

export const SITE_SCRIPTS_MAX = 20
export const FOOTER_LINKS_MAX = 20
export const ABOUT_SERVICES_MAX = 12
export const NAV_CUSTOM_MAX = 10

export const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v)
export const asRecord = (v: unknown): Record<string, unknown> => (isRecord(v) ? v : {})
export const asString = (v: unknown, fallback = ''): string => (v === null || v === undefined || v === '' ? fallback : String(v))

export const createLocalizedField = (): LocalizedField => ({ 'zh-CN': '', 'zh-TW': '', 'en-US': '' })

export const normalizeLocalizedField = (raw: unknown): LocalizedField => {
  const out = createLocalizedField()
  if (!isRecord(raw)) return out
  SUPPORTED_LANGS.forEach((lang) => {
    const value = raw[lang]
    out[lang] = typeof value === 'string' ? value : ''
  })
  return out
}

export const isLocalizedFieldNotEmpty = (value: LocalizedField) => Object.values(value).some((item) => item.trim() !== '')

export const normalizeNumber = (value: unknown, fallback: number): number => {
  if (value === null || value === undefined || value === '') return fallback
  const parsed = Number(value)
  return Number.isNaN(parsed) ? fallback : parsed
}

export const clampNumber = (value: unknown, min: number, max: number, fallback: number): number => {
  const parsed = normalizeNumber(value, fallback)
  if (parsed < min) return min
  if (parsed > max) return max
  return parsed
}

// ---- basic tab ----
export type SiteScriptPosition = 'head' | 'body_end'
export interface SiteScriptItem {
  name: string
  enabled: boolean
  position: SiteScriptPosition
  code: string
}
export interface FooterLinkItem {
  name: string
  url: string
}

export const createSiteScriptItem = (): SiteScriptItem => ({ name: '', enabled: true, position: 'head', code: '' })
export const createFooterLinkItem = (): FooterLinkItem => ({ name: '', url: '' })

export const normalizeSiteScriptEnabled = (raw: unknown): boolean => {
  if (typeof raw === 'boolean') return raw
  if (typeof raw === 'number') return raw !== 0
  if (typeof raw === 'string') {
    const value = raw.trim().toLowerCase()
    return value === '1' || value === 'true' || value === 'yes' || value === 'on'
  }
  return false
}

export const normalizeSiteScripts = (raw: unknown): SiteScriptItem[] => {
  if (!Array.isArray(raw)) return []
  return raw
    .filter(isRecord)
    .map((value) => ({
      name: typeof value.name === 'string' ? value.name : '',
      enabled: normalizeSiteScriptEnabled(value.enabled),
      position: (value.position === 'body_end' ? 'body_end' : 'head') as SiteScriptPosition,
      code: typeof value.code === 'string' ? value.code : '',
    }))
    .slice(0, SITE_SCRIPTS_MAX)
}

export const normalizeFooterLinks = (raw: unknown): FooterLinkItem[] => {
  if (!Array.isArray(raw)) return []
  return raw
    .filter(isRecord)
    .map((value) => ({
      name: typeof value.name === 'string' ? value.name : '',
      url: typeof value.url === 'string' ? value.url : '',
    }))
    .filter((item) => item.name.trim() !== '')
    .slice(0, FOOTER_LINKS_MAX)
}

export const splitAllowedEmailDomains = (raw: string): string[] => {
  const seen = new Set<string>()
  const result: string[] = []
  raw
    .split(/[\s,，;；]+/)
    .map((item) => item.trim().replace(/^@+/, '').toLowerCase())
    .filter(Boolean)
    .forEach((domain) => {
      if (seen.has(domain)) return
      seen.add(domain)
      result.push(domain)
    })
  return result
}

export const joinAllowedEmailDomains = (raw: unknown): string => {
  if (!Array.isArray(raw)) return ''
  return raw
    .map((item) => String(item || '').trim())
    .filter(Boolean)
    .join('\n')
}

const FALLBACK_CURRENCIES = [
  'CNY', 'USD', 'EUR', 'GBP', 'JPY', 'KRW', 'HKD', 'TWD', 'SGD', 'AUD',
  'CAD', 'CHF', 'NZD', 'SEK', 'NOK', 'DKK', 'AED', 'SAR', 'MYR', 'THB',
  'PHP', 'IDR', 'VND', 'INR', 'RUB', 'TRY', 'ZAR', 'BRL', 'MXN', 'ARS',
]

/** CNY first, then every ISO 4217 code the runtime knows (plus the fallback list), sorted. */
export const buildCurrencyOptions = (): string[] => {
  const values: string[] = []
  const intl = Intl as unknown as { supportedValuesOf?: (key: string) => unknown }
  if (typeof intl.supportedValuesOf === 'function') {
    const candidate = intl.supportedValuesOf('currency')
    if (Array.isArray(candidate)) values.push(...candidate.map((item: unknown) => String(item || '').trim().toUpperCase()))
  }
  values.push(...FALLBACK_CURRENCIES)
  const unique = Array.from(new Set(values.filter((item) => /^[A-Z]{3}$/.test(item))))
  return ['CNY', ...unique.filter((item) => item !== 'CNY').sort()]
}

export const normalizeCurrency = (raw: unknown): string => {
  const value = String(raw || 'CNY').trim().toUpperCase()
  return /^[A-Z]{3}$/.test(value) ? value : 'CNY'
}

/** 判断富文本是否为空（保留纯图片/表格类内容） */
export const isRichTextEmpty = (html: string): boolean => {
  if (!html) return true
  if (/<(img|table|iframe|hr|video)\b/i.test(html)) return false
  return html.replace(/<[^>]*>/g, '').replace(/&nbsp;/gi, '').trim() === ''
}

// ---- theme (Zebra Store addition: site_config.theme) ----
export type ThemeMode = 'system' | 'light' | 'dark'
export interface ThemeForm {
  primary_color: string
  secondary_color: string
  accent_color: string
  background_image: string
  mascot_image: string
  login_background: string
  effects: { sakura: boolean; sparkle: boolean }
  default_mode: ThemeMode
}

export const DEFAULT_THEME_COLORS = {
  primary_color: '#ff5fa2',
  secondary_color: '#8b5cf6',
  accent_color: '#38bdf8',
} as const

const HEX_RE = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i
export const isHexColor = (v: string) => HEX_RE.test(v.trim())

/** `<input type="color">` only accepts #rrggbb. */
export const toColorInputValue = (v: string, fallback: string): string => {
  const s = v.trim()
  if (!HEX_RE.test(s)) return fallback
  if (s.length === 4) return `#${s[1]}${s[1]}${s[2]}${s[2]}${s[3]}${s[3]}`.toLowerCase()
  return s.toLowerCase()
}

export const createThemeForm = (): ThemeForm => ({
  ...DEFAULT_THEME_COLORS,
  background_image: '',
  mascot_image: '',
  login_background: '',
  effects: { sakura: true, sparkle: true },
  default_mode: 'system',
})

export const normalizeTheme = (raw: unknown): ThemeForm => {
  const d = createThemeForm()
  if (!isRecord(raw)) return d
  const effects = asRecord(raw.effects)
  const color = (v: unknown, fb: string) => (typeof v === 'string' && isHexColor(v) ? v.trim() : fb)
  const mode = raw.default_mode
  return {
    primary_color: color(raw.primary_color, d.primary_color),
    secondary_color: color(raw.secondary_color, d.secondary_color),
    accent_color: color(raw.accent_color, d.accent_color),
    background_image: asString(raw.background_image),
    mascot_image: asString(raw.mascot_image),
    login_background: asString(raw.login_background),
    effects: { sakura: effects.sakura !== false, sparkle: effects.sparkle !== false },
    default_mode: mode === 'light' || mode === 'dark' ? mode : 'system',
  }
}

/** Returns the list of invalid colour fields (empty = OK). */
export const invalidThemeColors = (theme: ThemeForm): string[] =>
  (['primary_color', 'secondary_color', 'accent_color'] as const).filter((k) => !isHexColor(theme[k]))

export const buildThemePayload = (theme: ThemeForm, rawTheme: unknown): Record<string, unknown> => ({
  ...asRecord(rawTheme),
  primary_color: theme.primary_color.trim().toLowerCase(),
  secondary_color: theme.secondary_color.trim().toLowerCase(),
  accent_color: theme.accent_color.trim().toLowerCase(),
  background_image: theme.background_image,
  mascot_image: theme.mascot_image,
  login_background: theme.login_background,
  effects: { ...asRecord(asRecord(rawTheme).effects), sakura: theme.effects.sakura, sparkle: theme.effects.sparkle },
  default_mode: theme.default_mode,
})

// ---- notification center ----
export const splitRecipients = (raw: string): string[] =>
  raw
    .split(/\r?\n|,/)
    .map((item) => item.trim())
    .filter((item) => item !== '')

export const joinLines = (items: unknown): string => {
  if (!Array.isArray(items)) return ''
  return items
    .map((item) => String(item ?? '').trim())
    .filter((item) => item !== '')
    .join('\n')
}

export const splitNumericIDs = (raw: string): number[] => {
  const seen = new Set<number>()
  return raw
    .split(/\r?\n|,/)
    .map((item) => Number(item.trim()))
    .filter((item) => Number.isInteger(item) && item > 0)
    .filter((item) => {
      if (seen.has(item)) return false
      seen.add(item)
      return true
    })
}

export interface TitleBody {
  title: string
  body: string
}
export type SceneTemplate = Record<LangCode, TitleBody>

export const createSceneTemplate = (): SceneTemplate => ({
  'zh-CN': { title: '', body: '' },
  'zh-TW': { title: '', body: '' },
  'en-US': { title: '', body: '' },
})

export const normalizeSceneTemplate = (raw: unknown): SceneTemplate => {
  const out = createSceneTemplate()
  if (!isRecord(raw)) return out
  SUPPORTED_LANGS.forEach((lang) => {
    const item = raw[lang]
    if (!isRecord(item)) return
    out[lang].title = typeof item.title === 'string' ? item.title : ''
    out[lang].body = typeof item.body === 'string' ? item.body : ''
  })
  return out
}
