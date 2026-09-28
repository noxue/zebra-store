import type { ResellerDomainData, ResellerManagementSnapshotData } from '@/api/types'
import type { BadgeTone } from '@/utils/status'
import {
  RESELLER_DOMAIN_STATUS_ACTIVE,
  RESELLER_DOMAIN_STATUS_PENDING_REVIEW,
  RESELLER_DOMAIN_VERIFICATION_FAILED,
  RESELLER_DOMAIN_VERIFICATION_PENDING,
  RESELLER_DOMAIN_VERIFICATION_VERIFIED,
} from './constants'

export type ResellerModuleKey = 'apply' | 'dashboard' | 'domains' | 'site' | 'products' | 'orders' | 'finance' | 'ledger' | 'withdraws'

export interface ResellerConsoleModuleState {
  visible: boolean
  enabled: boolean
  reason?: string
}

export type ResellerProfileStatus = 'not_opened' | 'pending_review' | 'active' | 'rejected' | 'disabled' | 'unknown'

export interface ResellerConsoleState {
  opened: boolean
  profileStatus: ResellerProfileStatus
  canApply: boolean
  modules: Record<ResellerModuleKey, ResellerConsoleModuleState>
}

const activeModules: ResellerModuleKey[] = ['dashboard', 'domains', 'site', 'products', 'orders', 'finance', 'ledger', 'withdraws']
const knownStatuses: ResellerProfileStatus[] = ['pending_review', 'active', 'rejected', 'disabled']

/** Module gating from the management snapshot (only `active` profiles unlock modules). */
export const getResellerConsoleState = (snapshot?: ResellerManagementSnapshotData | null): ResellerConsoleState => {
  const raw = snapshot?.profile?.status || (snapshot?.opened ? 'unknown' : 'not_opened')
  const profileStatus: ResellerProfileStatus = raw === 'not_opened' || (knownStatuses as string[]).includes(raw) ? (raw as ResellerProfileStatus) : 'unknown'
  const canApply = snapshot?.can_apply === true
  const active = profileStatus === 'active'
  const modules = {
    apply: { visible: true, enabled: canApply || !active },
  } as Record<ResellerModuleKey, ResellerConsoleModuleState>
  for (const key of activeModules) {
    modules[key] = { visible: true, enabled: active, reason: active ? undefined : profileStatus }
  }
  return { opened: snapshot?.opened === true, profileStatus, canApply, modules }
}

export const resolveResellerConsoleModule = (path: string): ResellerModuleKey => {
  const clean = String(path || '').split(/[?#]/)[0]?.replace(/\/+$/, '') || '/reseller'
  if (clean === '/reseller') return 'dashboard'
  const segment = clean.replace(/^\/reseller\/?/, '').split('/')[0] || ''
  const known: ResellerModuleKey[] = ['apply', 'domains', 'site', 'products', 'orders', 'finance', 'ledger', 'withdraws']
  return (known as string[]).includes(segment) ? (segment as ResellerModuleKey) : 'dashboard'
}

export const canRenderResellerConsoleModule = (path: string, state: ResellerConsoleState): boolean => {
  const mod = resolveResellerConsoleModule(path)
  if (mod === 'apply' || mod === 'dashboard') return true
  return state.modules[mod]?.enabled === true
}

export const formatResellerConsoleDate = (raw?: string | null): string => {
  if (!raw) return '-'
  const date = new Date(raw)
  if (Number.isNaN(date.getTime())) return raw
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

/** `1,234.50 USD`; keeps up to 8 decimals present in the input. */
export const formatResellerConsoleAmount = (amount?: string | number | null, currency?: string | null): string => {
  if (amount === undefined || amount === null || amount === '') return '-'
  const num = typeof amount === 'number' ? amount : Number(amount)
  let value: string
  if (Number.isFinite(num)) {
    const decimals = (String(amount).split('.')[1] || '').length
    value = num.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: Math.max(2, Math.min(decimals, 8)) })
  } else {
    value = String(amount)
  }
  return currency ? `${value} ${currency}` : value
}

export const resellerAmountSign = (amount?: string | number | null): 'positive' | 'negative' | 'zero' => {
  const num = typeof amount === 'number' ? amount : Number(amount)
  if (!Number.isFinite(num) || num === 0) return 'zero'
  return num > 0 ? 'positive' : 'negative'
}

export const resellerProfitStatusKey = (status?: string): 'credited' | 'pending' | 'unavailable' | 'unknown' => {
  if (status === 'credited' || status === 'pending' || status === 'unavailable') return status
  return 'unknown'
}

export const resellerProfitTone = (status?: string): BadgeTone => {
  if (status === 'credited') return 'success'
  if (status === 'pending') return 'warning'
  return 'neutral'
}

export const resellerOrderStatusTone = (status?: string): BadgeTone => {
  if (status === 'paid' || status === 'completed' || status === 'delivered') return 'success'
  if (status === 'pending_payment') return 'warning'
  if (status === 'refunded' || status === 'partially_refunded' || status === 'canceled') return 'neutral'
  return 'info'
}

/** Chart palette from design tokens (theme aware). */
export const RESELLER_CHART_PALETTE = [
  'var(--zs-primary)',
  'var(--zs-secondary)',
  'var(--zs-accent)',
  'var(--zs-gold)',
  'var(--zs-success)',
  'var(--zs-warning)',
  'var(--zs-danger)',
] as const

export const resellerOrderStatusColor = (status?: string): string => {
  switch (status) {
    case 'paid':
      return 'var(--zs-success)'
    case 'completed':
      return 'var(--zs-accent)'
    case 'delivered':
      return 'var(--zs-secondary)'
    case 'pending_payment':
      return 'var(--zs-gold)'
    case 'partially_refunded':
      return 'var(--zs-warning)'
    case 'refunded':
      return 'var(--zs-danger)'
    case 'canceled':
      return 'var(--zs-text-muted)'
    default:
      return 'var(--zs-primary)'
  }
}

export const resellerCurrencyColor = (index: number): string => RESELLER_CHART_PALETTE[index % RESELLER_CHART_PALETTE.length]

export const isActiveVerifiedDomain = (domain: Pick<ResellerDomainData, 'status' | 'verification_status'>): boolean =>
  domain.status === RESELLER_DOMAIN_STATUS_ACTIVE && domain.verification_status === RESELLER_DOMAIN_VERIFICATION_VERIFIED

/** Primary active+verified domain, else first active one. */
export const pickPrimaryDomain = (domains?: ResellerDomainData[] | null): ResellerDomainData | null => {
  const active = (domains || []).filter(isActiveVerifiedDomain)
  return active.find((d) => d.is_primary) || active[0] || null
}

export const isPendingDomain = (domain: Pick<ResellerDomainData, 'status' | 'verification_status'>): boolean =>
  domain.status === RESELLER_DOMAIN_STATUS_PENDING_REVIEW || domain.verification_status === RESELLER_DOMAIN_VERIFICATION_PENDING

export const domainVerificationKey = (status?: string): 'verified' | 'pending' | 'failed' | 'unknown' => {
  if (status === RESELLER_DOMAIN_VERIFICATION_VERIFIED) return 'verified'
  if (status === RESELLER_DOMAIN_VERIFICATION_PENDING) return 'pending'
  if (status === RESELLER_DOMAIN_VERIFICATION_FAILED) return 'failed'
  return 'unknown'
}

export const domainVerificationTone = (status?: string): BadgeTone => {
  const key = domainVerificationKey(status)
  if (key === 'verified') return 'success'
  if (key === 'unknown') return 'neutral'
  return 'warning'
}

export const domainStatusTone = (status?: string): BadgeTone => {
  if (status === RESELLER_DOMAIN_STATUS_ACTIVE) return 'success'
  if (status === RESELLER_DOMAIN_STATUS_PENDING_REVIEW) return 'warning'
  return 'neutral'
}
