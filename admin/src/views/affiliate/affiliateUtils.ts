import type { AdminAffiliateSetting } from '@/api/types'

export const AFFILIATE_PROFILE_STATUS_ACTIVE = 'active'
export const AFFILIATE_PROFILE_STATUS_DISABLED = 'disabled'
export const AFFILIATE_COMMISSION_STATUSES = ['pending_confirm', 'available', 'rejected', 'withdrawn'] as const
export const AFFILIATE_WITHDRAW_STATUS_PENDING_REVIEW = 'pending_review'
export const AFFILIATE_WITHDRAW_STATUSES = ['pending_review', 'rejected', 'paid'] as const

/**
 * Row of `GET /admin/affiliates/users`. The Go backend returns `{profile, stats}` where `stats`
 * has no json tags (PascalCase keys) — hence the dual-key lookups below, same as the original.
 */
export interface AffiliateUserRow {
  id?: number
  user_id?: number
  status?: string
  created_at?: string
  profile?: {
    id?: number
    user_id?: number
    code?: string
    affiliate_code?: string
    status?: string
    created_at?: string
    user?: { id?: number; email?: string; display_name?: string }
  }
  stats?: Record<string, unknown>
}

const parseNumber = (value: unknown, fallback = 0) => {
  const n = Number(value)
  return Number.isFinite(n) ? n : fallback
}

export const pickStatNumber = (stats: Record<string, unknown> | undefined, camelKey: string, snakeKey: string) =>
  parseNumber(stats?.[snakeKey] ?? stats?.[camelKey], 0)

export const pickStatAmount = (stats: Record<string, unknown> | undefined, camelKey: string, snakeKey: string) => {
  const value = stats?.[snakeKey] ?? stats?.[camelKey]
  if (value === null || value === undefined || value === '') return '0.00'
  return String(value)
}

export const conversionRateText = (stats: Record<string, unknown> | undefined) =>
  `${pickStatNumber(stats, 'ConversionRate', 'conversion_rate').toFixed(2)}%`

export const resolveProfileID = (row: AffiliateUserRow) => Number(row.profile?.id || row.id || 0)
export const resolveUserID = (row: AffiliateUserRow) => Number(row.profile?.user_id || row.user_id || 0)
export const resolveProfileStatus = (row: AffiliateUserRow) => String(row.profile?.status || row.status || '').trim()

// ---- settings ----
export const normalizeNumber = (value: unknown, fallback: number) => {
  if (value === null || value === undefined || value === '') return fallback
  const n = Number(value)
  return Number.isNaN(n) ? fallback : n
}

export const clampNumber = (value: unknown, min: number, max: number, fallback: number) => {
  const n = normalizeNumber(value, fallback)
  return Math.min(Math.max(n, min), max)
}

export const splitChannels = (raw: string) =>
  raw
    .split(/\r?\n|,/)
    .map((s) => s.trim())
    .filter((s) => s !== '')

export const joinChannels = (items: unknown) =>
  Array.isArray(items)
    ? items
        .map((s) => String(s ?? '').trim())
        .filter((s) => s !== '')
        .join('\n')
    : ''

export interface AffiliateSettingsForm {
  enabled: boolean
  commission_rate: number | ''
  confirm_days: number | ''
  min_withdraw_amount: number | ''
  withdraw_channels_text: string
}

export const settingsToForm = (data: Partial<AdminAffiliateSetting> | null | undefined, fallback?: AdminAffiliateSetting): AffiliateSettingsForm => ({
  enabled: Boolean(data?.enabled),
  commission_rate: clampNumber(data?.commission_rate, 0, 100, fallback?.commission_rate ?? 0),
  confirm_days: clampNumber(data?.confirm_days, 0, 3650, fallback?.confirm_days ?? 0),
  min_withdraw_amount: Math.max(normalizeNumber(data?.min_withdraw_amount, fallback?.min_withdraw_amount ?? 0), 0),
  withdraw_channels_text: joinChannels(data?.withdraw_channels),
})

/** Payload identical to the original (JSON numbers, channels split by newline or comma). */
export const formToSettings = (form: AffiliateSettingsForm): AdminAffiliateSetting => ({
  enabled: form.enabled,
  commission_rate: clampNumber(form.commission_rate, 0, 100, 0),
  confirm_days: clampNumber(form.confirm_days, 0, 3650, 0),
  min_withdraw_amount: Math.max(normalizeNumber(form.min_withdraw_amount, 0), 0),
  withdraw_channels: splitChannels(form.withdraw_channels_text),
})
