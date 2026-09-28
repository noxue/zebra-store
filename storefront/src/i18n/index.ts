import { createI18n } from 'vue-i18n'
import zhCN from './locales/zh-CN.json'

export const SUPPORTED_LOCALES = ['zh-CN', 'zh-TW', 'en-US'] as const
export type AppLocale = (typeof SUPPORTED_LOCALES)[number]
export const LOCALE_LABELS: Record<AppLocale, string> = {
  'zh-CN': '简体中文',
  'zh-TW': '繁體中文',
  'en-US': 'English',
}

type Messages = typeof zhCN
type MessageTree = { [key: string]: string | MessageTree }

const extraModules = import.meta.glob<{ default: Partial<Record<string, MessageTree>> }>('./extra/*.ts', { eager: true })

const isTree = (v: unknown): v is MessageTree => typeof v === 'object' && v !== null && !Array.isArray(v)

/** Deep merge `add` into `base` (returns a new object). */
export const mergeMessages = (base: MessageTree, add: MessageTree): MessageTree => {
  const out: MessageTree = { ...base }
  for (const [key, value] of Object.entries(add)) {
    const current = out[key]
    out[key] = isTree(current) && isTree(value) ? mergeMessages(current, value) : value
  }
  return out
}

const withExtras = (locale: string, messages: Messages): Messages => {
  let merged = messages as unknown as MessageTree
  for (const mod of Object.values(extraModules)) {
    const add = mod.default?.[locale]
    if (add) merged = mergeMessages(merged, add)
  }
  return merged as unknown as Messages
}

const loaders: Record<string, () => Promise<{ default: Messages }>> = {
  'zh-TW': () => import('./locales/zh-TW.json') as Promise<{ default: Messages }>,
  'en-US': () => import('./locales/en-US.json') as Promise<{ default: Messages }>,
}
const loaded = new Set<string>(['zh-CN'])

export const isSupportedLocale = (value: string): value is AppLocale =>
  (SUPPORTED_LOCALES as readonly string[]).includes(value)

/** localStorage `locale` > browser language > zh-CN (same as original). */
export function detectLocale(): AppLocale {
  let saved: string | null
  try {
    saved = localStorage.getItem('locale')
  } catch {
    saved = null
  }
  if (saved && isSupportedLocale(saved)) return saved
  const browser = typeof navigator !== 'undefined' ? navigator.language || '' : ''
  if (isSupportedLocale(browser)) return browser
  const prefix = browser.split('-')[0]
  if (prefix === 'zh') {
    return /TW|HK|Hant/.test(browser) ? 'zh-TW' : 'zh-CN'
  }
  if (prefix === 'en') return 'en-US'
  return 'zh-CN'
}

const i18n = createI18n<[Messages], string, false>({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: { 'zh-CN': withExtras('zh-CN', zhCN) },
  missingWarn: false,
  fallbackWarn: false,
})

async function loadLocaleMessages(locale: string): Promise<void> {
  if (loaded.has(locale)) return
  const loader = loaders[locale]
  if (!loader) return
  const mod = await loader()
  i18n.global.setLocaleMessage(locale, withExtras(locale, mod.default))
  loaded.add(locale)
}

let pending = ''
/** Loads the pack first, then switches (keeps current locale on failure). */
export async function setI18nLocale(locale: string): Promise<void> {
  if (!isSupportedLocale(locale)) return
  pending = locale
  try {
    await loadLocaleMessages(locale)
  } catch {
    return
  }
  if (pending !== locale) return
  i18n.global.locale.value = locale
  if (typeof document !== 'undefined') document.documentElement.lang = locale
}

export default i18n
