// 个人中心 → API 对接 →「一键对接」: connection code visibility/copy logic and secret rotation.
import { ref } from 'vue'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { ApiCredentialData } from '@/api/types'
import { ApiError } from '@/api/client'

const createConnectionCode = vi.fn()
const rotate = vi.fn()
const copyText = vi.fn()
const toastSuccess = vi.fn()
const toastError = vi.fn()

vi.mock('@/api/credential', () => ({
  apiCredentialAPI: {
    createConnectionCode: () => createConnectionCode(),
    rotate: () => rotate(),
  },
}))
vi.mock('@/utils/clipboard', () => ({ copyText: (v: string) => copyText(v) }))
vi.mock('@/composables/useToast', () => ({ toast: { success: (m: string) => toastSuccess(m), error: (m: string) => toastError(m) } }))

const { useApiConnect } = await import('@/composables/personal/useApiConnect')
const { maskConnectionCode, resolveRotationState, supportsConnectionCode } = await import('@/utils/apiConnect')

const CODE = 'zsc1_eyJ2IjoxLCJ1cmwiOiJodHRwczovL3MuZXhhbXBsZSIsImtleSI6ImsiLCJzZWNyZXQiOiJzIn0'
const approved = (extra: Partial<ApiCredentialData> = {}): ApiCredentialData => ({
  status: 'approved',
  id: 3,
  api_key: 'key',
  protocols: ['dujiao-next', 'zebra-store'],
  rotation_pending: false,
  rotation_expires_at: null,
  ...extra,
})

beforeEach(() => {
  vi.clearAllMocks()
  copyText.mockResolvedValue(undefined)
})

describe('api connect helpers', () => {
  it('offers 一键对接 only for approved credentials served over zebra-store', () => {
    expect(supportsConnectionCode(approved())).toBe(true)
    expect(supportsConnectionCode(approved({ protocols: ['dujiao-next'] }))).toBe(false)
    expect(supportsConnectionCode(approved({ protocols: undefined }))).toBe(false)
    expect(supportsConnectionCode(approved({ status: 'pending' }))).toBe(false)
    expect(supportsConnectionCode(null)).toBe(false)
  })

  it('masks the middle of the code (it embeds the secret)', () => {
    const masked = maskConnectionCode(CODE)
    expect(masked.startsWith('zsc1_eyJ2I')).toBe(true)
    expect(masked.endsWith(CODE.slice(-4))).toBe(true)
    expect(masked).not.toContain('c2VjcmV0')
    expect(maskConnectionCode('short')).toBe('•••••')
  })

  it('resolves the rotation state, preferring the latest response and dropping expired ones', () => {
    const now = Date.parse('2026-09-25T00:00:00Z')
    expect(resolveRotationState(approved(), null, now)).toEqual({ pending: false, expiresAt: null })
    expect(resolveRotationState(approved({ rotation_pending: true, rotation_expires_at: '2026-10-02T00:00:00Z' }), null, now)).toEqual({
      pending: true,
      expiresAt: '2026-10-02T00:00:00Z',
    })
    expect(resolveRotationState(approved(), '2026-10-03T00:00:00Z', now)).toEqual({ pending: true, expiresAt: '2026-10-03T00:00:00Z' })
    expect(resolveRotationState(approved({ rotation_pending: true, rotation_expires_at: '2026-09-01T00:00:00Z' }), null, now).pending).toBe(false)
    expect(resolveRotationState(approved({ rotation_pending: true }), null, now)).toEqual({ pending: true, expiresAt: null })
  })
})

describe('useApiConnect', () => {
  it('requires confirmation, then shows the code once and reloads the credential', async () => {
    createConnectionCode.mockResolvedValue({ status_code: 0, msg: '', data: { code: CODE, rotation_expires_at: '2099-10-02T00:00:00Z' } })
    const reload = vi.fn(() => Promise.resolve())
    const c = useApiConnect({ credential: ref(approved()), reload })
    expect(c.available.value).toBe(true)

    c.ask('code')
    expect(c.confirming.value).toBe('code')
    expect(createConnectionCode).not.toHaveBeenCalled()
    c.cancel()
    expect(c.confirming.value).toBeNull()

    c.ask('code')
    expect(await c.confirm()).toBe(true)
    expect(createConnectionCode).toHaveBeenCalledTimes(1)
    expect(reload).toHaveBeenCalledTimes(1)
    expect(c.confirming.value).toBeNull()
    expect(c.code.value).toBe(CODE)
    expect(c.displayedCode.value).toBe(CODE)
    expect(c.rotation.value).toEqual({ pending: true, expiresAt: '2099-10-02T00:00:00Z' })
    expect(toastSuccess).toHaveBeenCalled()
  })

  it('masks on demand but always copies the full code', async () => {
    createConnectionCode.mockResolvedValue({ data: { code: CODE, rotation_expires_at: null } })
    const c = useApiConnect({ credential: ref(approved()) })
    c.ask('code')
    await c.confirm()

    c.toggleReveal()
    expect(c.codeRevealed.value).toBe(false)
    expect(c.displayedCode.value).toBe(maskConnectionCode(CODE))

    expect(await c.copyCode()).toBe(true)
    expect(copyText).toHaveBeenCalledWith(CODE)
    expect(c.codeCopied.value).toBe(true)

    c.toggleReveal()
    expect(c.displayedCode.value).toBe(CODE)
  })

  it('forgets the code after "I have saved it" so it cannot be shown or copied again', async () => {
    createConnectionCode.mockResolvedValue({ data: { code: CODE, rotation_expires_at: null } })
    const c = useApiConnect({ credential: ref(approved()) })
    c.ask('code')
    await c.confirm()
    c.dismissCode()
    expect(c.code.value).toBe('')
    expect(c.displayedCode.value).toBe('')
    expect(await c.copyCode()).toBe(false)
    expect(copyText).not.toHaveBeenCalled()
  })

  it('reports a copy failure without marking the code as copied', async () => {
    createConnectionCode.mockResolvedValue({ data: { code: CODE, rotation_expires_at: null } })
    copyText.mockRejectedValue(new Error('denied'))
    const c = useApiConnect({ credential: ref(approved()) })
    c.ask('code')
    await c.confirm()
    expect(await c.copyCode()).toBe(false)
    expect(c.codeCopied.value).toBe(false)
    expect(toastError).toHaveBeenCalled()
  })

  it('rotates the secret, shows it once and hides any previous code', async () => {
    createConnectionCode.mockResolvedValue({ data: { code: CODE, rotation_expires_at: null } })
    rotate.mockResolvedValue({ data: { api_secret: 'new-secret-123', rotation_expires_at: '2099-01-01T00:00:00Z' } })
    const c = useApiConnect({ credential: ref(approved()) })
    c.ask('code')
    await c.confirm()
    c.ask('rotate')
    expect(await c.confirm()).toBe(true)
    expect(c.code.value).toBe('')
    expect(c.rotatedSecret.value).toBe('new-secret-123')
    expect(c.rotation.value.expiresAt).toBe('2099-01-01T00:00:00Z')
    c.dismissSecret()
    expect(c.rotatedSecret.value).toBe('')
  })

  it('surfaces the backend message when generation fails', async () => {
    createConnectionCode.mockRejectedValue(new ApiError('凭证已禁用', 403, 200))
    const c = useApiConnect({ credential: ref(approved()) })
    c.ask('code')
    expect(await c.confirm()).toBe(false)
    expect(c.error.value).toBe('凭证已禁用')
    expect(c.code.value).toBe('')
    expect(c.busy.value).toBe(false)
    expect(c.confirming.value).toBeNull()
  })

  it('shows the pending rotation from the loaded credential', () => {
    const credential = ref<ApiCredentialData | null>(approved({ rotation_pending: true, rotation_expires_at: '2099-10-02T00:00:00Z' }))
    const c = useApiConnect({ credential })
    expect(c.rotation.value).toEqual({ pending: true, expiresAt: '2099-10-02T00:00:00Z' })
    credential.value = approved({ protocols: ['dujiao-next'] })
    expect(c.available.value).toBe(false)
    expect(c.rotation.value.pending).toBe(false)
  })
})
