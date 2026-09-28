// Live QA 2026-09-26 I-5 (QA-A05): the update dialog must use the original endpoints.
import { beforeEach, describe, expect, it, vi } from 'vitest'

const checkSystemUpdate = vi.fn()
const getUpdateCapability = vi.fn()

vi.mock('@/api/admin', () => ({
  adminAPI: {
    checkSystemUpdate: () => checkSystemUpdate(),
    getUpdateCapability: () => getUpdateCapability(),
  },
}))

const { loadSystemUpdateInfo } = await import('./useSystemUpdateInfo')

describe('system update info', () => {
  beforeEach(() => {
    checkSystemUpdate.mockReset()
    getUpdateCapability.mockReset()
  })

  it('QA-A05 reads version/check and update/capability and derives the block reason', async () => {
    checkSystemUpdate.mockResolvedValue({ data: { current_version: '0.1.0', latest_version: '0.1.0', has_update: false, source: 'local' } })
    getUpdateCapability.mockResolvedValue({ data: { capability: { can_update: false, block_reason: 'source_build' }, state: { status: 'idle' } } })
    const info = await loadSystemUpdateInfo('x')
    expect(checkSystemUpdate).toHaveBeenCalledTimes(1)
    expect(getUpdateCapability).toHaveBeenCalledTimes(1)
    expect(info).toEqual({ currentVersion: '0.1.0', latestVersion: '0.1.0', hasUpdate: false, canUpdate: false, blockReason: 'source_build' })
  })

  it('QA-A05 falls back to the public config version and a generic reason', async () => {
    checkSystemUpdate.mockResolvedValue({ data: {} })
    getUpdateCapability.mockResolvedValue({ data: {} })
    const info = await loadSystemUpdateInfo('0.2.0')
    expect(info.currentVersion).toBe('0.2.0')
    expect(info.blockReason).toBe('unsupported')
  })
})
