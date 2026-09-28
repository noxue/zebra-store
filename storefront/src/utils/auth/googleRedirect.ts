/** Google GIS redirect-mode helpers (ported from the original storefront). */
export const GOOGLE_REDIRECT_CREDENTIAL_CALLBACK_PATH = '/auth/google/redirect/callback'
export const GOOGLE_REDIRECT_FRONTEND_CALLBACK_PATH = '/auth/google/callback'
export const GOOGLE_REDIRECT_INTENT_STORAGE_KEY = 'google_redirect_intent'
/** Intent lifetime: 10 minutes (backend state TTL). */
export const GOOGLE_REDIRECT_INTENT_TTL_MS = 10 * 60 * 1000
/** Refresh the prepared state after 8 minutes so it never expires on screen. */
export const GOOGLE_REDIRECT_INTENT_REFRESH_MS = 8 * 60 * 1000

export type GoogleRedirectFlow = 'login' | 'bind'

export const GOOGLE_REDIRECT_ERRORS = [
  'invalid_request',
  'csrf_mismatch',
  'session_expired',
  'tenant_mismatch',
  'auth_disabled',
  'configuration_error',
  'credential_invalid',
  'credential_expired',
  'email_unverified',
  'service_unavailable',
  'internal_error',
] as const
export type GoogleRedirectError = (typeof GOOGLE_REDIRECT_ERRORS)[number]

export interface GoogleRedirectCallback {
  flow: GoogleRedirectFlow
  error: GoogleRedirectError | null
}

export interface GoogleRedirectIntent {
  flow: GoogleRedirectFlow
  returnPath: string
  issuedAt: number
}

export interface GoogleRedirectPreparedIntent {
  state: string
  issuedAt: number
}

export interface StorageLike {
  getItem(key: string): string | null
  setItem(key: string, value: string): void
  removeItem(key: string): void
}

const FLOWS = new Set<string>(['login', 'bind'])
const ERROR_SET = new Set<string>(GOOGLE_REDIRECT_ERRORS)
// 32 random bytes as canonical unpadded base64url.
const STATE_PATTERN = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/
const CALLBACK_QUERY_KEYS = new Set(['flow', 'error'])
const DEFAULT_LOGIN_RETURN_PATH = '/me/orders'
const BIND_RETURN_PATH = '/me/security'

/** Only same-site absolute paths are allowed as post-login redirects. */
export const normalizeReturnPath = (value: unknown, fallback = DEFAULT_LOGIN_RETURN_PATH): string => {
  const path = typeof value === 'string' ? value.trim() : ''
  if (!path.startsWith('/') || path.startsWith('//') || path.includes('\\')) return fallback
  return path
}

export const createGoogleRedirectIntent = (flow: GoogleRedirectFlow, returnPath: unknown, issuedAt = Date.now()): GoogleRedirectIntent => ({
  flow,
  returnPath: flow === 'bind' ? BIND_RETURN_PATH : normalizeReturnPath(returnPath),
  issuedAt,
})

export const storeGoogleRedirectIntent = (storage: StorageLike | null | undefined, intent: GoogleRedirectIntent): boolean => {
  if (!storage) return false
  try {
    storage.setItem(GOOGLE_REDIRECT_INTENT_STORAGE_KEY, JSON.stringify(intent))
    return true
  } catch {
    return false
  }
}

/** Read-once: removed before interpretation so stale intents can't be retried. */
export const consumeGoogleRedirectIntent = (storage: StorageLike | null | undefined, now = Date.now()): GoogleRedirectIntent | null => {
  if (!storage) return null
  let raw: string
  try {
    raw = storage.getItem(GOOGLE_REDIRECT_INTENT_STORAGE_KEY) || ''
    storage.removeItem(GOOGLE_REDIRECT_INTENT_STORAGE_KEY)
  } catch {
    return null
  }
  if (!raw) return null
  try {
    const parsed = JSON.parse(raw) as Partial<GoogleRedirectIntent>
    if (!FLOWS.has(String(parsed.flow))) return null
    const issuedAt = Number(parsed.issuedAt)
    if (!Number.isFinite(issuedAt)) return null
    const age = now - issuedAt
    if (age < 0 || age >= GOOGLE_REDIRECT_INTENT_TTL_MS) return null
    return createGoogleRedirectIntent(parsed.flow as GoogleRedirectFlow, parsed.returnPath, issuedAt)
  } catch {
    return null
  }
}

export const resolveGoogleRedirectIntentRefreshDelay = (issuedAt: number, now = Date.now()): number => {
  if (!Number.isFinite(issuedAt) || !Number.isFinite(now)) return 0
  return Math.max(0, GOOGLE_REDIRECT_INTENT_REFRESH_MS - Math.max(0, now - issuedAt))
}

export const normalizeGoogleRedirectState = (value: unknown): string => {
  const state = typeof value === 'string' ? value.trim() : ''
  return STATE_PATTERN.test(state) ? state : ''
}

export const createGoogleRedirectPreparedIntent = (responseData: unknown, issuedAt = Date.now()): GoogleRedirectPreparedIntent | null => {
  if (!responseData || typeof responseData !== 'object' || !Number.isFinite(issuedAt)) return null
  const state = normalizeGoogleRedirectState((responseData as { state?: unknown }).state)
  return state ? { state, issuedAt } : null
}

/** Callback URL may only carry `flow` and (known) `error`. */
export const parseGoogleRedirectCallbackQuery = (query: Record<string, unknown>): GoogleRedirectCallback | null => {
  if (Object.keys(query).some((key) => !CALLBACK_QUERY_KEYS.has(key))) return null
  const flow = query.flow
  if (typeof flow !== 'string' || !FLOWS.has(flow)) return null
  const rawError = query.error
  if (rawError === undefined || rawError === null) return { flow: flow as GoogleRedirectFlow, error: null }
  if (typeof rawError !== 'string' || !ERROR_SET.has(rawError)) return null
  return { flow: flow as GoogleRedirectFlow, error: rawError as GoogleRedirectError }
}

export const shouldResumeGoogleRedirect2FA = (marker: unknown, challengeToken: unknown): boolean =>
  marker === '1' && typeof challengeToken === 'string' && challengeToken.trim() !== ''

/** Absolute backend URL GIS posts the credential to (same-origin API only). */
export const buildGoogleRedirectCredentialCallbackURL = (apiBaseURL: string, pageOrigin: string): string => {
  const origin = pageOrigin.trim()
  if (!origin) throw new Error('Page origin is required for Google redirect mode')
  const pageURL = new URL(origin)
  const configured = apiBaseURL.trim()
  let basePath = ''
  if (configured) {
    const configuredURL = new URL(configured, pageURL)
    if (configuredURL.origin !== pageURL.origin) throw new Error('Google redirect mode requires a same-origin API')
    if (configuredURL.search || configuredURL.hash) throw new Error('Google redirect API base must not contain a query or fragment')
    basePath = configuredURL.pathname.replace(/\/+$/, '')
  }
  return new URL(`${basePath}/api/v1${GOOGLE_REDIRECT_CREDENTIAL_CALLBACK_PATH}`, pageURL).toString()
}

export const tryBuildGoogleRedirectCredentialCallbackURL = (): string => {
  try {
    return buildGoogleRedirectCredentialCallbackURL(import.meta.env.VITE_API_BASE_URL || '', window.location.origin)
  } catch {
    return ''
  }
}

export const getSessionStorage = (): Storage | null => {
  try {
    return window.sessionStorage
  } catch {
    return null
  }
}
