import { normalizeGoogleRedirectState } from './googleRedirect'

export interface GoogleCredentialResponse {
  credential?: string
}

export interface GoogleIdentityConfiguration {
  client_id: string
  callback?: (response: GoogleCredentialResponse) => void
  ux_mode?: 'popup' | 'redirect'
  login_uri?: string
  auto_select?: boolean
  cancel_on_tap_outside?: boolean
  use_fedcm_for_button?: boolean
}

export interface GoogleIdentityButtonConfiguration {
  type: 'standard' | 'icon'
  theme?: 'outline' | 'filled_blue' | 'filled_black'
  size?: 'large' | 'medium' | 'small'
  text?: 'signin_with' | 'signup_with' | 'continue_with' | 'signin'
  shape?: 'rectangular' | 'pill' | 'circle' | 'square'
  logo_alignment?: 'left' | 'center'
  width?: string
  locale?: string
  state?: string
}

export interface GoogleAccountsID {
  initialize: (configuration: GoogleIdentityConfiguration) => void
  renderButton: (parent: HTMLElement, configuration: GoogleIdentityButtonConfiguration) => void
  cancel?: () => void
}


const SCRIPT_URL = 'https://accounts.google.com/gsi/client'
const SCRIPT_ID = 'google-identity-services'
/** GIS script load timeout (ms). */
const SCRIPT_TIMEOUT_MS = 10_000
/** GIS renders buttons at most 400px wide. */
const BUTTON_MAX_WIDTH = 400

let scriptPromise: Promise<GoogleAccountsID> | null = null

export interface NavigatorLike {
  userAgent?: unknown
  platform?: unknown
  maxTouchPoints?: unknown
}

export const isIOSOrIPadOS = (source: NavigatorLike): boolean => {
  const ua = String(source.userAgent || '')
  const platform = String(source.platform || '')
  const touch = Number(source.maxTouchPoints || 0)
  return /iPad|iPhone|iPod/i.test(ua) || ((/Macintosh/i.test(ua) || /Mac/i.test(platform)) && touch > 1)
}

/** iOS blocks the GIS popup reliably → redirect mode there. */
export const resolveGoogleIdentityUXMode = (source: NavigatorLike): 'popup' | 'redirect' => (isIOSOrIPadOS(source) ? 'redirect' : 'popup')

export const detectGoogleIdentityUXMode = (): 'popup' | 'redirect' =>
  typeof navigator === 'undefined' ? 'popup' : resolveGoogleIdentityUXMode(navigator)

export const resolveGoogleButtonWidth = (available: unknown): number | undefined => {
  const width = Number(available)
  if (!Number.isFinite(width) || width <= 0) return undefined
  return Math.min(BUTTON_MAX_WIDTH, Math.floor(width))
}

const LOCALE_MAP: Record<string, string> = { 'zh-CN': 'zh_CN', 'zh-TW': 'zh_TW', 'en-US': 'en_US' }
export const resolveGoogleButtonLocale = (locale: unknown): string => {
  const normalized = String(locale || '').trim()
  return LOCALE_MAP[normalized] || normalized
}

const accountsId = (): GoogleAccountsID | null =>
  typeof window === 'undefined' ? null : (window as unknown as { google?: { accounts?: { id?: GoogleAccountsID } } }).google?.accounts?.id || null

export const loadGoogleIdentityScript = (): Promise<GoogleAccountsID> => {
  const existing = accountsId()
  if (existing) return Promise.resolve(existing)
  if (scriptPromise) return scriptPromise
  let script = document.getElementById(SCRIPT_ID) as HTMLScriptElement | null
  const append = !script
  if (!script) {
    script = document.createElement('script')
    script.id = SCRIPT_ID
    script.src = SCRIPT_URL
    script.async = true
    script.defer = true
  }
  const el = script
  scriptPromise = new Promise<GoogleAccountsID>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      el.remove()
      reject(new Error('Google Identity Services load timed out'))
    }, SCRIPT_TIMEOUT_MS)
    el.addEventListener(
      'load',
      () => {
        window.clearTimeout(timer)
        const id = accountsId()
        if (id) resolve(id)
        else reject(new Error('Google Identity Services is unavailable'))
      },
      { once: true },
    )
    el.addEventListener(
      'error',
      () => {
        window.clearTimeout(timer)
        el.remove()
        reject(new Error('Failed to load Google Identity Services'))
      },
      { once: true },
    )
  }).catch((error: unknown) => {
    scriptPromise = null
    throw error
  })
  if (append) document.head.appendChild(el)
  return scriptPromise
}

export const createGoogleIdentityConfiguration = (
  clientId: string,
  onCredential: (credential: string) => void,
  onInvalid: () => void,
  options: { uxMode?: 'popup' | 'redirect'; loginUri?: string } = {},
): GoogleIdentityConfiguration => {
  if (options.uxMode === 'redirect') {
    const loginUri = String(options.loginUri || '').trim()
    if (!loginUri) throw new Error('Google redirect login URI is missing')
    return { client_id: clientId.trim(), ux_mode: 'redirect', login_uri: loginUri, auto_select: false }
  }
  return {
    client_id: clientId.trim(),
    callback: (response) => {
      const credential = String(response?.credential || '').trim()
      if (!credential) onInvalid()
      else onCredential(credential)
    },
    ux_mode: 'popup',
    auto_select: false,
    cancel_on_tap_outside: true,
    use_fedcm_for_button: true,
  }
}

export const createGoogleButtonConfiguration = (options: { locale?: string; width?: number; state?: string; text?: 'signin_with' | 'continue_with' }): GoogleIdentityButtonConfiguration => {
  const width = resolveGoogleButtonWidth(options.width)
  const locale = resolveGoogleButtonLocale(options.locale)
  const state = normalizeGoogleRedirectState(options.state)
  return {
    type: 'standard',
    theme: 'outline',
    size: 'large',
    text: options.text || 'signin_with',
    shape: 'pill',
    logo_alignment: 'left',
    ...(width ? { width: String(width) } : {}),
    ...(locale ? { locale } : {}),
    ...(state ? { state } : {}),
  }
}
