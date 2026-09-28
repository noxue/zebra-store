// AUTH-10 (bugfix-lessons §21): an expired session is cleared both on HTTP 401 and on a
// business `status_code: 401`, but the 401 of the login endpoints themselves is only an
// error message (no logout, no redirect).
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, userApi } from '@/api/client'
import { isPublicAuthEndpoint } from '@/utils/authEndpoints'

const envelope = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })

describe('AUTH-10 expired tokens vs failed logins', () => {
  beforeEach(() => {
    localStorage.clear()
    localStorage.setItem('user_token', 'expired')
    localStorage.setItem('user_profile', '{"id":1}')
  })
  afterEach(() => vi.restoreAllMocks())

  it('clears the session on a business status_code 401 (HTTP 200)', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(envelope({ status_code: 401, msg: 'expired', data: null }))
    await expect(userApi.get('/me')).rejects.toBeInstanceOf(ApiError)
    expect(localStorage.getItem('user_token')).toBeNull()
    expect(localStorage.getItem('user_profile')).toBeNull()
  })

  it('clears the session on an HTTP 401', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(envelope({ status_code: 401, msg: 'expired', data: null }, 401))
    await expect(userApi.get('/orders')).rejects.toBeInstanceOf(ApiError)
    expect(localStorage.getItem('user_token')).toBeNull()
  })

  it('keeps the page state when the login itself answers 401', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(envelope({ status_code: 401, msg: '邮箱或密码错误', data: null }, 401))
    const failure = await userApi.post('/auth/login', { email: 'a@b.co', password: 'x' }).catch((e: unknown) => e)
    expect(failure).toBeInstanceOf(ApiError)
    expect((failure as ApiError).message).toBe('邮箱或密码错误')
    expect(localStorage.getItem('user_token')).toBe('expired')
  })

  it('treats every public auth endpoint as exempt', () => {
    for (const path of ['/auth/login', '/auth/register', '/auth/telegram/login', '/auth/forgot-password?x=1']) {
      expect(isPublicAuthEndpoint(path)).toBe(true)
    }
    expect(isPublicAuthEndpoint('/me')).toBe(false)
  })
})
