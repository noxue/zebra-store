import { afterEach, beforeEach, expect, it, vi } from 'vitest'

beforeEach(() => { vi.resetModules(); vi.useFakeTimers(); history.replaceState(null, '', '/') })
afterEach(() => {
  document.querySelectorAll('script[src*="telegram.org"]').forEach((s) => s.remove())
  history.replaceState(null, '', '/')
  vi.useRealTimers(); vi.restoreAllMocks()
})
it('FE-07 never loads Telegram for ordinary visitors', async () => {
  const { loadTelegramSdk } = await import('@/utils/auth/telegramSdk')
  await loadTelegramSdk()
  expect(document.querySelector('script[src*="telegram.org"]')).toBeNull()
})
it.each(['?tgWebAppData=x', '#tgWebAppData=x'])('FE-07 / FE-15 detects %s and degrades on SDK failure', async (suffix) => {
  history.replaceState(null, '', `/${suffix}`)
  const { loadTelegramSdk } = await import('@/utils/auth/telegramSdk')
  const first = loadTelegramSdk()
  expect(loadTelegramSdk()).toBe(first)
  const script = document.querySelector('script[src*="telegram.org"]')!
  expect(script).not.toBeNull()
  script.dispatchEvent(new Event('error'))
  await first
  expect(vi.getTimerCount()).toBe(0)
})
it('FE-07 finishes startup after three seconds if the SDK never responds', async () => {
  history.replaceState(null, '', '/?tgWebAppVersion=8')
  const { loadTelegramSdk } = await import('@/utils/auth/telegramSdk')
  const done = vi.fn()
  const pending = loadTelegramSdk().then(done)
  await vi.advanceTimersByTimeAsync(2999)
  expect(done).not.toHaveBeenCalled()
  await vi.advanceTimersByTimeAsync(1)
  await pending
  expect(done).toHaveBeenCalledOnce()
})
