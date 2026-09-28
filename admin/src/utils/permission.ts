/** Pure permission helpers (ported from original stores/auth.ts) — unit tested. */
export interface PolicyLike {
  object: string
  action: string
}

export function normalizeObjectPath(path: string): string {
  const normalized = String(path || '').trim()
  if (!normalized) return '/'
  if (normalized.startsWith('/api/v1/')) return normalized.replace('/api/v1', '')
  if (normalized === '/api/v1') return '/'
  return normalized.startsWith('/') ? normalized : `/${normalized}`
}

export function normalizePermissionKey(action: string, object: string): string {
  return `${String(action || '').trim().toUpperCase()}:${normalizeObjectPath(object)}`
}

export function parsePermissionKey(permission: string): { action: string; object: string } {
  const splitIndex = permission.indexOf(':')
  if (splitIndex <= 0) return { action: '*', object: '/' }
  return {
    action: permission.slice(0, splitIndex).trim().toUpperCase(),
    object: normalizeObjectPath(permission.slice(splitIndex + 1)),
  }
}

const escapeRegex = (input: string) => input.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** `granted` may use `*` (any chars) and `:param` (one path segment). */
export function matchObject(requiredObject: string, grantedObject: string): boolean {
  const required = normalizeObjectPath(requiredObject)
  const granted = normalizeObjectPath(grantedObject)
  if (granted === '*' || granted === '/*') return true
  const pattern = `^${escapeRegex(granted).replace(/\\\*/g, '.*').replace(/:[^/]+/g, '[^/]+')}$`
  return new RegExp(pattern).test(required)
}

export function buildPermissionKeys(policies: PolicyLike[]): string[] {
  return Array.from(new Set(policies.map((p) => normalizePermissionKey(p.action, p.object))))
}

export function hasPermission(granted: string[], permission: string | undefined, isSuper: boolean): boolean {
  if (!permission) return true
  if (isSuper) return true
  const required = parsePermissionKey(permission)
  return granted.some((item) => {
    const g = parsePermissionKey(item)
    if (g.action !== '*' && g.action !== required.action) return false
    return matchObject(required.object, g.object)
  })
}
