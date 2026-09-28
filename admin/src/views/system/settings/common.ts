import { ApiError } from '@/api/client'
import i18n from '@/i18n'
import { notifyError } from '@/utils/notify'

export const tr = (key: string, params?: Record<string, unknown>) => (params ? i18n.global.t(key, params) : i18n.global.t(key))

/** API errors are toasted by the client already; only surface unexpected errors. */
export const notifyFailure = (err: unknown, fallbackKey = 'admin.settings.alerts.saveFailed') => {
  if (err instanceof ApiError && err.notified) return
  notifyError(err instanceof Error && err.message ? err.message : tr(fallbackKey))
}
