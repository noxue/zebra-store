import { beforeEach, expect, it } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { defineComponent, h } from 'vue'
import { mount } from '@vue/test-utils'
import i18n from '@/i18n'
import { useAppStore } from '@/stores/app'
import { useNavConfig } from '@/composables/useNavConfig'
import About from '@/views/About'
import Contact from '@/views/Contact'
import router from '@/router'

beforeEach(() => setActivePinia(createPinia()))
const options = { global: { plugins: [i18n], stubs: { Mascot: true, EmptyState: { props: ['title'], template: '<p>{{ title }}</p>' } } } }

it('shows main-site QR codes on the independent support page and keeps About introduction-only', () => {
  useAppStore().config = {
    tenant: { mode: 'main' },
    about: { introduction: { 'zh-CN': '主站介绍' }, contact: { title: { 'zh-CN': '服务团队' }, text: { 'zh-CN': '请提供订单号' } } },
    contact: { wechat_qr_code: '/uploads/main-wechat.png', telegram_qr_code: '/uploads/main-tg.png' },
  }
  const support = mount(Contact, options)
  expect(support.text()).toContain('服务团队')
  expect(support.text()).toContain('请提供订单号')
  expect(support.findAll('img').map((img) => img.attributes('src'))).toEqual(['/uploads/main-wechat.png', '/uploads/main-tg.png'])
  const about = mount(About, options)
  expect(about.text()).toContain('主站介绍')
  expect(about.text()).not.toContain('请提供订单号')
  expect(about.find('img').exists()).toBe(false)
  support.unmount()
  about.unmount()
})

// RSL-06: never inherit main-shop guidance or legacy support in a reseller shop.
it('isolates reseller support and removes the main About entry from navigation', async () => {
  useAppStore().config = {
    tenant: { mode: 'reseller' },
    about: { introduction: { 'zh-CN': '主站私有介绍' }, contact: { text: { 'zh-CN': '主站客服说明' } } },
    support: { email: 'main@example.test' },
    contact: { wechat_qr_code: '/uploads/reseller/wechat.png', telegram_qr_code: '/uploads/reseller/tg.png' },
  }
  const support = mount(Contact, options)
  expect(support.text()).not.toContain('主站客服说明')
  expect(support.text()).not.toContain('main@example.test')
  expect(support.findAll('img').map((img) => img.attributes('src'))).toEqual(['/uploads/reseller/wechat.png', '/uploads/reseller/tg.png'])
  const nav = mount(defineComponent({ setup() {
    const nav = useNavConfig()
    return () => h('div', nav.primaryNavItems.value.map((item) => h('a', { href: item.path }, item.label)))
  } }), options)
  expect(nav.find('a[href="/about"]').exists()).toBe(false)
  expect(nav.find('a[href="/contact"]').exists()).toBe(true)
  await router.push('/about')
  expect(router.currentRoute.value.path).toBe('/contact')
  const about = mount(About, options)
  expect(about.text()).not.toContain('主站私有介绍')
  support.unmount()
  nav.unmount()
  about.unmount()
})

it('shows an empty state for a reseller without contact settings', () => {
  useAppStore().config = { tenant: { mode: 'reseller' }, contact: {}, support: { telegram: 'https://t.me/main' } }
  const support = mount(Contact, options)
  expect(support.text()).toContain('本站暂未配置客服联系方式')
  expect(support.find('a').exists()).toBe(false)
  support.unmount()
})

it('FE-CONTACT-QR-01 opens each QR code in a closable in-page preview', async () => {
  useAppStore().config = {
    tenant: { mode: 'main' },
    contact: { wechat_qr_code: '/uploads/wechat.png', telegram_qr_code: '/uploads/telegram.png', wechat_group_qr_code: '/uploads/group.png' },
  }
  const support = mount(Contact, { ...options, attachTo: document.body })
  const triggers = support.findAll('button[aria-haspopup="dialog"]')
  expect(triggers).toHaveLength(3)
  expect(support.find('a[href="/uploads/wechat.png"]').exists()).toBe(false)
  for (const [index, image] of ['/uploads/wechat.png', '/uploads/telegram.png', '/uploads/group.png'].entries()) {
    await triggers[index]!.trigger('click')
    const dialog = document.querySelector('[role="dialog"]')!
    expect(dialog.getAttribute('aria-modal')).toBe('true')
    expect(dialog.querySelector('img')?.getAttribute('src')).toBe(image)
    expect(document.body.style.overflow).toBe('hidden')
    ;(dialog.querySelector('button[aria-label="close"]') as HTMLButtonElement).click()
    await support.vm.$nextTick()
    expect(document.querySelector('[role="dialog"]')).toBeNull()
    expect(document.body.style.overflow).toBe('')
  }
  await triggers[0]!.trigger('click')
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
  await support.vm.$nextTick()
  expect(document.querySelector('[role="dialog"]')).toBeNull()
  await triggers[0]!.trigger('click')
  ;(document.querySelector('[role="dialog"]')!.parentElement as HTMLElement).click()
  await support.vm.$nextTick()
  expect(document.querySelector('[role="dialog"]')).toBeNull()
  support.unmount()
})
