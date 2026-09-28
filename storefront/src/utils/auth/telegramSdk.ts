import { getTelegramMiniAppInitData, isTelegramUrlEnvironment } from './telegram'

const SDK_TIMEOUT_MS = 3000
let request: Promise<void> | null = null

/** Telegram is optional: ordinary visits never request its SDK and failure never blocks startup. */
export function loadTelegramSdk(): Promise<void> {
  if (!isTelegramUrlEnvironment() || getTelegramMiniAppInitData()) return Promise.resolve()
  if (request) return request
  request = new Promise<void>((resolve) => {
    const script = document.createElement('script')
    const finish = () => {
      clearTimeout(timer)
      script.onload = null
      script.onerror = null
      resolve()
    }
    const timer = setTimeout(finish, SDK_TIMEOUT_MS)
    script.async = true
    script.src = 'https://telegram.org/js/telegram-web-app.js'
    script.onload = finish
    script.onerror = finish
    document.head.appendChild(script)
  })
  return request
}
