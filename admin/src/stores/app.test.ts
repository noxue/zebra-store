import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { adminAPI } from '@/api/admin'
import { useAppStore } from './app'

beforeEach(() => {
  setActivePinia(createPinia())
  localStorage.clear()
  document.documentElement.className = ''
  document.documentElement.removeAttribute('style')
  document.body.className = ''
})

afterEach(() => vi.restoreAllMocks())

it('keeps admin colours and mode independent of the storefront theme', async () => {
  const app = useAppStore()
  app.applyTheme('light')
  vi.spyOn(adminAPI, 'getPublicConfig').mockResolvedValue({
    status_code: 0,
    msg: '',
    data: {
      brand: { site_name: 'Test shop' },
      theme: {
        primary_color: '#123456',
        background_color: '#abcdef',
        background_image: '/uploads/shop.jpg',
        default_mode: 'dark',
      },
    },
  })

  await app.loadConfig()

  expect(app.siteName).toBe('Test shop')
  expect(app.theme).toBe('light')
  expect(document.documentElement.classList.contains('dark')).toBe(false)
  expect(document.documentElement.style.getPropertyValue('--zs-primary')).toBe('')
  expect(document.documentElement.style.getPropertyValue('--zs-bg')).toBe('')
  expect(document.body.classList.contains('zs-custom-bg')).toBe(false)
})
