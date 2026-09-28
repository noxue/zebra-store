export const CALLBACK_ROUTE_KEYS = [
  'payment_callback',
  'dujiaopay_webhook',
  'paypal_webhook',
  'stripe_webhook',
  'upstream_callback',
] as const

export type CallbackRouteKey = (typeof CALLBACK_ROUTE_KEYS)[number]

export type CallbackRoutesValue = Record<CallbackRouteKey, string>

export const DEFAULT_CALLBACK_ROUTE_PATHS: CallbackRoutesValue = {
  payment_callback: '/api/v1/payments/callback',
  dujiaopay_webhook: '/api/v1/payments/webhook/dujiaopay',
  paypal_webhook: '/api/v1/payments/webhook/paypal',
  stripe_webhook: '/api/v1/payments/webhook/stripe',
  upstream_callback: '/api/v1/upstream/callback',
}

export const normalizeCallbackRouteInput = (value: unknown): string => {
  return String(value || '').trim().replace(/\/+$/, '')
}

export const getCallbackRouteDisplayValue = (key: CallbackRouteKey, value: unknown): string => {
  return normalizeCallbackRouteInput(value) || DEFAULT_CALLBACK_ROUTE_PATHS[key]
}

export const toCallbackRouteSaveValue = (key: CallbackRouteKey, value: unknown): string => {
  const normalized = normalizeCallbackRouteInput(value)
  if (normalized === DEFAULT_CALLBACK_ROUTE_PATHS[key]) {
    return ''
  }
  return normalized
}

export const buildCallbackRoutesSavePayload = (value: Partial<Record<CallbackRouteKey, unknown>>): CallbackRoutesValue => {
  return CALLBACK_ROUTE_KEYS.reduce((payload, key) => {
    payload[key] = toCallbackRouteSaveValue(key, value[key])
    return payload
  }, {} as CallbackRoutesValue)
}

export const RESERVED_CALLBACK_ROUTE_PREFIXES = [
  '/api/v1/public/',
  '/api/v1/admin/',
  '/api/v1/auth/',
  '/api/v1/guest/',
  '/api/v1/channel/',
  '/api/v1/upstream/api/',
  '/api/v1/user/',
] as const

export type CallbackRouteError = 'mustStartWithApi' | 'conflictWithSystem' | 'duplicatePath'

/**
 * Validate the save payload exactly like the original CallbackRoutes.vue:
 * non-empty paths must start with `/api/`, must not overlap a reserved prefix and must be unique.
 * Returns the first error (i18n suffix under `admin.settings.callbackRoutes`) or null.
 */
export const validateCallbackRoutes = (payload: CallbackRoutesValue): CallbackRouteError | null => {
  const seen: string[] = []
  for (const key of CALLBACK_ROUTE_KEYS) {
    const v = payload[key]
    if (!v) continue
    if (!v.startsWith('/api/')) return 'mustStartWithApi'
    const withSlash = `${v}/`
    if (RESERVED_CALLBACK_ROUTE_PREFIXES.some((p) => withSlash.startsWith(p) || p.startsWith(withSlash))) {
      return 'conflictWithSystem'
    }
    if (seen.includes(v)) return 'duplicatePath'
    seen.push(v)
  }
  return null
}
