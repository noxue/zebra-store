import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import Settings from '../Settings'
import { adminAPI } from '@/api/admin'
import i18n from '@/i18n'

vi.mock('vue-router', () => ({ useRoute: () => ({ query: { tab: 'smtp' } }), useRouter: () => ({ replace: vi.fn() }) }))
beforeEach(() => { setActivePinia(createPinia()) })
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers() })
it('FE-24 disables save until delayed SMTP data is loaded and renders the returned host', async () => {
  vi.useFakeTimers()
  vi.spyOn(adminAPI, 'getSettings').mockResolvedValue({ status_code: 0, msg: '', data: {} })
  for (const key of ['getCaptchaSettings', 'getTelegramAuthSettings', 'getGoogleAuthSettings', 'getOrderEmailTemplateSettings'] as const) {
    vi.spyOn(adminAPI, key).mockResolvedValue({ status_code: 0, msg: '', data: {} })
  }
  vi.spyOn(adminAPI, 'getSMTPSettings').mockImplementation(() => new Promise((resolve) => {
    setTimeout(() => resolve({ status_code: 0, msg: '', data: { host: 'smtp.x.com', port: 587 } }), 500)
  }))
  const wrapper = mount(Settings, { global: { plugins: [i18n] } })
  await wrapper.vm.$nextTick()
  expect(wrapper.get('[data-testid="settings-save"]').attributes('disabled')).toBeDefined()
  await vi.advanceTimersByTimeAsync(500)
  await flushPromises()
  expect(wrapper.get('[data-testid="settings-save"]').attributes('disabled')).toBeUndefined()
  expect(wrapper.findAll('input').some((input) => input.element.value === 'smtp.x.com')).toBe(true)
  wrapper.unmount()
})
