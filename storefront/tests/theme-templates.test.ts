import { beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick, ref } from 'vue'
import { mount, shallowMount } from '@vue/test-utils'
import App from '@/App'
import { configAPI } from '@/api/catalog'
import type { SiteConfig } from '@/api/types'
import { HomeHero } from '@/components/home/HomeHero'
import { SiteFooter } from '@/components/layout/SiteFooter'
import { SiteLogo } from '@/components/layout/SiteLogo'
import i18n from '@/i18n'
import { useAppStore } from '@/stores/app'
import Home from '@/views/Home'
import { hasLegacyClassicPalette, storefrontTemplate } from '@/utils/storefrontTemplate'
import { useTheme } from '@/utils/theme'

vi.mock('@/composables/useHome', () => ({
  useHome: () => ({
    banner: {}, featured: ref([]), featuredLoading: ref(false), latestVisible: ref(false), posts: ref([]),
    isListMode: ref(false), quickBuy: { product: ref(null), open: ref(false), close: vi.fn() },
    announcement: ref(null),
  }),
}))

beforeEach(() => {
  setActivePinia(createPinia())
  document.documentElement.removeAttribute('data-storefront-template')
  document.documentElement.style.cssText = ''
  vi.restoreAllMocks()
})

it('uses the dark Logo on theme switch and falls back to the light Logo', async () => {
  const app = useAppStore()
  const { theme } = useTheme()
  const originalTheme = theme.value
  try {
    app.config = { brand: { site_name: 'Shop', site_logo: '/light.png', site_logo_dark: '/dark.png' } }
    const wrapper = mount(SiteLogo, { global: { stubs: { RouterLink: { template: '<a><slot /></a>' } } } })
    theme.value = 'light'
    await nextTick()
    expect(wrapper.get('img').attributes('src')).toBe('/light.png')
    theme.value = 'dark'
    await nextTick()
    expect(wrapper.get('img').attributes('src')).toBe('/dark.png')
    app.config.brand!.site_logo_dark = ''
    await nextTick()
    expect(wrapper.get('img').attributes('src')).toBe('/light.png')
    wrapper.unmount()
  } finally {
    theme.value = originalTheme
  }
})

it('shows a neutral boot skeleton and no default title until site config arrives', async () => {
  const app = useAppStore()
  const wrapper = shallowMount(App, { global: { plugins: [i18n] } })
  expect(wrapper.find('[data-testid="site-boot"]').exists()).toBe(true)
  expect(wrapper.text()).not.toContain('Zebra Store')
  expect(document.title).toBe('')
  wrapper.unmount()
  vi.spyOn(configAPI, 'get').mockResolvedValue({ data: { brand: { site_name: 'AI小店铺' } }, status_code: 0, msg: 'ok' } as Awaited<ReturnType<typeof configAPI.get>>)
  await app.loadConfig()
  await nextTick()
  expect(document.title).toBe('AI小店铺')
  expect(document.documentElement.classList.contains('zs-config-pending')).toBe(false)
})

it('SET-04 applies the saved vault template instead of the legacy Sakura colours', async () => {
  const config: SiteConfig = {
    storefront_template: 'vault',
    theme: { primary_color: '#ff5fa2', secondary_color: '#8b5cf6', accent_color: '#38bdf8', default_mode: 'system', effects: { sakura: true } },
  }
  expect(hasLegacyClassicPalette(config)).toBe(true)
  vi.spyOn(configAPI, 'get').mockResolvedValue({ data: config, status_code: 0, msg: 'ok' } as Awaited<ReturnType<typeof configAPI.get>>)
  const app = useAppStore()
  await app.loadConfig(true)
  expect(app.template).toBe('vault')
  expect(app.sakuraEnabled).toBe(false)
  expect(document.documentElement.dataset.storefrontTemplate).toBe('vault')
  expect(document.documentElement.style.getPropertyValue('--zs-primary')).toBe('')
  expect(storefrontTemplate('atelier')).toBe('atelier')
  expect(storefrontTemplate('unexpected')).toBe('classic')
})

it('SET-04 preserves a deliberately selected Sakura palette on a dark vault template', async () => {
  const config: SiteConfig = {
    storefront_template: 'vault',
    theme: { primary_color: '#ff5fa2', secondary_color: '#8b5cf6', accent_color: '#38bdf8', default_mode: 'dark' },
  }
  expect(hasLegacyClassicPalette(config)).toBe(false)
  vi.spyOn(configAPI, 'get').mockResolvedValue({ data: config, status_code: 0, msg: 'ok' } as Awaited<ReturnType<typeof configAPI.get>>)
  await useAppStore().loadConfig(true)
  expect(document.documentElement.style.getPropertyValue('--zs-primary')).toBe('#ff5fa2')
})

it('SET-04 preserves Sakura colours with system mode when vault Sakura effects are off', async () => {
  const config: SiteConfig = {
    storefront_template: 'vault',
    theme: { primary_color: '#ff5fa2', secondary_color: '#8b5cf6', accent_color: '#38bdf8', default_mode: 'system', effects: { sakura: false } },
  }
  expect(hasLegacyClassicPalette(config)).toBe(false)
  vi.spyOn(configAPI, 'get').mockResolvedValue({ data: config, status_code: 0, msg: 'ok' } as Awaited<ReturnType<typeof configAPI.get>>)
  await useAppStore().loadConfig(true)
  expect(document.documentElement.style.getPropertyValue('--zs-primary')).toBe('#ff5fa2')
})

it('SET-04 keeps custom palette colours when switching the storefront template', async () => {
  const config: SiteConfig = { storefront_template: 'aurora', theme: { primary_color: '#004488' } }
  vi.spyOn(configAPI, 'get').mockResolvedValue({ data: config, status_code: 0, msg: 'ok' } as Awaited<ReturnType<typeof configAPI.get>>)
  await useAppStore().loadConfig(true)
  expect(document.documentElement.dataset.storefrontTemplate).toBe('aurora')
  expect(document.documentElement.style.getPropertyValue('--zs-primary')).toBe('#004488')
})

it('hides the home hero, footer quick links, and repository link independently', async () => {
  const app = useAppStore()
  app.config = { theme: { show_home_hero: true, show_footer_quick_links: true, show_repository_link: true } }
  const global = { plugins: [i18n], stubs: { RouterLink: true } }
  const home = shallowMount(Home, { global })
  const footer = shallowMount(SiteFooter, { global })
  expect(home.findComponent(HomeHero).exists()).toBe(true)
  expect(footer.text()).toContain('快速链接')
  expect(footer.text()).toContain('github.com/noxue/zebra-store')

  app.config = { theme: { show_home_hero: false, show_footer_quick_links: true, show_repository_link: true } }
  await nextTick()
  expect(home.findComponent(HomeHero).exists()).toBe(false)
  expect(footer.text()).toContain('快速链接')
  expect(footer.text()).toContain('github.com/noxue/zebra-store')

  app.config = { theme: { show_home_hero: false, show_footer_quick_links: false, show_repository_link: true } }
  await nextTick()
  expect(footer.text()).not.toContain('快速链接')
  expect(footer.text()).toContain('github.com/noxue/zebra-store')

  app.config = { theme: { show_home_hero: false, show_footer_quick_links: false, show_repository_link: false } }
  await nextTick()
  expect(footer.text()).not.toContain('github.com/noxue/zebra-store')
  expect(footer.find('router-link-stub[to="/privacy"]').exists()).toBe(true)
  home.unmount()
  footer.unmount()
})
