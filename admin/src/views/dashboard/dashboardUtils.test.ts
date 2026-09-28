import { describe, expect, it } from 'vitest'
import { barPercent, buildDashboardQuery, defaultCustomRange, shortDate } from './dashboardUtils'

describe('dashboard utils', () => {
  it('builds preset range queries with timezone', () => {
    expect(buildDashboardQuery({ range: '7d', from: '', to: '' }, 'Asia/Shanghai')).toEqual({ range: '7d', tz: 'Asia/Shanghai' })
    expect(buildDashboardQuery({ range: 'today', from: '', to: '' }, 'UTC', true)).toEqual({ range: 'today', tz: 'UTC', force_refresh: true })
  })

  it('requires both ends of a custom range', () => {
    expect(buildDashboardQuery({ range: 'custom', from: '2026-01-01', to: '' }, 'UTC')).toBeNull()
    const q = buildDashboardQuery({ range: 'custom', from: '2026-01-01', to: '2026-01-07' }, 'UTC')
    expect(q?.from).toBe(new Date('2026-01-01T00:00:00').toISOString())
    expect(q?.to).toBe(new Date('2026-01-07T23:59:59').toISOString())
  })

  it('clamps bar heights with a 4% floor', () => {
    expect(barPercent(0, 10)).toBe(4)
    expect(barPercent(5, 10)).toBe(50)
    expect(barPercent(10, 10)).toBe(100)
    expect(barPercent(3, 0)).toBe(100)
  })

  it('formats short dates and default range', () => {
    expect(shortDate('2026-09-18')).toBe('09/18')
    expect(shortDate(undefined)).toBe('-')
    expect(defaultCustomRange(new Date('2026-09-24T12:00:00Z'))).toEqual({ from: '2026-09-18', to: '2026-09-24' })
  })

  it('QA-A21 default custom range uses the local calendar day, not UTC', () => {
    // 01:30 local time: in UTC+ zones toISOString() would still be the previous day.
    const earlyMorning = new Date(2026, 8, 26, 1, 30, 0)
    expect(defaultCustomRange(earlyMorning)).toEqual({ from: '2026-09-20', to: '2026-09-26' })
    const lateNight = new Date(2026, 8, 26, 23, 30, 0)
    expect(defaultCustomRange(lateNight)).toEqual({ from: '2026-09-20', to: '2026-09-26' })
  })
})

describe('dashboard quick actions', () => {
  it('QA-A17 only shows quick links the role can open', async () => {
    const { visibleQuickActions } = await import('./dashboardUtils')
    const { hasPermission } = await import('@/utils/permission')
    const granted = ['GET:/admin/orders', 'GET:/admin/dashboard/overview']
    expect(visibleQuickActions((p) => hasPermission(granted, p, false)).map((a) => a.path)).toEqual(['/orders'])
    expect(visibleQuickActions((p) => hasPermission([], p, true))).toHaveLength(5)
  })
})
