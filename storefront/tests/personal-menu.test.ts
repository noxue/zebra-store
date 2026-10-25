import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, h, nextTick } from 'vue'
import { mount } from '@vue/test-utils'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import i18n from '@/i18n'
import { usePersonalCenter } from '@/composables/personal/usePersonalCenter'
import { useAppStore } from '@/stores/app'
import { useUserProfileStore } from '@/stores/userProfile'

beforeEach(() => setActivePinia(createPinia()))
afterEach(() => vi.restoreAllMocks())

it('hides disabled affiliate and reseller menus, then shows them when enabled', async () => {
  const app = useAppStore()
  const profile = useUserProfileStore()
  vi.spyOn(profile, 'loadProfile').mockResolvedValue(true)
  vi.spyOn(profile, 'loadRecentOrders').mockResolvedValue(true)
  vi.spyOn(profile, 'loadMemberLevels').mockResolvedValue(undefined)
  app.config = { affiliate: { enabled: false }, reseller_enabled: false, tenant: { mode: 'main' } }
  const view = mount(defineComponent({
    setup() {
      const shell = usePersonalCenter(() => 'affiliate')
      return () => h('div', { 'data-current': shell.currentSection.value },
        shell.navItems.value.map((item) => h('span', { key: item.key }, item.key)))
    },
  }), { global: { plugins: [i18n] } })
  expect(view.text()).not.toContain('affiliate')
  expect(view.text()).not.toContain('reseller')
  expect(view.attributes('data-current')).toBe('overview')

  app.config = { affiliate: { enabled: true }, reseller_enabled: true, tenant: { mode: 'main' } }
  await nextTick()
  expect(view.text()).toContain('affiliate')
  expect(view.text()).toContain('reseller')
  expect(view.attributes('data-current')).toBe('affiliate')

  app.config = { affiliate: { enabled: true }, reseller_enabled: true, tenant: { mode: 'reseller' } }
  await nextTick()
  expect(view.text()).not.toContain('reseller')
  view.unmount()
})
