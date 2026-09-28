import type { LocalizedText } from '@/api/types'

const FALLBACK_ORDER = ['zh-CN', 'zh-TW', 'en-US'] as const

/**
 * Resolves a `{zh-CN, zh-TW, en-US}` object for the locale, falling back
 * to zh-CN → zh-TW → en-US → any non-empty value. Plain strings pass through.
 */
export const localizedText = (value: LocalizedText | Record<string, unknown> | string | null | undefined, locale: string): string => {
  if (value === null || value === undefined) return ''
  if (typeof value === 'string') return value
  const record = value as Record<string, unknown>
  const candidates: unknown[] = [record[locale], ...FALLBACK_ORDER.map((code) => record[code]), ...Object.values(record)]
  for (const item of candidates) {
    if (typeof item === 'string' && item.trim()) return item.trim()
  }
  return ''
}

export const hasLocalizedText = (value: LocalizedText | Record<string, unknown> | null | undefined): boolean =>
  !!value && Object.values(value).some((item) => typeof item === 'string' && item.trim() !== '')
