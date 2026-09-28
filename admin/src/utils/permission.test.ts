import { describe, expect, it } from 'vitest'
import { buildPermissionKeys, hasPermission, matchObject, normalizeObjectPath, parsePermissionKey } from './permission'

describe('permission matcher', () => {
  it('normalizes object paths', () => {
    expect(normalizeObjectPath('/api/v1/admin/products')).toBe('/admin/products')
    expect(normalizeObjectPath('admin/products')).toBe('/admin/products')
    expect(normalizeObjectPath('')).toBe('/')
    expect(normalizeObjectPath('/api/v1')).toBe('/')
  })

  it('parses permission keys', () => {
    expect(parsePermissionKey('get:/admin/orders')).toEqual({ action: 'GET', object: '/admin/orders' })
    expect(parsePermissionKey('broken')).toEqual({ action: '*', object: '/' })
  })

  it('matches * and :param wildcards', () => {
    expect(matchObject('/admin/users/12', '/admin/users/:id')).toBe(true)
    expect(matchObject('/admin/users/12/wallet', '/admin/users/:id')).toBe(false)
    expect(matchObject('/admin/users/12/wallet', '/admin/users/*')).toBe(true)
    expect(matchObject('/admin/anything', '*')).toBe(true)
    expect(matchObject('/admin/orders', '/admin/products')).toBe(false)
  })

  it('builds de-duplicated keys from policies', () => {
    const keys = buildPermissionKeys([
      { object: '/api/v1/admin/products', action: 'get' },
      { object: '/admin/products', action: 'GET' },
      { object: '/admin/orders', action: '*' },
    ])
    expect(keys).toEqual(['GET:/admin/products', '*:/admin/orders'])
  })

  it('checks method + object, super admin bypass and empty requirement', () => {
    const granted = ['GET:/admin/products', '*:/admin/orders/*', 'GET:/admin/users/:id']
    expect(hasPermission(granted, 'GET:/admin/products', false)).toBe(true)
    expect(hasPermission(granted, 'POST:/admin/products', false)).toBe(false)
    expect(hasPermission(granted, 'PATCH:/admin/orders/5', false)).toBe(true)
    expect(hasPermission(granted, 'GET:/admin/users/7', false)).toBe(true)
    expect(hasPermission(granted, 'GET:/admin/settings', false)).toBe(false)
    expect(hasPermission([], 'GET:/admin/settings', true)).toBe(true)
    expect(hasPermission([], undefined, false)).toBe(true)
  })
})
