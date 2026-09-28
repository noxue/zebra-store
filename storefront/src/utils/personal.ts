import type { PaymentChannel, PublicMemberLevel, TelegramAuthPayload } from '@/api/types'
import type { BadgeTone } from './status'
import { amountToCents, calculateFeeCents, centsToAmount, rateToBasisPoints } from './money'

// ------------------------------------------------------------ member level

/** Next level after `current` by sort_order (first level when none). */
export const resolveNextLevel = (
  levels: PublicMemberLevel[],
  current: PublicMemberLevel | null,
): PublicMemberLevel | null => {
  const sorted = [...levels].sort((a, b) => a.sort_order - b.sort_order)
  if (!current) return sorted[0] ?? null
  const idx = sorted.findIndex((l) => l.id === current.id)
  if (idx < 0 || idx >= sorted.length - 1) return null
  return sorted[idx + 1] ?? null
}

export interface UpgradeProgress {
  rechargePercent: number | null
  spendPercent: number | null
  recharged: number
  spent: number
  rechargeThreshold: number
  spendThreshold: number
}

/** Progress toward `next` level thresholds (percent capped at 100, null when no threshold). */
export const computeUpgradeProgress = (
  next: PublicMemberLevel | null,
  totalRecharged: number | string | undefined,
  totalSpent: number | string | undefined,
): UpgradeProgress | null => {
  if (!next) return null
  const recharged = Number(totalRecharged || 0) || 0
  const spent = Number(totalSpent || 0) || 0
  const rechargeThreshold = Number(next.recharge_threshold || 0)
  const spendThreshold = Number(next.spend_threshold || 0)
  return {
    rechargePercent: rechargeThreshold > 0 ? Math.min(100, (recharged / rechargeThreshold) * 100) : null,
    spendPercent: spendThreshold > 0 ? Math.min(100, (spent / spendThreshold) * 100) : null,
    recharged,
    spent,
    rechargeThreshold,
    spendThreshold,
  }
}

/** discount_rate 95 → "9.5" (used with `{n}折`); null when there is no discount. */
export const discountNumber = (rate: number | undefined | null): string | null => {
  const value = Number(rate)
  if (!Number.isFinite(value) || value <= 0 || value >= 100) return null
  return String(Math.round(value) / 10)
}

export const isImageIcon = (icon: string | undefined | null): boolean =>
  !!icon && (icon.startsWith('/uploads/') || icon.startsWith('http'))

// ------------------------------------------------------------ affiliate

/** `origin + promotion_path`, falling back to `/?aff=CODE`; '' when no code. */
export const buildPromotionUrl = (origin: string, code: string | undefined, path?: string): string => {
  if (!code) return ''
  const p = (path || '').trim() || `/?aff=${code}`
  return `${origin}${p.startsWith('/') ? p : `/${p}`}`
}

export const formatPercent = (value: unknown): string => {
  const n = Number(value || 0)
  return Number.isFinite(n) ? `${n.toFixed(2)}%` : '0.00%'
}

export const commissionStatusTone = (status?: string): BadgeTone => {
  if (status === 'pending_confirm') return 'warning'
  if (status === 'available') return 'success'
  if (status === 'withdrawn') return 'info'
  return 'neutral'
}

export const commissionStatusKey = (status?: string): string | null => {
  const map: Record<string, string> = {
    pending_confirm: 'pendingConfirm',
    available: 'available',
    rejected: 'rejected',
    withdrawn: 'withdrawn',
  }
  return status && map[status] ? map[status] : null
}

export const withdrawStatusTone = (status?: string): BadgeTone => {
  if (status === 'pending_review') return 'warning'
  if (status === 'paid') return 'success'
  if (status === 'rejected') return 'danger'
  return 'neutral'
}

export const withdrawStatusKey = (status?: string): string | null => {
  const map: Record<string, string> = { pending_review: 'pendingReview', rejected: 'rejected', paid: 'paid' }
  return status && map[status] ? map[status] : null
}

// ------------------------------------------------------------ wallet / recharge

export const RECHARGE_STATUSES = ['pending', 'success', 'failed', 'expired'] as const

export const rechargeStatusTone = (status?: string): BadgeTone => {
  const s = String(status || '').toLowerCase()
  if (s === 'success') return 'success'
  if (s === 'failed' || s === 'expired') return 'danger'
  return 'warning'
}

const EPAY_ALLOWED_CHANNEL_TYPES = new Set(['wechat', 'wxpay', 'alipay', 'qqpay'])

export interface ChannelLimit {
  minCents: number | null
  maxCents: number | null
}

export const channelLimit = (channel: Pick<PaymentChannel, 'min_amount' | 'max_amount'>): ChannelLimit => {
  const min = amountToCents(channel.min_amount ?? '')
  const max = amountToCents(channel.max_amount ?? '')
  return { minCents: min !== null && min > 0 ? min : null, maxCents: max !== null && max > 0 ? max : null }
}

export const isChannelOutOfRange = (channel: Pick<PaymentChannel, 'min_amount' | 'max_amount'>, amountCents: number): boolean => {
  const { minCents, maxCents } = channelLimit(channel)
  return (minCents !== null && amountCents < minCents) || (maxCents !== null && amountCents > maxCents)
}

/**
 * Filters wallet recharge channels like the original: unsupported epay
 * sub-types, hidden out-of-range channels and channels not in
 * `wallet_recharge_channel_ids` (empty list = all allowed).
 */
export const filterRechargeChannels = (
  list: PaymentChannel[],
  amountCents: number,
  allowedIds: number[] | undefined,
): PaymentChannel[] => {
  const allowed = Array.isArray(allowedIds) && allowedIds.length > 0 ? new Set(allowedIds.map(Number)) : null
  return list.filter((ch) => {
    if (!Number.isFinite(Number(ch.id)) || Number(ch.id) <= 0) return false
    if (String(ch.provider_type || '').toLowerCase() === 'epay' && !EPAY_ALLOWED_CHANNEL_TYPES.has(String(ch.channel_type || '').toLowerCase())) {
      return false
    }
    if (ch.hide_amount_out_range && isChannelOutOfRange(ch, amountCents)) return false
    if (allowed && !allowed.has(Number(ch.id))) return false
    return true
  })
}

/** amount × fee_rate% + fixed_fee, as a decimal string. */
export const computeChannelFee = (amount: string, channel: Pick<PaymentChannel, 'fee_rate' | 'fixed_fee'> | null): string => {
  const cents = amountToCents(amount)
  if (!channel || cents === null || cents <= 0) return '0.00'
  const rate = rateToBasisPoints(channel.fee_rate) || 0
  const fixed = amountToCents(channel.fixed_fee) || 0
  return centsToAmount((calculateFeeCents(cents, rate) || 0) + fixed)
}

export const signedAmount = (direction: string, formatted: string): string => {
  if (formatted === '-') return formatted
  return direction === 'out' ? `-${formatted}` : `+${formatted}`
}

// ------------------------------------------------------------ api credential

export const maskSecret = (tail?: string): string => (tail ? `••••••••${tail}` : '••••••••••••')

// ------------------------------------------------------------ telegram

/** Validates the Telegram widget callback payload. */
export const buildTelegramPayload = (raw: unknown): TelegramAuthPayload | null => {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as Record<string, unknown>
  const id = Number(r.id)
  const authDate = Number(r.auth_date)
  const hash = String(r.hash || '').trim()
  if (!Number.isFinite(id) || id <= 0 || !Number.isFinite(authDate) || authDate <= 0 || !hash) return null
  const s = (v: unknown) => String(v || '').trim()
  return {
    id,
    first_name: s(r.first_name),
    last_name: s(r.last_name),
    username: s(r.username),
    photo_url: s(r.photo_url),
    auth_date: authDate,
    hash,
  }
}
