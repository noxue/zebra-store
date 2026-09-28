// 个人中心 → API 对接 → 异次元 / 萌次元 对接: compat key load / issue / reset, switch and IP allowlist.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ApiCompatKeyData } from '@/api/types'
import { ApiError } from '@/api/client'

const getCompat = vi.fn()
const issueCompat = vi.fn()
const updateCompat = vi.fn()
const toastSuccess = vi.fn()

vi.mock('@/api/credential', () => ({
  apiCredentialAPI: {
    getCompat: () => getCompat(),
    issueCompat: () => issueCompat(),
    updateCompat: (body: unknown) => updateCompat(body),
  },
}))
vi.mock('@/composables/useToast', () => ({ toast: { success: (m: string) => toastSuccess(m), error: vi.fn() } }))

const { useCompatConnect, maskAppKey, normalizeAllowlist } = await import('@/composables/personal/useCompatConnect')

const KEY = 'ABCD1234EFGH5678IJKL9012MNOP3456'
const view = (extra: Partial<ApiCompatKeyData> = {}): ApiCompatKeyData => ({
  app_id: '42',
  app_key: KEY,
  is_active: true,
  ip_allowlist: '',
  last_used_at: null,
  site_url: 'https://shop.example',
  protocols: [
    { id: 'acg-faka', enabled: true, path: '/shared/' },
    { id: 'mcy-open-api', enabled: false, path: '/plugin/open-api/' },
  ],
  ...extra,
})

beforeEach(() => {
  vi.clearAllMocks()
})

describe('compat connect helpers', () => {
  it('masks the middle of the key', () => {
    expect(maskAppKey(KEY)).toBe('ABCD••••••••3456')
    expect(maskAppKey('SHORT')).toBe('•••••')
    expect(maskAppKey('')).toBe('')
  })

  it('normalizes allowlist text like the server', () => {
    expect(normalizeAllowlist(' 1.2.3.4\n10.0.0.0/8 ,  ;2001:db8::/32\n\n')).toBe('1.2.3.4,10.0.0.0/8,2001:db8::/32')
    expect(normalizeAllowlist('   ')).toBe('')
  })
})

describe('useCompatConnect', () => {
  it('loads the key masked and lists enabled protocols', async () => {
    getCompat.mockResolvedValue({ data: view({ ip_allowlist: '1.2.3.4,10.0.0.0/8' }) })
    const c = useCompatConnect()
    await c.load()
    expect(c.hasKey.value).toBe(true)
    expect(c.displayedKey.value).toBe(maskAppKey(KEY))
    c.toggleReveal()
    expect(c.displayedKey.value).toBe(KEY)
    expect(c.enabledProtocols.value.map((p) => p.id)).toEqual(['acg-faka'])
    expect(c.allowlist.value).toBe('1.2.3.4\n10.0.0.0/8')
    expect(c.allowlistDirty.value).toBe(false)
  })

  it('issues a key when none exists and reveals it once', async () => {
    getCompat.mockResolvedValue({ data: view({ app_key: '' }) })
    issueCompat.mockResolvedValue({ data: view() })
    const c = useCompatConnect()
    await c.load()
    expect(c.hasKey.value).toBe(false)
    expect(await c.issue()).toBe(true)
    expect(issueCompat).toHaveBeenCalledTimes(1)
    expect(c.hasKey.value).toBe(true)
    expect(c.keyRevealed.value).toBe(true)
    expect(c.displayedKey.value).toBe(KEY)
    expect(toastSuccess).toHaveBeenCalled()
  })

  it('asks before resetting and can be cancelled', async () => {
    getCompat.mockResolvedValue({ data: view() })
    const c = useCompatConnect()
    await c.load()
    c.askIssue()
    expect(c.confirmingIssue.value).toBe(true)
    c.cancelIssue()
    expect(c.confirmingIssue.value).toBe(false)
    expect(issueCompat).not.toHaveBeenCalled()
  })

  it('switches access and saves the normalized allowlist', async () => {
    getCompat.mockResolvedValue({ data: view() })
    updateCompat.mockImplementation((body: { is_active?: boolean; ip_allowlist?: string }) =>
      Promise.resolve({ data: view({ is_active: body.is_active ?? true, ip_allowlist: body.ip_allowlist ?? '' }) }),
    )
    const c = useCompatConnect()
    await c.load()
    expect(await c.toggleActive(false)).toBe(true)
    expect(updateCompat).toHaveBeenLastCalledWith({ is_active: false })
    expect(c.data.value?.is_active).toBe(false)

    c.allowlist.value = '203.0.113.7\n198.51.100.0/24'
    expect(c.allowlistDirty.value).toBe(true)
    expect(await c.saveAllowlist()).toBe(true)
    expect(updateCompat).toHaveBeenLastCalledWith({ ip_allowlist: '203.0.113.7,198.51.100.0/24' })
    expect(c.allowlistDirty.value).toBe(false)
  })

  it('surfaces the backend message (e.g. credential not approved)', async () => {
    getCompat.mockRejectedValue(new ApiError('API credential is not approved', 400, 200))
    const c = useCompatConnect()
    await c.load()
    expect(c.data.value).toBeNull()
    expect(c.error.value).toBe('API credential is not approved')
    issueCompat.mockRejectedValue(new ApiError('参数错误', 400, 200))
    expect(await c.issue()).toBe(false)
    expect(c.error.value).toBe('参数错误')
    expect(c.busy.value).toBe(false)
  })
})
