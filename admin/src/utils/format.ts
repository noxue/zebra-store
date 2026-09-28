import i18n from '@/i18n'
import type { Money } from '@/api/types'

export const toRFC3339 = (raw?: string) => {
  if (!raw) return undefined
  const date = new Date(raw)
  if (Number.isNaN(date.getTime())) return undefined
  return date.toISOString()
}

/** Convert an ISO timestamp into the value format of <input type="datetime-local">. */
export const toDateTimeLocal = (raw?: string | null) => {
  if (!raw) return ''
  const date = new Date(raw)
  if (Number.isNaN(date.getTime())) return ''
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export const formatDate = (raw?: string | null) => {
  if (!raw) return ''
  const date = new Date(raw)
  if (Number.isNaN(date.getTime())) return raw
  return date.toLocaleString()
}

/** `formatMoney('12.3', 'CNY') -> '12.3 CNY'`, same contract as the original project. */
export const formatMoney = (amount?: Money | null, currency?: string) => {
  if (amount === null || amount === undefined || amount === '') return '-'
  if (!currency) return String(amount)
  return `${amount} ${currency}`
}

/** Normalise a money value into a two-decimal string ("12.30"); invalid input yields "0.00". */
export const toMoneyString = (amount?: Money | null): string => {
  if (amount === null || amount === undefined || amount === '') return '0.00'
  const value = typeof amount === 'number' ? amount : Number(String(amount).trim())
  if (!Number.isFinite(value)) return '0.00'
  const cents = Math.round(Math.abs(value) * 100 + Number.EPSILON * 100)
  const sign = value < 0 && cents !== 0 ? '-' : ''
  return `${sign}${Math.floor(cents / 100)}.${String(cents % 100).padStart(2, '0')}`
}

export const hasPositiveAmount = (amount?: Money | null) => {
  if (amount === null || amount === undefined || amount === '') return false
  const value = Number(amount)
  return !Number.isNaN(value) && value > 0
}

/** Format a ratio/percent string coming from the backend ("12.5" -> "12.50%"). */
export const formatPercent = (value?: Money | null) => {
  if (value === null || value === undefined || value === '') return '-'
  const n = Number(value)
  if (!Number.isFinite(n)) return String(value)
  return `${n.toFixed(2)}%`
}

const resolveI18nLocale = () => String(i18n.global.locale.value || '').trim()

const buildLocaleCandidates = (locale: string) => {
  const normalized = locale.replace('_', '-')
  const lower = normalized.toLowerCase()
  if (!lower) return [] as string[]
  const list = new Set<string>([normalized])
  if (lower.startsWith('zh-cn') || lower === 'zh') {
    list.add('zh-CN')
    list.add('zh')
  }
  if (lower.startsWith('zh-tw') || lower.startsWith('zh-hk') || lower.startsWith('zh-mo')) {
    list.add('zh-TW')
  }
  if (lower.startsWith('en')) {
    list.add('en-US')
    list.add('en')
  }
  return Array.from(list)
}

/** Pick the best text of a `{zh-CN, zh-TW, en-US}` object for the current (or given) locale. */
export const getLocalizedText = (value: unknown, locale = resolveI18nLocale()): string => {
  if (value === null || value === undefined) return ''
  if (typeof value === 'string') return value
  if (typeof value !== 'object') return String(value)
  const source = value as Record<string, unknown>
  for (const key of buildLocaleCandidates(locale)) {
    const val = source[key]
    if (val !== undefined && val !== null && String(val).trim() !== '') return String(val)
  }
  const first = source['zh-CN'] || source['zh-TW'] || source['en-US'] || Object.values(source)[0] || ''
  return String(first)
}

export const emptyLocalized = (): Record<'zh-CN' | 'zh-TW' | 'en-US', string> => ({ 'zh-CN': '', 'zh-TW': '', 'en-US': '' })

/** Normalise any localized payload into a full 3-locale object for editing. */
export const toLocalizedForm = (value: unknown): Record<'zh-CN' | 'zh-TW' | 'en-US', string> => {
  const out = emptyLocalized()
  if (typeof value === 'string') {
    out['zh-CN'] = value
    return out
  }
  if (value && typeof value === 'object') {
    const src = value as Record<string, unknown>
    for (const key of Object.keys(out) as Array<keyof typeof out>) {
      out[key] = typeof src[key] === 'string' ? (src[key] as string) : ''
    }
  }
  return out
}

/** Build list query params: drop empty strings/undefined and the `__all__` sentinel. */
export const cleanParams = (params: Record<string, unknown>): Record<string, string | number | boolean> => {
  const out: Record<string, string | number | boolean> = {}
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === '' || value === '__all__') continue
    if (typeof value === 'string') {
      const trimmed = value.trim()
      if (trimmed) out[key] = trimmed
    } else if (typeof value === 'number' || typeof value === 'boolean') {
      out[key] = value
    } else {
      out[key] = String(value)
    }
  }
  return out
}

/** Label/value separator for inline "label: value" rows — full-width only in Chinese (QA-A23). */
export const labelSeparator = (locale: string): string => (locale.toLowerCase().startsWith('zh') ? '：' : ': ')
