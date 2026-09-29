export interface GuestOrderAuth {
  email: string
  order_password: string
}

const GUEST_ORDER_AUTH_KEY = 'guest_order_auth'
const GUEST_ORDER_DRAFT_KEY = 'guest_order_auth_draft'
const GENERATED_EMAIL_SUFFIX = '@uuid.com'
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
const EMPTY_GUEST_ORDER_AUTH: GuestOrderAuth = { email: '', order_password: '' }

let volatileGuestOrderAuth: GuestOrderAuth | null = null
let volatileGuestOrderDraft: GuestOrderAuth | null = null

const normalizeGuestOrderAuth = (auth: Partial<GuestOrderAuth>): GuestOrderAuth => ({
  email: typeof auth.email === 'string' ? auth.email.trim() : '',
  order_password: typeof auth.order_password === 'string' ? auth.order_password : '',
})

const clone = (auth: GuestOrderAuth): GuestOrderAuth => ({ ...auth })

const parse = (raw: string | null): GuestOrderAuth | null => {
  if (!raw) return null
  try {
    return normalizeGuestOrderAuth(JSON.parse(raw) as Partial<GuestOrderAuth>)
  } catch {
    return null
  }
}

const read = (key: string): string | null => {
  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

const write = (key: string, value: GuestOrderAuth) => {
  try {
    window.localStorage.setItem(key, JSON.stringify(value))
  } catch {
    // 浏览器禁用存储时，当前页面仍使用内存中的身份。
  }
}

const remove = (key: string) => {
  try {
    window.localStorage.removeItem(key)
  } catch {
    // 浏览器禁用存储时只清理内存状态。
  }
}

const randomUuid = (): string => {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  const bytes = new Uint8Array(16)
  crypto.getRandomValues(bytes)
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  bytes[8] = (bytes[8] & 0x3f) | 0x80
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

export const isGeneratedGuestOrderAuth = (auth: GuestOrderAuth): boolean => {
  const email = auth.email.toLowerCase()
  if (!email.endsWith(GENERATED_EMAIL_SUFFIX)) return false
  const uuid = email.slice(0, -GENERATED_EMAIL_SUFFIX.length)
  return UUID_PATTERN.test(uuid) && auth.order_password.toLowerCase() === uuid
}

export const createGeneratedGuestOrderAuth = (): GuestOrderAuth => {
  const uuid = randomUuid()
  return { email: `${uuid}${GENERATED_EMAIL_SUFFIX}`, order_password: uuid }
}

export const loadGuestOrderAuth = (): GuestOrderAuth => {
  if (typeof window === 'undefined') return clone(EMPTY_GUEST_ORDER_AUTH)
  if (!volatileGuestOrderAuth) volatileGuestOrderAuth = parse(read(GUEST_ORDER_AUTH_KEY))
  return clone(volatileGuestOrderAuth || EMPTY_GUEST_ORDER_AUTH)
}

export const ensureGuestOrderAuth = (): GuestOrderAuth => {
  const current = loadGuestOrderAuth()
  if (current.email && current.order_password) return current
  const generated = createGeneratedGuestOrderAuth()
  saveGuestOrderAuth(generated)
  return generated
}

export const saveGuestOrderAuth = (auth: GuestOrderAuth) => {
  const normalized = normalizeGuestOrderAuth(auth)
  volatileGuestOrderAuth = clone(normalized)
  if (typeof window !== 'undefined') write(GUEST_ORDER_AUTH_KEY, normalized)
}

export const loadGuestOrderDraft = (): GuestOrderAuth => {
  if (typeof window === 'undefined') return clone(EMPTY_GUEST_ORDER_AUTH)
  if (!volatileGuestOrderDraft) volatileGuestOrderDraft = parse(read(GUEST_ORDER_DRAFT_KEY))
  return clone(volatileGuestOrderDraft || EMPTY_GUEST_ORDER_AUTH)
}

export const saveGuestOrderDraft = (auth: GuestOrderAuth) => {
  const normalized = normalizeGuestOrderAuth(auth)
  volatileGuestOrderDraft = clone(normalized)
  if (typeof window !== 'undefined') write(GUEST_ORDER_DRAFT_KEY, normalized)
}

export const clearGuestOrderDraft = () => {
  volatileGuestOrderDraft = null
  if (typeof window !== 'undefined') remove(GUEST_ORDER_DRAFT_KEY)
}

export const clearGuestOrderAuth = () => {
  volatileGuestOrderAuth = null
  volatileGuestOrderDraft = null
  if (typeof window === 'undefined') return
  remove(GUEST_ORDER_AUTH_KEY)
  remove(GUEST_ORDER_DRAFT_KEY)
  try {
    window.sessionStorage.removeItem(GUEST_ORDER_AUTH_KEY)
  } catch {
    // 兼容清理旧版 sessionStorage 数据。
  }
}
