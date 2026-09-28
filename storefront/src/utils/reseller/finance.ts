import type { ResellerLedgerData } from '@/api/types'
import type { BadgeTone } from '@/utils/status'
import {
  RESELLER_BALANCE_STATUS_DISABLED,
  RESELLER_BALANCE_STATUS_FROZEN_REVIEW,
  RESELLER_BALANCE_STATUS_NEGATIVE_BALANCE,
  RESELLER_BALANCE_STATUS_NORMAL,
  RESELLER_LEDGER_STATUS_AVAILABLE,
  RESELLER_LEDGER_STATUS_CANCELED,
  RESELLER_LEDGER_STATUS_LOCKED,
  RESELLER_LEDGER_STATUS_PENDING_CONFIRM,
  RESELLER_LEDGER_STATUS_WITHDRAWN,
  RESELLER_WITHDRAW_STATUS_PAID,
  RESELLER_WITHDRAW_STATUS_PENDING,
  RESELLER_WITHDRAW_STATUS_REJECTED,
} from './constants'
import { resellerAmountSign } from './console'

type ResellerFinanceStatusNamespace = 'profileStatusMap' | 'settlementStatusMap'

export interface ResellerFinanceStatusView {
  namespace: ResellerFinanceStatusNamespace
  key: string
  badgeTone: BadgeTone
}

const profileStatusKeyMap: Record<string, string> = {
  pending_review: 'pendingReview',
  active: 'active',
  rejected: 'rejected',
  disabled: 'disabled',
}
const settlementStatusKeyMap: Record<string, string> = {
  normal: 'normal',
  frozen: 'frozen',
  frozen_review: 'frozen',
  disabled: 'disabled',
}
const withdrawDisabledReasonKeyMap: Record<string, string> = {
  profile_inactive: 'profileInactive',
  settlement_unavailable: 'settlementUnavailable',
}
const ledgerTypeKeyMap: Record<string, string> = {
  order_profit: 'orderProfit',
  refund_deduct: 'refundDeduct',
  withdraw_lock: 'withdrawLock',
  manual_adjust: 'manualAdjust',
  withdraw_paid: 'withdrawPaid',
}
const ledgerStatusKeyMap: Record<string, string> = {
  [RESELLER_LEDGER_STATUS_PENDING_CONFIRM]: 'pendingConfirm',
  [RESELLER_LEDGER_STATUS_AVAILABLE]: 'available',
  [RESELLER_LEDGER_STATUS_LOCKED]: 'locked',
  [RESELLER_LEDGER_STATUS_WITHDRAWN]: 'withdrawn',
  [RESELLER_LEDGER_STATUS_CANCELED]: 'canceled',
}
const balanceStatusKeyMap: Record<string, string> = {
  [RESELLER_BALANCE_STATUS_NORMAL]: 'normal',
  [RESELLER_BALANCE_STATUS_NEGATIVE_BALANCE]: 'negativeBalance',
  [RESELLER_BALANCE_STATUS_FROZEN_REVIEW]: 'frozenReview',
  [RESELLER_BALANCE_STATUS_DISABLED]: 'disabled',
}
const withdrawStatusKeyMap: Record<string, string> = {
  [RESELLER_WITHDRAW_STATUS_PENDING]: 'pending',
  [RESELLER_WITHDRAW_STATUS_REJECTED]: 'rejected',
  [RESELLER_WITHDRAW_STATUS_PAID]: 'paid',
}

interface BalanceLike {
  currency: string
  available_amount: string
}

/** Balance with the largest available amount (there is no "main" settlement currency). */
export const pickPrimaryResellerBalance = <T extends BalanceLike>(balances?: T[] | null): T | null => {
  const list = (balances || []).filter(Boolean)
  if (list.length === 0) return null
  return list.reduce((best, cur) => ((Number(cur.available_amount) || 0) > (Number(best.available_amount) || 0) ? cur : best))
}

export const isResellerWithdrawEnabled = (dashboard?: { withdraw_enabled?: boolean } | null): boolean => dashboard?.withdraw_enabled === true

export const getResellerWithdrawDisabledReasonKey = (reason?: string): string =>
  (reason && withdrawDisabledReasonKeyMap[reason]) || 'default'

export const getResellerFinanceStatusView = (profile?: { status?: string; settlement_status?: string } | null): ResellerFinanceStatusView => {
  if (!profile) return { namespace: 'profileStatusMap', key: 'unknown', badgeTone: 'neutral' }
  const status = profile.status || ''
  if (status && status !== 'active') {
    return {
      namespace: 'profileStatusMap',
      key: profileStatusKeyMap[status] || status,
      badgeTone: status === 'pending_review' ? 'warning' : 'neutral',
    }
  }
  const settlement = profile.settlement_status || ''
  return {
    namespace: 'settlementStatusMap',
    key: settlementStatusKeyMap[settlement] || settlement || 'unknown',
    badgeTone: settlement === 'normal' ? 'success' : 'warning',
  }
}

export const getResellerLedgerTypeKey = (type?: string): string | null => (type && ledgerTypeKeyMap[type]) || null
export const getResellerLedgerStatusKey = (status?: string): string | null => (status && ledgerStatusKeyMap[status]) || null
export const getResellerBalanceStatusKey = (status?: string): string | null => (status && balanceStatusKeyMap[status]) || null
export const getResellerWithdrawStatusKey = (status?: string): string | null => (status && withdrawStatusKeyMap[status]) || null

export const ledgerStatusTone = (status?: string): BadgeTone => {
  if (status === RESELLER_LEDGER_STATUS_AVAILABLE) return 'success'
  if (status === RESELLER_LEDGER_STATUS_PENDING_CONFIRM || status === RESELLER_LEDGER_STATUS_LOCKED) return 'warning'
  return 'neutral'
}

export const balanceStatusTone = (status?: string): BadgeTone => {
  if (status === RESELLER_BALANCE_STATUS_NORMAL) return 'success'
  if (status === RESELLER_BALANCE_STATUS_NEGATIVE_BALANCE || status === RESELLER_BALANCE_STATUS_FROZEN_REVIEW) return 'warning'
  return 'neutral'
}

export const withdrawStatusTone = (status?: string): BadgeTone => {
  if (status === RESELLER_WITHDRAW_STATUS_PAID) return 'success'
  if (status === RESELLER_WITHDRAW_STATUS_PENDING) return 'warning'
  if (status === RESELLER_WITHDRAW_STATUS_REJECTED) return 'danger'
  return 'neutral'
}

export type LedgerDirection = 'income' | 'expense' | 'neutral'

export const ledgerDirection = (item: Pick<ResellerLedgerData, 'type' | 'amount'>): LedgerDirection => {
  if (item.type === 'order_profit') return 'income'
  if (item.type === 'refund_deduct' || item.type === 'withdraw_lock') return 'expense'
  const sign = resellerAmountSign(item.amount)
  if (sign === 'negative') return 'expense'
  if (sign === 'positive') return 'income'
  return 'neutral'
}

/** `+12.00 USD` / `−3.00 USD` */
export const ledgerAmountDisplay = (item: Pick<ResellerLedgerData, 'type' | 'amount' | 'currency'>): string => {
  const dir = ledgerDirection(item)
  const abs = Math.abs(Number(item.amount) || 0)
  const prefix = dir === 'income' ? '+' : dir === 'expense' ? '−' : ''
  return `${prefix}${abs.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${item.currency}`
}

/** True when `amount` exceeds the available balance for the selected currency. */
export const exceedsAvailable = (amount: string, available: number | null): boolean => {
  const value = Number(amount)
  if (!amount || Number.isNaN(value) || available === null) return false
  return value > available
}
