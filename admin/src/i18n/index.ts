import { createI18n } from 'vue-i18n'
import zhCN from './locales/zh-CN'
import zhTW from './locales/zh-TW'
import enUS from './locales/en-US'

export const SUPPORTED_LOCALES = ['zh-CN', 'zh-TW', 'en-US'] as const
export type AppLocale = (typeof SUPPORTED_LOCALES)[number]

/** Native language names, shown identically regardless of the current UI language. */
export const LOCALE_NATIVE_NAMES: Record<AppLocale, string> = {
  'zh-CN': '简体中文',
  'zh-TW': '繁體中文',
  'en-US': 'English',
}

/** Select options for the language switcher. */
export const localeSelectOptions = (): { value: AppLocale; label: string }[] =>
  SUPPORTED_LOCALES.map((code) => ({ value: code, label: LOCALE_NATIVE_NAMES[code] }))
export const LOCALE_STORAGE_KEY = 'admin_locale'

type MessageTree = { [key: string]: string | MessageTree }
export type ExtraMessages = Partial<Record<AppLocale, MessageTree>>

const isTree = (value: unknown): value is MessageTree =>
  typeof value === 'object' && value !== null && !Array.isArray(value)

export function deepMerge(target: MessageTree, source: MessageTree): MessageTree {
  const out: MessageTree = { ...target }
  for (const [key, value] of Object.entries(source)) {
    const prev = out[key]
    out[key] = isTree(prev) && isTree(value) ? deepMerge(prev, value) : value
  }
  return out
}

/** vue-i18n treats `@` as linked-message syntax; escape it like the original project. */
export function sanitizeLinkedMessage(value: MessageTree): MessageTree {
  const out: MessageTree = {}
  for (const [key, item] of Object.entries(value)) {
    out[key] = typeof item === 'string' ? item.replace(/@/g, "{'@'}") : sanitizeLinkedMessage(item)
  }
  return out
}

// Extra strings added by Zebra Store live in ./extra/*.ts (default export: ExtraMessages)
const extraModules = import.meta.glob<{ default: ExtraMessages }>('./extra/*.ts', { eager: true })

function build(locale: AppLocale, base: MessageTree): MessageTree {
  let merged = base
  for (const mod of Object.values(extraModules)) {
    const part = mod.default[locale]
    if (part) merged = deepMerge(merged, part)
  }
  return sanitizeLinkedMessage(merged)
}

export function readStoredLocale(): AppLocale {
  try {
    const saved = localStorage.getItem(LOCALE_STORAGE_KEY)
    if (saved && (SUPPORTED_LOCALES as readonly string[]).includes(saved)) return saved as AppLocale
  } catch {
    /* ignore */
  }
  return 'zh-CN'
}

const i18n = createI18n({
  legacy: false,
  locale: readStoredLocale(),
  fallbackLocale: 'zh-CN',
  missingWarn: false,
  fallbackWarn: false,
  messages: {
    'zh-CN': build('zh-CN', zhCN as MessageTree),
    'zh-TW': build('zh-TW', zhTW as MessageTree),
    'en-US': build('en-US', enUS as MessageTree),
  },
})

export function currentLocale(): AppLocale {
  return i18n.global.locale.value as AppLocale
}

export function setLocale(locale: AppLocale) {
  i18n.global.locale.value = locale
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale)
  } catch {
    /* ignore */
  }
  document.documentElement.lang = locale
}

export default i18n
