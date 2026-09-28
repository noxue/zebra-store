interface TelegramWebAppLike {
  initData?: string
  openLink?: (url: string) => void
}

const webApp = (): TelegramWebAppLike | undefined => {
  if (typeof window === 'undefined') return undefined
  return (window as unknown as { Telegram?: { WebApp?: TelegramWebAppLike } }).Telegram?.WebApp
}

/** True inside a Telegram Mini App (WebApp with initData). */
export const isTelegramMiniApp = (): boolean => Boolean(webApp()?.initData)

/** Opens a link the Telegram-compatible way (falls back to a new window). */
export const openTelegramLink = (url: string) => {
  const app = webApp()
  if (app?.openLink) app.openLink(url)
  else window.open(url, '_blank', 'noopener')
}
