import { describe, expect, it } from 'vitest'
import { NAV_GROUPS, isNavItemActive, resolveNav } from './nav'
import { hasPermission } from '@/utils/permission'

const identity = (k: string) => k

describe('sidebar nav', () => {
  it('filters items by permission and drops empty groups', () => {
    const can = (p?: string) => hasPermission(['GET:/admin/products'], p, false)
    const groups = resolveNav(NAV_GROUPS, identity, can, '')
    // products (商品列表) is shared by the product group and the marketing group (批发价); security has no permission
    expect(groups.map((g) => g.id)).toEqual(['products', 'marketing', 'system'])
    expect(groups[0]?.items.map((i) => i.to)).toEqual(['/products'])
    expect(groups[2]?.items.map((i) => i.to)).toEqual(['/security'])
  })

  it('searches group labels and item labels', () => {
    const labels: Record<string, string> = { 'admin.navGroups.orderManagement': '订单管理', 'admin.navItems.coupons': '优惠券' }
    const t = (k: string) => labels[k] ?? k
    const all = () => true
    expect(resolveNav(NAV_GROUPS, t, all, '订单').map((g) => g.id)).toEqual(['orders'])
    const coupon = resolveNav(NAV_GROUPS, t, all, '优惠')
    expect(coupon).toHaveLength(1)
    expect(coupon[0]?.items.map((i) => i.to)).toEqual(['/coupons'])
  })

  it('prefers the most specific active item', () => {
    const paths = NAV_GROUPS.flatMap((g) => g.items.map((i) => i.to))
    expect(isNavItemActive('/', '/', paths)).toBe(true)
    expect(isNavItemActive('/', '/orders', paths)).toBe(false)
    expect(isNavItemActive('/users', '/users/5', paths)).toBe(true)
    expect(isNavItemActive('/telegram-bot', '/telegram-bot/settings', paths)).toBe(false)
    expect(isNavItemActive('/telegram-bot/broadcasts', '/telegram-bot/broadcasts/3', paths)).toBe(true)
    expect(isNavItemActive('/settings', '/settings/notifications', paths)).toBe(false)
  })
})
