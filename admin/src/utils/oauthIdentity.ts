import type { AdminUserOAuthIdentity } from '@/api/types'

export type ManagedOAuthProvider = 'telegram' | 'google'

export const normalizeOAuthProvider = (provider?: string) => provider?.trim().toLowerCase() || ''

export const managedOAuthProvider = (identity: AdminUserOAuthIdentity): ManagedOAuthProvider | null => {
  const provider = normalizeOAuthProvider(identity.provider)
  return provider === 'telegram' || provider === 'google' ? provider : null
}

export const formatOAuthProviderLabel = (provider?: string) => {
  const normalized = normalizeOAuthProvider(provider)
  if (normalized === 'telegram') return 'Telegram'
  if (normalized === 'google') return 'Google'
  return normalized || '-'
}

export const formatOAuthIdentityUsername = (identity: AdminUserOAuthIdentity) => {
  const username = identity.username?.trim()
  if (!username) return '-'
  if (normalizeOAuthProvider(identity.provider) === 'telegram') {
    return username.startsWith('@') ? username : `@${username}`
  }
  return username
}

export const formatOAuthIdentityAccount = (identity: AdminUserOAuthIdentity) => {
  const username = formatOAuthIdentityUsername(identity)
  if (username !== '-') return username
  return identity.provider_user_id?.trim() || formatOAuthProviderLabel(identity.provider)
}
