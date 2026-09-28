import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import { configAPI } from '@/api/catalog'
import { installNavigationFeedback } from '@/router/navigation'
import { useAppStore } from '@/stores/app'
import { toast } from '@/composables/useToast'

beforeEach(() => { setActivePinia(createPinia()) })
afterEach(() => { vi.restoreAllMocks() })

it('FE-23 releases navigation loading and reports a failed page chunk', async () => {
  let rejectChunk!: (reason: Error) => void
  const router = createRouter({ history: createMemoryHistory(), routes: [
    { path: '/', component: { render: () => null } },
    { path: '/broken', component: () => new Promise((_, reject) => { rejectChunk = reject }) },
  ] })
  installNavigationFeedback(router)
  await router.push('/')
  const notify = vi.spyOn(toast, 'error')
  const navigation = router.push('/broken')
  const result = expect(navigation).rejects.toThrow('chunk 404')
  await vi.waitFor(() => expect(rejectChunk).toBeTypeOf('function'))
  expect(useAppStore().navigating).toBe(true)
  rejectChunk(new Error('chunk 404'))
  await result
  expect(useAppStore().navigating).toBe(false)
  expect(notify).toHaveBeenCalledOnce()
  await router.push('/')
  expect(useAppStore().navigating).toBe(false)
})

it('FE-23 coalesces concurrent config requests and permits a later retry after failure', async () => {
  const request = vi.spyOn(configAPI, 'get').mockRejectedValueOnce(new Error('offline')).mockResolvedValue({ status_code: 0, msg: 'ok', data: { brand: { site_name: 'Test' } } })
  vi.spyOn(console, 'warn').mockImplementation(() => {})
  const app = useAppStore()
  await Promise.all([app.loadConfig(), app.loadConfig(), app.loadConfig(true)])
  expect(request).toHaveBeenCalledOnce()
  expect(app.loading).toBe(false)
  await Promise.all([app.loadConfig(), app.loadConfig()])
  expect(request).toHaveBeenCalledTimes(2)
  expect(app.siteName).toBe('Test')
})
