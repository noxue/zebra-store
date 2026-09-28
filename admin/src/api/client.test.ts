import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { ApiError, api, buildUrl } from './client'
import { useNoticeStore } from '@/stores/notice'

const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } })

describe('api client', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    localStorage.clear()
  })
  afterEach(() => vi.restoreAllMocks())

  it('builds urls skipping null params and repeating arrays', () => {
    expect(buildUrl('/admin/x', { a: 1, b: undefined, c: null, d: ['p', 'q'] })).toBe('/api/v1/admin/x?a=1&d=p&d=q')
  })

  it('sends auth + language headers and returns the envelope', async () => {
    localStorage.setItem('admin_token', 'tok')
    const spy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ status_code: 0, msg: 'ok', data: [1], pagination: { page: 1, page_size: 20, total: 1, total_page: 1 } }))
    const res = await api.get<number[]>('/admin/products', { params: { page: 1 } })
    expect(res.data).toEqual([1])
    const init = spy.mock.calls[0]?.[1] as RequestInit
    const headers = init.headers as Record<string, string>
    expect(headers.Authorization).toBe('Bearer tok')
    expect(headers['X-Lang']).toBe('zh-CN')
  })

  it('throws ApiError and toasts on business errors', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ status_code: 1001, msg: '参数错误', data: null }))
    await expect(api.post('/admin/coupons', { a: 1 })).rejects.toBeInstanceOf(ApiError)
    expect(useNoticeStore().items.map((i) => i.message)).toContain('参数错误')
  })

  it('does not toast compliance rejections', async () => {
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ status_code: 403, msg: 'compliance_required', data: null }))
    await expect(api.get('/admin/payments')).rejects.toMatchObject({ message: 'compliance_required', notified: false })
    expect(useNoticeStore().items).toHaveLength(0)
  })

  it('clears the token on 401', async () => {
    localStorage.setItem('admin_token', 'old')
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ status_code: 401, msg: 'unauthorized', data: null }, 401))
    await expect(api.get('/admin/orders')).rejects.toBeInstanceOf(ApiError)
    expect(localStorage.getItem('admin_token')).toBeNull()
  })
})

describe('FE-20 login failures and request timeout', () => {
  beforeEach(() => { setActivePinia(createPinia()); localStorage.clear() })
  afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers() })
  it('does not redirect or clear session state for a login 401', async () => {
    localStorage.setItem('admin_token', 'existing')
    const before = window.location.href
    vi.spyOn(globalThis, 'fetch').mockResolvedValue(jsonResponse({ status_code: 401, msg: 'wrong password', data: null }, 401))
    await expect(api.post('/admin/login', {})).rejects.toMatchObject({ statusCode: 401 })
    expect(localStorage.getItem('admin_token')).toBe('existing')
    expect(window.location.href).toBe(before)
  })
  it('aborts a stalled request after ten seconds and notifies the user', async () => {
    vi.useFakeTimers()
    let signal: AbortSignal | undefined
    vi.spyOn(globalThis, 'fetch').mockImplementation((_url, init) => new Promise((_resolve, reject) => {
      signal = init?.signal as AbortSignal
      signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')))
    }))
    const request = api.get('/admin/orders')
    const failure = expect(request).rejects.toMatchObject({ statusCode: -1, notified: true })
    await vi.advanceTimersByTimeAsync(9999)
    expect(signal?.aborted).toBe(false)
    await vi.advanceTimersByTimeAsync(1)
    await failure
    expect(signal?.aborted).toBe(true)
    expect(useNoticeStore().items).toHaveLength(1)
  })
})
