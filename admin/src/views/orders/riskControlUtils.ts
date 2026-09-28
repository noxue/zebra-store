export interface RateLimitForm {
  enabled: boolean
  window_seconds: number | ''
  max_requests: number | ''
  block_seconds: number | ''
}

export interface RiskControlForm {
  enabled: boolean
  common: { ip_blacklist_text: string }
  guest: {
    enabled: boolean
    max_pending_orders_per_ip: number | ''
    max_quantity_per_product_per_order: number | ''
    max_pending_quantity_per_ip_product: number | ''
    payment_expire_minutes: number | ''
    rate_limit: RateLimitForm
  }
  member: {
    enabled: boolean
    max_pending_orders_per_user: number | ''
    max_pending_orders_per_ip: number | ''
    max_quantity_per_product_per_order: number | ''
    rate_limit: RateLimitForm
  }
}

type Rec = Record<string, unknown>
const asRec = (v: unknown): Rec | undefined => (v && typeof v === 'object' && !Array.isArray(v) ? (v as Rec) : undefined)

export const defaultRateLimit = (guest: boolean): RateLimitForm => ({
  enabled: guest,
  window_seconds: 60,
  max_requests: guest ? 3 : 10,
  block_seconds: 120,
})

export const recommendedGuestPolicy = (): RiskControlForm['guest'] => ({
  enabled: true,
  max_pending_orders_per_ip: 2,
  max_quantity_per_product_per_order: 1,
  max_pending_quantity_per_ip_product: 2,
  payment_expire_minutes: 10,
  rate_limit: defaultRateLimit(true),
})

export const defaultRiskControlForm = (): RiskControlForm => ({
  enabled: false,
  common: { ip_blacklist_text: '' },
  guest: recommendedGuestPolicy(),
  member: {
    enabled: true,
    max_pending_orders_per_user: 5,
    max_pending_orders_per_ip: 0,
    max_quantity_per_product_per_order: 0,
    rate_limit: defaultRateLimit(false),
  },
})

const num = (value: unknown, fallback: number) => {
  if (value === null || value === undefined || value === '') return fallback
  const n = Number(value)
  return Number.isFinite(n) ? n : fallback
}

const bool = (value: unknown, fallback: boolean) => (value != null ? !!value : fallback)

const strList = (value: unknown) => (Array.isArray(value) ? value.map((v) => String(v)) : [])

const rateLimitFrom = (raw: unknown, d: RateLimitForm): RateLimitForm => {
  const r = asRec(raw)
  return {
    enabled: bool(r?.enabled, d.enabled),
    window_seconds: num(r?.window_seconds, Number(d.window_seconds)),
    max_requests: num(r?.max_requests, Number(d.max_requests)),
    block_seconds: num(r?.block_seconds, Number(d.block_seconds)),
  }
}

/** Parse the stored `order_risk_control_config` (v2 nested, or legacy flat) into the form model. */
export const parseRiskControlConfig = (raw: unknown): RiskControlForm => {
  const data = asRec(raw)
  const form = defaultRiskControlForm()
  if (!data || Object.keys(data).length === 0) return form

  const guest = asRec(data.guest)
  const member = asRec(data.member)
  if (guest && member) {
    const common = asRec(data.common)
    form.enabled = !!data.enabled
    form.common.ip_blacklist_text = strList(common?.ip_blacklist).join('\n')
    form.guest = {
      enabled: bool(guest.enabled, true),
      max_pending_orders_per_ip: num(guest.max_pending_orders_per_ip, 2),
      max_quantity_per_product_per_order: num(guest.max_quantity_per_product_per_order, 1),
      max_pending_quantity_per_ip_product: num(guest.max_pending_quantity_per_ip_product, 2),
      payment_expire_minutes: num(guest.payment_expire_minutes, 10),
      rate_limit: rateLimitFrom(guest.rate_limit, defaultRateLimit(true)),
    }
    form.member = {
      enabled: bool(member.enabled, true),
      max_pending_orders_per_user: num(member.max_pending_orders_per_user, 5),
      max_pending_orders_per_ip: num(member.max_pending_orders_per_ip, 0),
      max_quantity_per_product_per_order: num(member.max_quantity_per_product_per_order, 0),
      rate_limit: rateLimitFrom(member.rate_limit, defaultRateLimit(false)),
    }
    return form
  }

  // legacy flat config
  const legacy: RateLimitForm = { enabled: false, window_seconds: 60, max_requests: 5, block_seconds: 120 }
  form.enabled = !!data.enabled
  form.common.ip_blacklist_text = strList(data.ip_blacklist).join('\n')
  form.guest = {
    enabled: true,
    max_pending_orders_per_ip: num(data.max_pending_orders_per_ip, 5),
    max_quantity_per_product_per_order: 0,
    max_pending_quantity_per_ip_product: 0,
    payment_expire_minutes: 0,
    rate_limit: rateLimitFrom(data.order_rate_limit, legacy),
  }
  form.member = {
    enabled: true,
    max_pending_orders_per_user: num(data.max_pending_orders_per_user, 3),
    max_pending_orders_per_ip: num(data.max_pending_orders_per_ip, 5),
    max_quantity_per_product_per_order: 0,
    rate_limit: rateLimitFrom(data.order_rate_limit, legacy),
  }
  return form
}

export const normalizeLines = (value: string) =>
  Array.from(
    new Set(
      value
        .split('\n')
        .map((l) => l.trim())
        .filter(Boolean),
    ),
  )

const rateLimitPayload = (r: RateLimitForm) => ({
  enabled: r.enabled,
  window_seconds: Number(r.window_seconds),
  max_requests: Number(r.max_requests),
  block_seconds: Number(r.block_seconds),
})

/** Build the v2 settings value written back to `order_risk_control_config`. */
export const buildRiskControlPayload = (form: RiskControlForm) => ({
  version: 2,
  enabled: form.enabled,
  common: { ip_blacklist: normalizeLines(form.common.ip_blacklist_text) },
  guest: {
    enabled: form.guest.enabled,
    max_pending_orders_per_ip: Number(form.guest.max_pending_orders_per_ip),
    max_quantity_per_product_per_order: Number(form.guest.max_quantity_per_product_per_order),
    max_pending_quantity_per_ip_product: Number(form.guest.max_pending_quantity_per_ip_product),
    payment_expire_minutes: Number(form.guest.payment_expire_minutes),
    rate_limit: rateLimitPayload(form.guest.rate_limit),
  },
  member: {
    enabled: form.member.enabled,
    max_pending_orders_per_user: Number(form.member.max_pending_orders_per_user),
    max_pending_orders_per_ip: Number(form.member.max_pending_orders_per_ip),
    max_quantity_per_product_per_order: Number(form.member.max_quantity_per_product_per_order),
    rate_limit: rateLimitPayload(form.member.rate_limit),
  },
})
