import { describe, expect, it } from 'vitest'
import {
  buildGoogleRedirectCredentialCallbackURL,
  consumeGoogleRedirectIntent,
  createGoogleRedirectIntent,
  createGoogleRedirectPreparedIntent,
  GOOGLE_REDIRECT_INTENT_STORAGE_KEY,
  GOOGLE_REDIRECT_INTENT_TTL_MS,
  normalizeReturnPath,
  parseGoogleRedirectCallbackQuery,
  resolveGoogleRedirectIntentRefreshDelay,
  shouldResumeGoogleRedirect2FA,
  storeGoogleRedirectIntent,
  type StorageLike,
} from '@/utils/auth/googleRedirect'
import { resolveGoogleButtonLocale, resolveGoogleButtonWidth, resolveGoogleIdentityUXMode } from '@/utils/auth/googleIdentity'
import { buildTelegramMiniAppEntryLink, buildTelegramPayload } from '@/utils/auth/telegram'
import { composeEmail, getEmailDomain, getPasswordStrength, isValidEmail, normalizeEmailDomains, secondsUntil } from '@/utils/auth/validation'
import { formatPostDate } from '@/composables/usePostList'

const memoryStorage = (): StorageLike & { data: Map<string, string> } => {
  const data = new Map<string, string>()
  return {
    data,
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => void data.set(k, v),
    removeItem: (k) => void data.delete(k),
  }
}

describe('password strength', () => {
  it('rates short passwords weak', () => {
    expect(getPasswordStrength('')).toBe('weak')
    expect(getPasswordStrength('abc12')).toBe('weak')
  })
  it('rates medium and strong passwords', () => {
    expect(getPasswordStrength('abcdefgh1')).toBe('weak')
    expect(getPasswordStrength('Abcdefgh1')).toBe('medium')
    expect(getPasswordStrength('Abcdefgh1!xyz')).toBe('strong')
  })
})

describe('email helpers', () => {
  it('validates emails', () => {
    expect(isValidEmail('u1@test.com')).toBe(true)
    expect(isValidEmail(' u1@test.com ')).toBe(true)
    expect(isValidEmail('u1@test')).toBe(false)
    expect(isValidEmail('no at.com')).toBe(false)
  })
  it('normalizes allow-listed domains', () => {
    expect(normalizeEmailDomains(['@Gmail.com', 'gmail.com', ' qq.com ', '', null])).toEqual(['gmail.com', 'qq.com'])
    expect(normalizeEmailDomains('gmail.com')).toEqual([])
  })
  it('extracts domain and composes email', () => {
    expect(getEmailDomain('A@B.COM')).toBe('b.com')
    expect(getEmailDomain('@b.com')).toBe('')
    expect(getEmailDomain('a@')).toBe('')
    expect(composeEmail(' alice ', 'qq.com')).toBe('alice@qq.com')
    expect(composeEmail('', 'qq.com')).toBe('')
  })
})

describe('return path sanitising', () => {
  it('accepts only same-site absolute paths', () => {
    expect(normalizeReturnPath('/me/wallet')).toBe('/me/wallet')
    expect(normalizeReturnPath('//evil.com')).toBe('/me/orders')
    expect(normalizeReturnPath('https://evil.com')).toBe('/me/orders')
    expect(normalizeReturnPath('/\\evil')).toBe('/me/orders')
    expect(normalizeReturnPath(undefined)).toBe('/me/orders')
    expect(normalizeReturnPath(['/x'])).toBe('/me/orders')
  })
  it('bind intents always return to security page', () => {
    expect(createGoogleRedirectIntent('bind', '/me/wallet', 1).returnPath).toBe('/me/security')
    expect(createGoogleRedirectIntent('login', '/cart', 1).returnPath).toBe('/cart')
  })
})

describe('google redirect intent storage', () => {
  it('is read once and expires after the TTL', () => {
    const storage = memoryStorage()
    storeGoogleRedirectIntent(storage, createGoogleRedirectIntent('login', '/cart', 1000))
    expect(consumeGoogleRedirectIntent(storage, 2000)).toEqual({ flow: 'login', returnPath: '/cart', issuedAt: 1000 })
    expect(storage.data.has(GOOGLE_REDIRECT_INTENT_STORAGE_KEY)).toBe(false)
    expect(consumeGoogleRedirectIntent(storage, 2000)).toBeNull()
    storeGoogleRedirectIntent(storage, createGoogleRedirectIntent('login', '/cart', 1000))
    expect(consumeGoogleRedirectIntent(storage, 1000 + GOOGLE_REDIRECT_INTENT_TTL_MS)).toBeNull()
  })
  it('rejects malformed payloads', () => {
    const storage = memoryStorage()
    storage.setItem(GOOGLE_REDIRECT_INTENT_STORAGE_KEY, '{"flow":"hack","issuedAt":1}')
    expect(consumeGoogleRedirectIntent(storage, 2)).toBeNull()
    storage.setItem(GOOGLE_REDIRECT_INTENT_STORAGE_KEY, 'not json')
    expect(consumeGoogleRedirectIntent(storage, 2)).toBeNull()
  })
  it('computes refresh delay (8 minutes after issue)', () => {
    expect(resolveGoogleRedirectIntentRefreshDelay(0, 60_000)).toBe(7 * 60_000)
    expect(resolveGoogleRedirectIntentRefreshDelay(0, 9 * 60_000)).toBe(0)
  })
  it('validates prepared state', () => {
    const state = 'A'.repeat(42) + 'Q'
    expect(createGoogleRedirectPreparedIntent({ state }, 5)).toEqual({ state, issuedAt: 5 })
    expect(createGoogleRedirectPreparedIntent({ state: 'short' }, 5)).toBeNull()
    expect(createGoogleRedirectPreparedIntent(null, 5)).toBeNull()
  })
})

describe('google redirect callback query', () => {
  it('allows only flow and known error keys', () => {
    expect(parseGoogleRedirectCallbackQuery({ flow: 'login' })).toEqual({ flow: 'login', error: null })
    expect(parseGoogleRedirectCallbackQuery({ flow: 'bind', error: 'csrf_mismatch' })).toEqual({ flow: 'bind', error: 'csrf_mismatch' })
    expect(parseGoogleRedirectCallbackQuery({ flow: 'login', credential: 'x' })).toBeNull()
    expect(parseGoogleRedirectCallbackQuery({ flow: 'other' })).toBeNull()
    expect(parseGoogleRedirectCallbackQuery({ flow: 'login', error: 'weird' })).toBeNull()
  })
  it('resumes 2FA only with marker and challenge token', () => {
    expect(shouldResumeGoogleRedirect2FA('1', 'tok')).toBe(true)
    expect(shouldResumeGoogleRedirect2FA('1', ' ')).toBe(false)
    expect(shouldResumeGoogleRedirect2FA(undefined, 'tok')).toBe(false)
  })
  it('builds the backend credential callback URL', () => {
    expect(buildGoogleRedirectCredentialCallbackURL('', 'https://shop.test')).toBe('https://shop.test/api/v1/auth/google/redirect/callback')
    expect(buildGoogleRedirectCredentialCallbackURL('/base/', 'https://shop.test')).toBe('https://shop.test/base/api/v1/auth/google/redirect/callback')
    expect(() => buildGoogleRedirectCredentialCallbackURL('https://api.other', 'https://shop.test')).toThrow()
  })
})

describe('google identity helpers', () => {
  it('uses redirect mode on iOS / iPadOS', () => {
    expect(resolveGoogleIdentityUXMode({ userAgent: 'Mozilla (iPhone)' })).toBe('redirect')
    expect(resolveGoogleIdentityUXMode({ userAgent: 'Macintosh', maxTouchPoints: 5 })).toBe('redirect')
    expect(resolveGoogleIdentityUXMode({ userAgent: 'Windows NT' })).toBe('popup')
  })
  it('clamps width and maps locales', () => {
    expect(resolveGoogleButtonWidth(520)).toBe(400)
    expect(resolveGoogleButtonWidth(0)).toBeUndefined()
    expect(resolveGoogleButtonLocale('zh-CN')).toBe('zh_CN')
    expect(resolveGoogleButtonLocale('fr')).toBe('fr')
  })
})

describe('telegram helpers', () => {
  it('validates widget payloads', () => {
    expect(buildTelegramPayload({ id: '12', auth_date: 100, hash: 'h', username: 'bob' })).toMatchObject({ id: 12, auth_date: 100, hash: 'h', username: 'bob' })
    expect(buildTelegramPayload({ id: 12, auth_date: 100, hash: '' })).toBeNull()
    expect(buildTelegramPayload(null)).toBeNull()
  })
  it('builds the mini app entry link', () => {
    expect(buildTelegramMiniAppEntryLink('@shop_bot', 'https://x')).toBe('https://telegram.me/shop_bot/webapp')
    expect(buildTelegramMiniAppEntryLink('shop_bot', '')).toBe('')
  })
})

describe('misc', () => {
  it('counts seconds until an instant', () => {
    expect(secondsUntil('2026-01-01T00:01:00Z', Date.parse('2026-01-01T00:00:00Z'))).toBe(60)
    expect(secondsUntil('2026-01-01T00:00:00Z', Date.parse('2026-01-01T00:01:00Z'))).toBe(0)
    expect(secondsUntil('bad', 0)).toBe(0)
  })
  it('formats post dates per locale', () => {
    expect(formatPostDate('2026-09-24T12:00:00Z', 'en-US')).toContain('2026')
    expect(formatPostDate('', 'zh-CN')).toBe('')
    expect(formatPostDate('garbage', 'zh-CN')).toBe('')
  })
})
