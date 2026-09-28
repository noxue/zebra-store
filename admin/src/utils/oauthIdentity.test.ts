import { describe, expect, it } from 'vitest'
import type { AdminUserOAuthIdentity } from '@/api/types'
import {
  formatOAuthIdentityAccount,
  formatOAuthIdentityUsername,
  formatOAuthProviderLabel,
  managedOAuthProvider,
} from './oauthIdentity'

const identity = (overrides: Partial<AdminUserOAuthIdentity>): AdminUserOAuthIdentity => ({
  id: 1,
  provider: 'telegram',
  provider_user_id: '42',
  created_at: '2026-07-30T00:00:00Z',
  ...overrides,
})

describe('admin OAuth identity presentation', () => {
  it('renders Google with its product name and email without a Telegram @ prefix', () => {
    const google = identity({
      provider: 'GOOGLE',
      provider_user_id: 'google-subject',
      username: 'alice@gmail.com',
    })

    expect(formatOAuthProviderLabel(google.provider)).toBe('Google')
    expect(formatOAuthIdentityUsername(google)).toBe('alice@gmail.com')
    expect(formatOAuthIdentityAccount(google)).toBe('alice@gmail.com')
    expect(managedOAuthProvider(google)).toBe('google')
  })

  it('keeps Telegram usernames prefixed exactly once', () => {
    expect(formatOAuthIdentityUsername(identity({ username: 'alice' }))).toBe('@alice')
    expect(formatOAuthIdentityUsername(identity({ username: '@alice' }))).toBe('@alice')
  })

  it('falls back to provider ID and does not enable unknown-provider operations', () => {
    const unknown = identity({ provider: 'github', provider_user_id: 'user-7', username: '' })

    expect(formatOAuthIdentityAccount(unknown)).toBe('user-7')
    expect(managedOAuthProvider(unknown)).toBeNull()
  })
})
