import { adminAPI } from '@/api/admin'
import type { AdminMemberLevel } from '@/api/types'
import type { TranslateFn } from '@/utils/status'
import { getLocalizedText } from '@/utils/format'

// ---------- sorting (Users list) ----------

export type UserSortColumn = 'wallet_balance' | 'created_at' | 'last_login_at'
export type SortDirection = '' | 'asc' | 'desc'
export interface UserSortState {
  by: UserSortColumn | ''
  order: SortDirection
}

/** Three-state cycle: none → desc → asc → none; clicking another column jumps to its desc. */
export const nextSortState = (current: UserSortState, column: UserSortColumn): UserSortState => {
  if (current.by !== column) return { by: column, order: 'desc' }
  if (current.order === 'desc') return { by: column, order: 'asc' }
  return { by: '', order: '' }
}

// ---------- currency ----------

/** Normalise a site currency code; anything that is not a 3-letter code falls back to CNY. */
export const normalizeCurrency = (raw: unknown): string => {
  const value = String(raw ?? 'CNY').trim().toUpperCase()
  return /^[A-Z]{3}$/.test(value) ? value : 'CNY'
}

/** Read `site_config.currency` like the original pages do. */
export const fetchSiteCurrency = async (): Promise<string> => {
  try {
    const res = await adminAPI.getSettings<Record<string, unknown>>({ key: 'site_config' })
    return normalizeCurrency(res.data?.currency)
  } catch {
    return 'CNY'
  }
}

/** Fetch all member levels (first 100, as the original). */
export const fetchAllMemberLevels = async (): Promise<AdminMemberLevel[]> => {
  try {
    const res = await adminAPI.getMemberLevels({ page: 1, page_size: 100 })
    return Array.isArray(res.data) ? res.data : []
  } catch {
    return []
  }
}

// ---------- labels ----------

export const LOCALE_OPTIONS = ['zh-CN', 'zh-TW', 'en-US'] as const

export const formatLocale = (t: TranslateFn, raw?: string) => {
  if (!raw) return '-'
  const map: Record<string, string> = {
    'zh-CN': t('admin.common.lang.zhCN'),
    'zh-TW': t('admin.common.lang.zhTW'),
    'en-US': t('admin.common.lang.enUS'),
  }
  return map[raw] || raw
}

/** "🥇 Gold" / "#3" (unknown level) / "-" (none). */
export const memberLevelLabel = (levelId: unknown, levels: Map<number, AdminMemberLevel>) => {
  const id = Number(levelId || 0)
  if (!id) return '-'
  const level = levels.get(id)
  if (!level) return `#${id}`
  const icon = level.icon && !isImagePath(level.icon) ? `${level.icon} ` : ''
  return icon + getLocalizedText(level.name)
}

/** A member-level icon is an uploaded image when it looks like a path/URL, otherwise an emoji. */
export const isImagePath = (val?: string | null) => !!val && val.includes('/')

export const formatScopeProducts = (products: unknown): string => {
  if (!Array.isArray(products) || products.length === 0) return '-'
  const names = products
    .map((item) => (item && typeof item === 'object' ? getLocalizedText((item as { title?: unknown }).title) : ''))
    .filter((name) => name)
  if (names.length === 0) return '-'
  if (names.length <= 3) return names.join(', ')
  return `${names.slice(0, 3).join(', ')}...`
}

// ---------- wallet adjust ----------

export type WalletAdjustError = 'invalidAmount' | 'remarkRequired'

/** Validate the admin wallet-adjust form; returns an error key or null. */
export const validateWalletAdjust = (amount: string, remark: string): WalletAdjustError | null => {
  const trimmed = amount.trim()
  const value = Number(trimmed)
  if (!trimmed || Number.isNaN(value) || value <= 0) return 'invalidAmount'
  if (!remark.trim()) return 'remarkRequired'
  return null
}

// ---------- wallet config ----------

/** Keep only positive numeric ids from the stored `wallet_config.recharge_channel_ids`. */
export const sanitizeChannelIds = (raw: unknown): number[] =>
  Array.isArray(raw) ? raw.filter((id): id is number => typeof id === 'number' && id > 0) : []

export const toggleId = (list: number[], id: number): number[] => (list.includes(id) ? list.filter((x) => x !== id) : [...list, id])
