import { mount, flushPromises } from '@vue/test-utils'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import i18n from '@/i18n'
import { PersonalApiKeyCard } from './PersonalApiKeyCard'
const mocks = vi.hoisted(() => ({ status: vi.fn(), create: vi.fn(), rotate: vi.fn(), remove: vi.fn(), confirm: vi.fn() }))
vi.mock('@/api/personalApiKey', () => ({ personalApiKeyAPI: mocks }))
vi.mock('@/utils/confirm', () => ({ confirmAction: mocks.confirm }))
describe('personal API key controls', () => {
  beforeEach(() => { vi.resetAllMocks(); mocks.status.mockResolvedValue({ data: null }); mocks.confirm.mockResolvedValue(false) })
  it('shows a newly generated secret once without persisting it', async () => {
    const secret = 'zsa_one-time-secret'
    mocks.create.mockResolvedValue({ data: { api_key: secret, prefix: 'zsa_prefix' } })
    const view = mount(PersonalApiKeyCard, { global: { plugins: [i18n] } })
    await flushPromises()
    mocks.status.mockResolvedValue({ data: { prefix: 'zsa_prefix' } })
    await view.findAll('button').find((b) => b.text() === '生成 API Key')!.trigger('click')
    await flushPromises()
    expect(view.text()).toContain(secret)
    expect(view.text()).toContain('/api/v1/admin/api-key/token')
    expect(localStorage.getItem('api_key')).toBeNull()
    view.unmount()
    const reopened = mount(PersonalApiKeyCard, { global: { plugins: [i18n] } })
    await flushPromises()
    expect(reopened.text()).not.toContain(secret)
    reopened.unmount()
  })
  it('requires confirmation before rotation or deletion', async () => {
    mocks.status.mockResolvedValue({ data: { prefix: 'zsa_prefix' } })
    const view = mount(PersonalApiKeyCard, { global: { plugins: [i18n] } })
    await flushPromises()
    for (const label of ['轮换 API Key', '删除 API Key']) await view.findAll('button').find((b) => b.text() === label)!.trigger('click')
    await flushPromises()
    expect(mocks.rotate).not.toHaveBeenCalled()
    expect(mocks.remove).not.toHaveBeenCalled()
    view.unmount()
  })
  it('does not offer generation if loading the current status failed', async () => {
    mocks.status.mockRejectedValue(new Error('offline'))
    const view = mount(PersonalApiKeyCard, { global: { plugins: [i18n] } })
    await flushPromises()
    expect(view.get('[role="alert"]').text()).toBe('offline')
    expect(view.findAll('button').map((b) => b.text())).not.toContain('生成 API Key')
    view.unmount()
  })
})
