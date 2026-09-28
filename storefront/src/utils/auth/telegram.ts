import type { TelegramAuthPayload } from '@/api/types'

interface TelegramWebAppLike {
  initData?: string
  ready?: () => void
  openLink?: (url: string, options?: { try_instant_view?: boolean }) => void
}

/** Avoids a global Window augmentation (other modules may declare their own). */
const telegramWebApp = (): TelegramWebAppLike | undefined =>
  typeof window === 'undefined' ? undefined : (window as unknown as { Telegram?: { WebApp?: TelegramWebAppLike } }).Telegram?.WebApp

/** Raw initData when running inside a Telegram Mini App, else ''. */
export const getTelegramMiniAppInitData = (): string => {
  if (typeof window === 'undefined') return ''
  return String(telegramWebApp()?.initData || '').trim()
}

/** Validates the Telegram login widget callback payload. */
export const buildTelegramPayload = (raw: unknown): TelegramAuthPayload | null => {
  if (!raw || typeof raw !== 'object') return null
  const r = raw as Record<string, unknown>
  const id = Number(r.id)
  const authDate = Number(r.auth_date)
  const hash = String(r.hash || '').trim()
  if (!Number.isFinite(id) || id <= 0 || !Number.isFinite(authDate) || authDate <= 0 || !hash) return null
  return {
    id,
    first_name: String(r.first_name || '').trim(),
    last_name: String(r.last_name || '').trim(),
    username: String(r.username || '').trim(),
    photo_url: String(r.photo_url || '').trim(),
    auth_date: authDate,
    hash,
  }
}

/** Entry link into the bot's Mini App (same format as the original storefront). */
export const buildTelegramMiniAppEntryLink = (botUsername: string, miniAppUrl: string): string => {
  const bot = botUsername.trim().replace(/^@+/, '')
  if (!bot || !miniAppUrl.trim()) return ''
  return `https://telegram.me/${bot}/webapp`
}

/** Telegram may append WebApp params to the query string or hash. */
export const isTelegramUrlEnvironment = (): boolean => {
  if (typeof window === 'undefined') return false
  const url = `${window.location.search || ''}${window.location.hash || ''}`
  return url.includes('tgWebAppData') || url.includes('tgWebAppVersion') || url.includes('tgWebAppPlatform')
}

export const openTelegramCompatibleLink = (url: string) => {
  const target = url.trim()
  if (!target) return
  const webApp = telegramWebApp()
  if (webApp?.openLink) webApp.openLink(target, { try_instant_view: false })
  else window.open(target, '_blank', 'noopener')
}
