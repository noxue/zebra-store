// Pure helpers for the Telegram Bot views (settings parsing, status labels, broadcast helpers).
import type { BadgeTone } from '@/components/ui/Badge'

export type Translate = (key: string, named?: Record<string, unknown>) => string

// ---------------------------------------------------------------------------
// Bot settings (ported from the original composables/useTelegramBotSettings.ts)
// ---------------------------------------------------------------------------

export const supportedLanguages = ['zh-CN', 'zh-TW', 'en-US'] as const
export type SupportedLanguage = (typeof supportedLanguages)[number]
export type LocalizedText = Record<SupportedLanguage, string>
export type MenuActionType = 'builtin' | 'url' | 'web_app' | 'command'

export interface MenuItem {
  key: string
  enabled: boolean
  order: number
  label: LocalizedText
  action: {
    type: MenuActionType
    value: string
  }
}

export interface HelpItem {
  key: string
  enabled: boolean
  order: number
  summary: LocalizedText
  title: LocalizedText
  content: LocalizedText
  show_support_link: boolean
}

export interface TelegramBotSettingsForm {
  enabled: boolean
  default_locale: SupportedLanguage
  basic: {
    display_name: string
    description: LocalizedText
    support_url: string
    cover_url: string
  }
  welcome: {
    enabled: boolean
    message: LocalizedText
  }
  help: {
    enabled: boolean
    title: LocalizedText
    intro: LocalizedText
    center_hint: LocalizedText
    support_hint: LocalizedText
    items: HelpItem[]
  }
  menu: {
    items: MenuItem[]
  }
}

export const menuActionTypes: MenuActionType[] = ['builtin', 'url', 'web_app', 'command']
export const menuItemsMaxCount = 20
export const helpItemsMaxCount = 12

const isRecord = (raw: unknown): raw is Record<string, unknown> => !!raw && typeof raw === 'object' && !Array.isArray(raw)

const isSupportedLanguage = (v: unknown): v is SupportedLanguage => typeof v === 'string' && (supportedLanguages as readonly string[]).includes(v)

const isMenuActionType = (v: unknown): v is MenuActionType => typeof v === 'string' && (menuActionTypes as string[]).includes(v)

export const emptyLocalizedText = (): LocalizedText => ({ 'zh-CN': '', 'zh-TW': '', 'en-US': '' })

export const createMenuItem = (): MenuItem => ({
  key: '',
  enabled: true,
  order: 0,
  label: emptyLocalizedText(),
  action: { type: 'builtin', value: '' },
})

export const createHelpItem = (): HelpItem => ({
  key: '',
  enabled: true,
  order: 0,
  summary: emptyLocalizedText(),
  title: emptyLocalizedText(),
  content: emptyLocalizedText(),
  show_support_link: false,
})

export const createTelegramBotSettingsForm = (): TelegramBotSettingsForm => ({
  enabled: false,
  default_locale: 'zh-CN',
  basic: { display_name: '', description: emptyLocalizedText(), support_url: '', cover_url: '' },
  welcome: { enabled: false, message: emptyLocalizedText() },
  help: {
    enabled: true,
    title: emptyLocalizedText(),
    intro: emptyLocalizedText(),
    center_hint: emptyLocalizedText(),
    support_hint: emptyLocalizedText(),
    items: [],
  },
  menu: { items: [] },
})

export const parseLocalized = (raw: unknown): LocalizedText => {
  const result = emptyLocalizedText()
  if (isRecord(raw)) {
    for (const lang of supportedLanguages) {
      const v = raw[lang]
      if (typeof v === 'string') result[lang] = v
    }
  }
  return result
}

export const parseMenuItem = (raw: unknown): MenuItem => {
  const item = createMenuItem()
  if (!isRecord(raw)) return item
  if (typeof raw.key === 'string') item.key = raw.key
  if (typeof raw.enabled === 'boolean') item.enabled = raw.enabled
  if (typeof raw.order === 'number') item.order = raw.order
  item.label = parseLocalized(raw.label)
  if (isRecord(raw.action)) {
    if (isMenuActionType(raw.action.type)) item.action.type = raw.action.type
    if (typeof raw.action.value === 'string') item.action.value = raw.action.value
  }
  return item
}

export const parseHelpItem = (raw: unknown): HelpItem => {
  const item = createHelpItem()
  if (!isRecord(raw)) return item
  if (typeof raw.key === 'string') item.key = raw.key
  if (typeof raw.enabled === 'boolean') item.enabled = raw.enabled
  if (typeof raw.order === 'number') item.order = raw.order
  if (typeof raw.show_support_link === 'boolean') item.show_support_link = raw.show_support_link
  item.summary = parseLocalized(raw.summary)
  item.title = parseLocalized(raw.title)
  item.content = parseLocalized(raw.content)
  return item
}

const str = (v: unknown, fallback = '') => (typeof v === 'string' ? v : fallback)
const bool = (v: unknown, fallback: boolean) => (typeof v === 'boolean' ? v : fallback)

/** Build the editable form from the `settings/telegram-bot` response (same defaults as the original). */
export const parseTelegramBotSettings = (data: unknown): TelegramBotSettingsForm => {
  const form = createTelegramBotSettingsForm()
  if (!isRecord(data)) return form
  form.enabled = bool(data.enabled, false)
  if (isSupportedLanguage(data.default_locale)) form.default_locale = data.default_locale

  if (isRecord(data.basic)) {
    const basic = data.basic
    form.basic.display_name = str(basic.display_name)
    form.basic.description = parseLocalized(basic.description)
    form.basic.support_url = str(basic.support_url)
    form.basic.cover_url = str(basic.cover_url)
  }
  if (isRecord(data.welcome)) {
    form.welcome.enabled = bool(data.welcome.enabled, false)
    form.welcome.message = parseLocalized(data.welcome.message)
  }
  if (isRecord(data.help)) {
    const help = data.help
    form.help.enabled = bool(help.enabled, true)
    form.help.title = parseLocalized(help.title)
    form.help.intro = parseLocalized(help.intro)
    form.help.center_hint = parseLocalized(help.center_hint)
    form.help.support_hint = parseLocalized(help.support_hint)
    if (Array.isArray(help.items)) form.help.items = help.items.map(parseHelpItem)
  }
  if (isRecord(data.menu) && Array.isArray(data.menu.items)) {
    form.menu.items = data.menu.items.map(parseMenuItem)
  }
  return form
}

/** Swap an item with its neighbour in place; returns false when out of range. */
export const moveItem = <T>(items: T[], index: number, direction: 'up' | 'down'): boolean => {
  const target = direction === 'up' ? index - 1 : index + 1
  if (index < 0 || index >= items.length || target < 0 || target >= items.length) return false
  const a = items[index] as T
  items[index] = items[target] as T
  items[target] = a
  return true
}

// ---------------------------------------------------------------------------
// Runtime status labels
// ---------------------------------------------------------------------------

export const formatWebhookStatus = (t: Translate, value?: string): string => {
  if (!value) return '-'
  const normalized = value.trim().toLowerCase()
  if (['active', 'enabled', 'connected', 'ok'].includes(normalized)) return t('telegramBot.status.webhookStatusActive')
  if (['inactive', 'disabled', 'disconnected'].includes(normalized)) return t('telegramBot.status.webhookStatusInactive')
  return value
}

const LICENSE_STATUS_KEYS: Record<string, string> = {
  active: 'telegramBot.status.licenseStatusActive',
  expired: 'telegramBot.status.licenseStatusExpired',
  revoked: 'telegramBot.status.licenseStatusRevoked',
  suspended: 'telegramBot.status.licenseStatusSuspended',
  inactive: 'telegramBot.status.licenseStatusInactive',
}

export const formatLicenseStatus = (t: Translate, value?: string): string => {
  if (!value) return t('telegramBot.status.licenseStatusUnknown')
  const key = LICENSE_STATUS_KEYS[value.trim().toLowerCase()]
  return key ? t(key) : value
}

export const licenseStatusTone = (value?: string): BadgeTone => {
  const normalized = value?.trim().toLowerCase()
  if (normalized === 'active') return 'success'
  if (normalized === 'expired' || normalized === 'revoked' || normalized === 'suspended') return 'danger'
  return 'neutral'
}

export const formatWarning = (t: Translate, value: string): string => {
  const normalized = value.trim().toLowerCase()
  if (normalized === 'license_lease_expiring_soon') return t('telegramBot.status.warningLeaseExpiringSoon')
  if (normalized === 'license_lease_expired') return t('telegramBot.status.warningLeaseExpired')
  return value
}

export const formatWarnings = (t: Translate, warnings?: string[]): string => {
  if (!warnings?.length) return t('telegramBot.status.licenseWarningsEmpty')
  return warnings.map((w) => formatWarning(t, w)).join(' / ')
}

// ---------------------------------------------------------------------------
// Broadcasts
// ---------------------------------------------------------------------------

export type BroadcastStatus = 'pending' | 'running' | 'completed' | 'failed'

export const normalizeBroadcastStatus = (value: unknown): BroadcastStatus => {
  const normalized = String(value ?? '').trim().toLowerCase()
  if (normalized === 'running' || normalized === 'completed' || normalized === 'failed') return normalized
  return 'pending'
}

export const formatBroadcastStatus = (t: Translate, value: unknown): string => {
  const s = normalizeBroadcastStatus(value)
  return t(`telegramBot.broadcasts.status${s.charAt(0).toUpperCase()}${s.slice(1)}`)
}

export const broadcastStatusTone = (value: unknown): BadgeTone => {
  const s = normalizeBroadcastStatus(value)
  if (s === 'completed') return 'success'
  if (s === 'failed') return 'danger'
  if (s === 'running') return 'info'
  return 'neutral'
}

export const formatRecipientType = (t: Translate, value: string): string =>
  value === 'specific' ? t('telegramBot.broadcasts.recipientTypeSpecific') : t('telegramBot.broadcasts.recipientTypeAll')

/** Only finished broadcasts (completed / failed) may be deleted. */
export const canDeleteBroadcast = (status: unknown): boolean => {
  const s = normalizeBroadcastStatus(status)
  return s === 'completed' || s === 'failed'
}

/** File name part of a media path/URL (used as attachment_name when picked from the library). */
export const fileNameFromPath = (path: string): string => path.split('?')[0]?.split('/').pop() || ''

const escapeHtml = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
const escapeAttr = (s: string) => escapeHtml(s).replace(/"/g, '&quot;')

const INLINE_TAGS: Record<string, string> = {
  b: 'b',
  strong: 'b',
  i: 'i',
  em: 'i',
  u: 'u',
  ins: 'u',
  s: 's',
  strike: 's',
  del: 's',
  code: 'code',
  pre: 'pre',
  blockquote: 'blockquote',
}
const SAFE_HREF = /^(https?:|tg:|mailto:)/i
const BLOCK_TAGS = new Set(['p', 'div', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'tr', 'table', 'thead', 'tbody'])

/**
 * Convert rich-editor HTML into the subset Telegram's HTML parse mode accepts
 * (b, i, u, s, a, code, pre, blockquote; newlines instead of paragraphs / <br>).
 * Unsupported tags are unwrapped, images and scripts are dropped.
 */
export const toTelegramHtml = (html: string): string => {
  if (!html.trim()) return ''
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, 'text/html')

  const walk = (node: Node, listIndex?: { n: number; ordered: boolean }): string => {
    if (node.nodeType === Node.TEXT_NODE) return escapeHtml(node.textContent ?? '')
    if (node.nodeType !== Node.ELEMENT_NODE) return ''
    const el = node as Element
    const tag = el.tagName.toLowerCase()
    if (tag === 'script' || tag === 'style' || tag === 'img') return ''
    if (tag === 'br') return '\n'

    if (tag === 'ul' || tag === 'ol') {
      const ctx = { n: 0, ordered: tag === 'ol' }
      return Array.from(el.childNodes).map((c) => walk(c, ctx)).join('')
    }
    const inner = Array.from(el.childNodes).map((c) => walk(c)).join('')
    if (tag === 'li') {
      const ctx = listIndex ?? { n: 0, ordered: false }
      ctx.n += 1
      return `${ctx.ordered ? `${ctx.n}.` : '•'} ${inner.replace(/\n+$/, '')}\n`
    }
    if (tag === 'a') {
      const href = (el.getAttribute('href') ?? '').trim()
      return SAFE_HREF.test(href) ? `<a href="${escapeAttr(href)}">${inner}</a>` : inner
    }
    if (tag === 'td' || tag === 'th') return `${inner.replace(/\n+$/, '')} | `
    const mapped = INLINE_TAGS[tag]
    if (mapped) {
      const wrapped = `<${mapped}>${mapped === 'blockquote' || mapped === 'pre' ? inner.replace(/\n+$/, '') : inner}</${mapped}>`
      return mapped === 'blockquote' || mapped === 'pre' ? `${wrapped}\n` : wrapped
    }
    if (/^h[1-6]$/.test(tag)) return `<b>${inner.replace(/\n+$/, '')}</b>\n`
    if (tag === 'tr') return `${inner.replace(/ \| $/, '')}\n`
    if (BLOCK_TAGS.has(tag)) return `${inner.replace(/\n+$/, '')}\n`
    return inner
  }

  return Array.from(doc.body.childNodes)
    .map((c) => walk(c))
    .join('')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}
