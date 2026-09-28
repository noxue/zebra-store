export type DashboardRange = 'today' | '7d' | '30d' | 'custom'

export interface DashboardFilters {
  range: DashboardRange
  from: string
  to: string
}

/** Convert a `YYYY-MM-DD` local date into an ISO timestamp at start/end of day. */
export function makeRangeDate(raw: string, endOfDay: boolean): string | undefined {
  if (!raw) return undefined
  const date = new Date(`${raw}${endOfDay ? 'T23:59:59' : 'T00:00:00'}`)
  return Number.isNaN(date.getTime()) ? undefined : date.toISOString()
}

/** Query params for the dashboard endpoints; null when a custom range is incomplete. */
export function buildDashboardQuery(filters: DashboardFilters, tz: string, forceRefresh = false): Record<string, string | boolean> | null {
  const params: Record<string, string | boolean> = { range: filters.range, tz }
  if (filters.range === 'custom') {
    const from = makeRangeDate(filters.from, false)
    const to = makeRangeDate(filters.to, true)
    if (!from || !to) return null
    params.from = from
    params.to = to
  }
  if (forceRefresh) params.force_refresh = true
  return params
}

/** Bar size in percent of the max, with a 4% floor so zero bars stay visible (original behaviour). */
export function barPercent(value: number, max: number): number {
  const safeMax = Math.max(1, max)
  return Math.min(100, Math.max(4, Math.round((Math.max(0, value) / safeMax) * 100)))
}

export function shortDate(value?: string): string {
  if (!value) return '-'
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(value)
  if (m) return `${m[2]}/${m[3]}`
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleDateString(undefined, { month: '2-digit', day: '2-digit' })
}

const pad2 = (n: number) => String(n).padStart(2, '0')

/** `YYYY-MM-DD` of `d` in the browser's local timezone (not UTC, QA-A21). */
export function localDateString(d: Date): string {
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`
}

/** Default custom range = last 7 days ending today, in local dates (YYYY-MM-DD). */
export function defaultCustomRange(now = new Date()): { from: string; to: string } {
  const start = new Date(now)
  start.setDate(start.getDate() - 6)
  return { from: localDateString(start), to: localDateString(now) }
}

/** Dashboard quick links with the permission of the page they open (QA-A17). */
export const DASHBOARD_QUICK_ACTIONS = [
  { key: 'orders', labelKey: 'admin.nav.orders', path: '/orders', permission: 'GET:/admin/orders' },
  { key: 'payments', labelKey: 'admin.nav.payments', path: '/payments', permission: 'GET:/admin/payments' },
  { key: 'products', labelKey: 'admin.nav.products', path: '/products', permission: 'GET:/admin/products' },
  { key: 'cardSecrets', labelKey: 'admin.nav.cardSecrets', path: '/card-secrets', permission: 'GET:/admin/card-secrets' },
  { key: 'users', labelKey: 'admin.nav.users', path: '/users', permission: 'GET:/admin/users' },
] as const

export type DashboardQuickAction = (typeof DASHBOARD_QUICK_ACTIONS)[number]

export const visibleQuickActions = (can: (permission: string) => boolean): DashboardQuickAction[] => DASHBOARD_QUICK_ACTIONS.filter((a) => can(a.permission))
