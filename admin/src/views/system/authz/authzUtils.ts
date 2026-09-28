import type { AdminAuthzAdmin, AdminAuthzPolicy, AdminAuthzRole, AdminPermissionCatalogItem } from '@/api/types'

export const POLICY_ACTIONS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', '*'] as const

export const AUDIT_ACTIONS = ['role_create', 'role_delete', 'policy_grant', 'policy_revoke', 'admin_roles_update'] as const

/** Casbin role subjects are stored as `role:<name>`; the prefix is hidden in the UI. */
export const stripRolePrefix = (role: string) => role.replace(/^role:/, '')

/**
 * Audit-log `role` filter: the backend matches the stored subject exactly, so a
 * name typed without the hidden prefix gets `role:` added (same rule as the
 * backend's `normalize_role`; QA-A22).
 */
export const auditRoleFilter = (input: string): string => {
  const trimmed = input.trim().replace(/ /g, '_')
  if (!trimmed) return ''
  return trimmed.startsWith('role:') ? trimmed : `role:${trimmed}`
}

export interface NormalizedRoles {
  roles: string[]
  immutable: Set<string>
}

/** `GET authz/roles?include_metadata=true` returns `{role, immutable}[]`; older backends return `string[]`. */
export function normalizeRoles(data: unknown): NormalizedRoles {
  const list: unknown[] = Array.isArray(data) ? data : []
  const roles: string[] = []
  const immutable = new Set<string>()
  for (const item of list) {
    if (typeof item === 'string') {
      const role = item.trim()
      if (role) roles.push(role)
      continue
    }
    if (item && typeof item === 'object' && typeof (item as AdminAuthzRole).role === 'string') {
      const role = (item as AdminAuthzRole).role.trim()
      if (!role) continue
      roles.push(role)
      if ((item as AdminAuthzRole).immutable) immutable.add(role)
    }
  }
  return { roles, immutable }
}

/** Admin list search: matches `${id} ${username}` (case-insensitive). */
export function filterAdmins(admins: AdminAuthzAdmin[], keyword: string): AdminAuthzAdmin[] {
  const kw = keyword.trim().toLowerCase()
  if (!kw) return admins
  return admins.filter((a) => `${a.id} ${a.username}`.toLowerCase().includes(kw))
}

/** Catalog search over module / method / object / permission. */
export function filterCatalog(items: AdminPermissionCatalogItem[], keyword: string): AdminPermissionCatalogItem[] {
  const kw = keyword.trim().toLowerCase()
  if (!kw) return items
  return items.filter((i) => `${i.module} ${i.method} ${i.object} ${i.permission}`.toLowerCase().includes(kw))
}

export interface CatalogGroup {
  module: string
  items: AdminPermissionCatalogItem[]
}

/** Group catalog entries by module (empty → `other`), modules sorted alphabetically. */
export function groupCatalog(items: AdminPermissionCatalogItem[]): CatalogGroup[] {
  const map = new Map<string, AdminPermissionCatalogItem[]>()
  for (const item of items) {
    const key = (item.module || 'other').trim() || 'other'
    const list = map.get(key) || []
    list.push(item)
    map.set(key, list)
  }
  return Array.from(map.entries())
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([module, list]) => ({ module, items: list }))
}

/** Whether a catalog entry is already covered by one of the role's policies (same object, same method or `*`). */
export function hasCoveredPolicy(policies: AdminAuthzPolicy[], item: AdminPermissionCatalogItem): boolean {
  return policies.some((p) => {
    if (p.object !== item.object) return false
    if (item.method === '*') return p.action === '*'
    return p.action === item.method || p.action === '*'
  })
}
