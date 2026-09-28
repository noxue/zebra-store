import { describe, expect, it } from 'vitest'
import type { AdminAuthzAdmin, AdminPermissionCatalogItem } from '@/api/types'
import { filterAdmins, filterCatalog, groupCatalog, hasCoveredPolicy, normalizeRoles, stripRolePrefix } from './authzUtils'

const cat = (module: string, method: string, object: string): AdminPermissionCatalogItem => ({ module, method, object, permission: `${method}:${object}` })

describe('authzUtils', () => {
  it('strips only a leading role: prefix', () => {
    expect(stripRolePrefix('role:finance')).toBe('finance')
    expect(stripRolePrefix('finance')).toBe('finance')
    expect(stripRolePrefix('x role:y')).toBe('x role:y')
  })

  it('normalizes role metadata and legacy string lists', () => {
    const r = normalizeRoles([{ role: ' role:a ', immutable: true }, { role: 'role:b', immutable: false }, { role: '' }, 3])
    expect(r.roles).toEqual(['role:a', 'role:b'])
    expect([...r.immutable]).toEqual(['role:a'])
    expect(normalizeRoles(['role:x', ' ']).roles).toEqual(['role:x'])
    expect(normalizeRoles(null).roles).toEqual([])
  })

  it('filters admins by id or username', () => {
    const admins = [
      { id: 1, username: 'Admin', is_super: true },
      { id: 12, username: 'ops', is_super: false },
    ] as AdminAuthzAdmin[]
    expect(filterAdmins(admins, 'admin').map((a) => a.id)).toEqual([1])
    expect(filterAdmins(admins, '12').map((a) => a.id)).toEqual([12])
    expect(filterAdmins(admins, '  ')).toHaveLength(2)
  })

  it('filters and groups the catalog', () => {
    const items = [cat('orders', 'GET', '/admin/orders'), cat('', 'GET', '/admin/x'), cat('2FA', 'POST', '/admin/2fa/enable')]
    expect(filterCatalog(items, 'orders get')).toHaveLength(1)
    expect(filterCatalog(items, 'get orders')).toHaveLength(0)
    expect(filterCatalog(items, 'ORDERS')).toHaveLength(1)
    expect(groupCatalog(items).map((g) => g.module)).toEqual(['2FA', 'orders', 'other'])
  })

  it('detects covered policies incl. wildcard', () => {
    const pol = (object: string, action: string) => ({ subject: 'role:a', object, action })
    expect(hasCoveredPolicy([pol('/admin/orders', 'GET')], cat('m', 'GET', '/admin/orders'))).toBe(true)
    expect(hasCoveredPolicy([pol('/admin/orders', '*')], cat('m', 'POST', '/admin/orders'))).toBe(true)
    expect(hasCoveredPolicy([pol('/admin/orders', 'GET')], cat('m', '*', '/admin/orders'))).toBe(false)
    expect(hasCoveredPolicy([pol('/admin/orders/:id', 'GET')], cat('m', 'GET', '/admin/orders'))).toBe(false)
  })
})

describe('audit log role filter', () => {
  it('QA-A22 adds the hidden role: prefix to a bare role name', async () => {
    const { auditRoleFilter } = await import('./authzUtils')
    expect(auditRoleFilter('operations')).toBe('role:operations')
    expect(auditRoleFilter(' role:operations ')).toBe('role:operations')
    expect(auditRoleFilter('my role')).toBe('role:my_role')
    expect(auditRoleFilter('  ')).toBe('')
  })
})
