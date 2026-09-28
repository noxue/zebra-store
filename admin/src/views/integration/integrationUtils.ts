import type { BadgeTone } from '@/components/ui'
import type { AdminSiteConnection, Money, SiteConnectionConfig, SiteConnectionProtocolDef, SiteConnectionProtocolField } from '@/api/types'
import { getLocalizedText } from '@/utils/format'

// ---------------------------------------------------------------------------
// Shared
// ---------------------------------------------------------------------------

export const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v)

/** Parse any money-ish value into a finite number (0 when invalid). */
export const parseMoneyValue = (value: unknown): number => {
  if (value === null || value === undefined || value === '') return 0
  const num = Number(value)
  return Number.isFinite(num) ? num : 0
}

/** "a ~ b" range of positive prices, falling back to `fallback` (or "-"). Same rules as the original. */
export const priceRange = (prices: Array<Money | null | undefined>, fallback?: Money | null): string => {
  const values = prices.map((p) => parseFloat(String(p))).filter((v) => !Number.isNaN(v) && v > 0)
  if (values.length === 0) return fallback !== undefined && fallback !== null && fallback !== '' ? String(fallback) : '-'
  const min = Math.min(...values)
  const max = Math.max(...values)
  return min === max ? `${min}` : `${min} ~ ${max}`
}

/** "color: red / size: L" from a SKU spec_values object (values may be localized objects). */
export const formatSpecValues = (specValues: unknown): string => {
  if (!isRecord(specValues)) return '-'
  const entries = Object.entries(specValues)
  if (entries.length === 0) return '-'
  return entries.map(([k, v]) => `${k}: ${getLocalizedText(v)}`).join(' / ')
}

/** Split an array into chunks of `size`. */
export const chunk = <T>(list: T[], size: number): T[][] => {
  const out: T[][] = []
  for (let i = 0; i < list.length; i += size) out.push(list.slice(i, i + size))
  return out
}

// ---------------------------------------------------------------------------
// Site connections
// ---------------------------------------------------------------------------

export interface SiteConnectionForm {
  name: string
  protocol: string
  /** Adapter-specific values keyed by the protocol's field keys (base_url/api_key/api_secret included). */
  config: SiteConnectionConfig
  callback_url: string
  retry_max: number | ''
  retry_intervals: string
  exchange_rate: number | ''
  price_markup_percent: number | ''
  price_rounding_mode: string
  auto_sync_price: 'true' | 'false'
}

export const emptySiteConnectionForm = (protocol = ''): SiteConnectionForm => ({
  name: '',
  protocol,
  config: {},
  callback_url: '',
  retry_max: 3,
  retry_intervals: '30,60,120',
  exchange_rate: 1,
  price_markup_percent: 0,
  price_rounding_mode: 'none',
  auto_sync_price: 'false',
})

/** retry_intervals may come back as an array, a JSON string ("[30,60]") or a plain string. */
export const retryIntervalsToText = (raw: unknown): string => {
  if (Array.isArray(raw)) return raw.join(',')
  if (typeof raw === 'string') {
    try {
      const parsed: unknown = JSON.parse(raw)
      if (Array.isArray(parsed)) return parsed.join(',')
    } catch {
      /* not JSON */
    }
    return raw
  }
  return '30,60,120'
}

/** Stringify config values from the backend (numbers/bools → string, objects dropped). */
export const normalizeConfig = (raw: unknown): SiteConnectionConfig => {
  const out: SiteConnectionConfig = {}
  if (!isRecord(raw)) return out
  for (const [k, v] of Object.entries(raw)) {
    if (typeof v === 'string') out[k] = v
    else if (typeof v === 'number' || typeof v === 'boolean') out[k] = String(v)
  }
  return out
}

export const siteConnectionToForm = (conn: AdminSiteConnection): SiteConnectionForm => ({
  name: conn.name || '',
  protocol: conn.protocol || 'dujiao-next',
  config: {
    ...normalizeConfig(conn.extra ?? conn.config),
    base_url: conn.base_url || '',
    api_key: conn.api_key || '',
    api_secret: conn.api_secret || '',
  },
  callback_url: conn.callback_url || '',
  retry_max: conn.retry_max ?? 3,
  retry_intervals: retryIntervalsToText(conn.retry_intervals),
  exchange_rate: conn.exchange_rate === undefined || conn.exchange_rate === null || conn.exchange_rate === '' ? 1 : Number(conn.exchange_rate),
  price_markup_percent: conn.price_markup_percent ?? 0,
  price_rounding_mode: conn.price_rounding_mode || 'none',
  auto_sync_price: conn.auto_sync_price ? 'true' : 'false',
})

/**
 * Config restricted to the selected protocol's fields (stale keys from a previously selected
 * protocol are dropped). Without a definition every value is kept.
 */
export const pickProtocolConfig = (config: SiteConnectionConfig, def?: Pick<SiteConnectionProtocolDef, 'fields'> | null): SiteConnectionConfig => {
  if (!def) return { ...config }
  const out: SiteConnectionConfig = {}
  for (const field of def.fields) out[field.key] = (config[field.key] ?? '').trim()
  return out
}

/** Connection columns; every other protocol field is adapter configuration (`extra`). */
const CORE_CONFIG_KEYS = ['base_url', 'api_key', 'api_secret']

/** Adapter-specific fields of a config (what the backend stores as `extra`). */
export const adapterExtra = (config: SiteConnectionConfig): SiteConnectionConfig =>
  Object.fromEntries(Object.entries(config).filter(([k]) => !CORE_CONFIG_KEYS.includes(k)))

/**
 * Build the create/update payload like the original (numbers as JSON numbers, intervals as a JSON
 * string) plus the adapter `config`; base_url/api_key/api_secret are also sent top-level and the
 * remaining adapter fields as `extra` (the backend field).
 */
export const buildSiteConnectionPayload = (form: SiteConnectionForm, def?: Pick<SiteConnectionProtocolDef, 'fields'> | null) => {
  const intervals = form.retry_intervals
    .split(',')
    .map((s) => Number(s.trim()))
    .filter((n) => !Number.isNaN(n) && n > 0)
  const config = pickProtocolConfig(form.config, def)
  return {
    name: form.name,
    base_url: config.base_url ?? '',
    api_key: config.api_key ?? '',
    api_secret: config.api_secret ?? '',
    protocol: form.protocol,
    config,
    extra: adapterExtra(config),
    callback_url: form.callback_url,
    retry_max: Number(form.retry_max) || 3,
    retry_intervals: JSON.stringify(intervals),
    exchange_rate: Number(form.exchange_rate) || 1,
    price_markup_percent: Number(form.price_markup_percent) || 0,
    price_rounding_mode: form.price_rounding_mode,
    auto_sync_price: form.auto_sync_price === 'true',
  }
}

export const hasMarkup = (conn: Pick<AdminSiteConnection, 'price_markup_percent'>) =>
  !!conn.price_markup_percent && Number(conn.price_markup_percent) !== 0

/** The original shows "reapply markup" when a markup or a non-1 exchange rate is configured. */
export const canReapplyMarkup = (conn: Pick<AdminSiteConnection, 'price_markup_percent' | 'exchange_rate'>) =>
  hasMarkup(conn) || (conn.exchange_rate !== undefined && conn.exchange_rate !== null && Number(conn.exchange_rate) !== 1)

export const siteConnectionStatusTone = (status?: string): BadgeTone =>
  status === 'active' ? 'success' : status === 'pending' ? 'warning' : 'neutral'

// ---------------------------------------------------------------------------
// Supplier adapters (protocol registry / connection code / handshake / capabilities)
// ---------------------------------------------------------------------------

/** A pasted connection code: a single token (the adapter decodes it server side). */
export const normalizeConnectionCode = (raw: string): string => raw.replace(/\s+/g, '')

/** Capability → i18n key under `siteConnections.features` (unknown ones are shown verbatim). */
const FEATURE_KEYS: Record<string, string> = {
  // wire handshake feature ids (protocol spec §4) and adapter-registry capability ids share labels
  changes: 'changes',
  incremental_changes: 'changes',
  webhooks: 'webhooks',
  push_events: 'webhooks',
  categories: 'categories',
  quote: 'quote',
  multi_item: 'multiItem',
  idempotency: 'idempotency',
  encrypted_delivery: 'encryptedDelivery',
}

export const featureLabelKey = (feature: string): string | null => {
  const key = FEATURE_KEYS[feature]
  return key ? `siteConnections.features.${key}` : null
}

/** Registered adapters get the brand tone; ids missing from the registry stay neutral. */
export const protocolTone = (registered: boolean): BadgeTone => (registered ? 'primary' : 'neutral')

export const syncModeTone = (mode?: string): BadgeTone => (mode === 'incremental' ? 'success' : 'neutral')

export const webhookStatusTone = (status?: string): BadgeTone => {
  switch (status) {
    case 'active':
    case 'registered':
    case 'ok':
      return 'success'
    case 'pending':
      return 'warning'
    case 'failed':
    case 'error':
      return 'danger'
    default:
      return 'neutral'
  }
}

/**
 * Generic definition used only while the protocol registry is unavailable (or for a connection whose
 * protocol is no longer registered) so the form still shows the classic URL/Key/Secret trio.
 */
export const fallbackProtocolDef = (id: string): SiteConnectionProtocolDef => ({
  id,
  name: { 'zh-CN': id, 'zh-TW': id, 'en-US': id },
  description: {},
  fields: [
    { key: 'base_url', label: { 'zh-CN': 'Base URL', 'zh-TW': 'Base URL', 'en-US': 'Base URL' }, kind: 'url', required: true, placeholder: { 'zh-CN': 'https://example.com' } },
    { key: 'api_key', label: { 'zh-CN': 'API Key', 'zh-TW': 'API Key', 'en-US': 'API Key' }, kind: 'text', required: true },
    { key: 'api_secret', label: { 'zh-CN': 'API Secret', 'zh-TW': 'API Secret', 'en-US': 'API Secret' }, kind: 'secret', required: true },
  ],
  capabilities: [],
  supports_connection_code: false,
})

/** Default protocol for a new connection: the first one offering a connection code, else the first. */
export const defaultProtocolId = (defs: readonly SiteConnectionProtocolDef[]): string =>
  (defs.find((d) => d.supports_connection_code) ?? defs[0])?.id ?? ''

/** Required fields of `def` still empty in `config` (in field order). */
export const missingRequiredFields = (def: Pick<SiteConnectionProtocolDef, 'fields'>, config: SiteConnectionConfig): SiteConnectionProtocolField[] =>
  def.fields.filter((f) => f.required && !(config[f.key] ?? '').trim())

export interface ConfigValidationMessages {
  required: string
  url: string
}

/**
 * Validate adapter fields. When editing, an empty secret means "keep the stored one" and is allowed.
 * Returns `{fieldKey: message}` for invalid fields only.
 */
export const validateProtocolConfig = (
  def: Pick<SiteConnectionProtocolDef, 'fields'>,
  config: SiteConnectionConfig,
  messages: ConfigValidationMessages,
  opts: { editing?: boolean } = {},
): Record<string, string> => {
  const errors: Record<string, string> = {}
  for (const field of def.fields) {
    const value = (config[field.key] ?? '').trim()
    if (!value) {
      if (field.required && !(opts.editing && field.kind === 'secret')) errors[field.key] = messages.required
      continue
    }
    if (field.kind === 'url' && !/^https?:\/\/[^\s/]+/i.test(value)) errors[field.key] = messages.url
    if (field.kind === 'select' && field.options && !field.options.some((o) => o.value === value)) errors[field.key] = messages.required
  }
  return errors
}

/** Normalize a suggested exchange rate ("7.100000" → 7.1); null when absent/invalid. */
export const parseSuggestedRate = (raw: string | null | undefined): number | null => {
  if (raw === null || raw === undefined || raw === '') return null
  const n = Number(raw)
  return Number.isFinite(n) && n > 0 ? n : null
}

// ---------------------------------------------------------------------------
// Product mappings / upstream import
// ---------------------------------------------------------------------------

export interface UpstreamSku {
  id: number
  sku_code?: string
  spec_values?: Record<string, unknown>
  price_amount: Money
  stock_status: string
  is_active: boolean
}

export interface UpstreamProduct {
  id: number
  title: Record<string, string>
  price_amount: Money
  currency?: string
  is_active: boolean
  category_id?: number
  skus?: UpstreamSku[]
}

export interface UpstreamCategory {
  id: number
  parent_id: number
  slug: string
  name: Record<string, string>
  icon: string
  sort_order: number
}

export interface SkuMapping {
  local_sku_id: number
  upstream_sku_id: number
  upstream_price: Money
  upstream_stock: number
  upstream_is_active: boolean
}

/** `GET upstream-products` returns either a bare array or `{items, total, mapped_ids}`. */
export const parseUpstreamResult = (data: unknown): { items: UpstreamProduct[]; total: number; mappedIds?: number[] } => {
  if (Array.isArray(data)) return { items: data as UpstreamProduct[], total: data.length }
  if (!isRecord(data)) return { items: [], total: 0 }
  const items = Array.isArray(data.items) ? (data.items as UpstreamProduct[]) : []
  const mapped = Array.isArray(data.mapped_ids) ? data.mapped_ids.map(Number).filter((n) => Number.isFinite(n)) : undefined
  return { items, total: Number(data.total) || 0, mappedIds: mapped }
}

export const isSkuAvailable = (status: string) => status === 'in_stock' || status === 'low_stock' || status === 'unlimited'

export type StockLevel = 'none' | 'all' | 'zero' | 'partial'

export const skuStockLevel = (product: Pick<UpstreamProduct, 'skus'>): { level: StockLevel; inStock: number; total: number } => {
  const skus = product.skus || []
  if (skus.length === 0) return { level: 'none', inStock: 0, total: 0 }
  const inStock = skus.filter((s) => isSkuAvailable(s.stock_status)).length
  const level: StockLevel = inStock === skus.length ? 'all' : inStock === 0 ? 'zero' : 'partial'
  return { level, inStock, total: skus.length }
}

export const upstreamPriceRange = (product: Pick<UpstreamProduct, 'skus' | 'price_amount'>) =>
  priceRange((product.skus || []).map((s) => s.price_amount), product.price_amount)

export const groupProductsByCategory = (products: UpstreamProduct[]) => {
  const map = new Map<number, UpstreamProduct[]>()
  for (const p of products) {
    const catId = p.category_id || 0
    const list = map.get(catId)
    if (list) list.push(p)
    else map.set(catId, [p])
  }
  return map
}

export interface CategoryDisplayItem {
  category: UpstreamCategory
  productCount: number
  path: string
  nonMappedCount: number
}

/** Upstream categories that hold at least one loaded product (+ an "uncategorized" bucket, id 0). */
export const buildCategoryDisplayList = (
  categories: UpstreamCategory[],
  byCategory: Map<number, UpstreamProduct[]>,
  mapped: Set<number>,
  labelOf: (name: Record<string, string>) => string,
  uncategorizedLabel: string,
): CategoryDisplayItem[] => {
  const catMap = new Map(categories.map((c) => [c.id, c]))
  const result: CategoryDisplayItem[] = []
  for (const cat of categories) {
    const products = byCategory.get(cat.id) || []
    if (products.length === 0) continue
    const parent = cat.parent_id > 0 ? catMap.get(cat.parent_id) : undefined
    const path = parent ? `${labelOf(parent.name)} / ${labelOf(cat.name)}` : labelOf(cat.name)
    result.push({ category: cat, productCount: products.length, path, nonMappedCount: products.filter((p) => !mapped.has(p.id)).length })
  }
  const uncategorized = byCategory.get(0) || []
  if (uncategorized.length > 0) {
    result.push({
      category: { id: 0, parent_id: 0, slug: '', name: {}, icon: '', sort_order: -1 },
      productCount: uncategorized.length,
      path: uncategorizedLabel,
      nonMappedCount: uncategorized.filter((p) => !mapped.has(p.id)).length,
    })
  }
  return result
}

/** Local selling price minus (upstream price × exchange rate); null when no mapping. */
export const skuPriceDiff = (localPrice: Money, upstreamPrice: Money, rate: number) => {
  const cost = parseMoneyValue(upstreamPrice) * rate
  const local = Number(localPrice)
  return { cost, diff: local - cost, equal: local === cost }
}

// ---------------------------------------------------------------------------
// Procurement
// ---------------------------------------------------------------------------

export interface ProcurementLocalOrderItem {
  title: Record<string, string>
  sku_snapshot?: { sku_code?: string }
  quantity: number | string
  cost_price?: number | string
  total_amount: number | string
}

export interface ProcurementLocalOrder {
  status: string
  user_email?: string
  refunded_amount?: number | string
  items?: ProcurementLocalOrderItem[]
}

export interface ProcurementFinanceInput {
  local_sell_amount?: Money
  upstream_refunded_amount?: Money
  connection?: { exchange_rate?: Money }
  local_order?: ProcurementLocalOrder
}

export const procurementExchangeRate = (order: ProcurementFinanceInput) => {
  const rate = parseMoneyValue(order.connection?.exchange_rate)
  return rate > 0 ? rate : 1
}

export const localCostValue = (order: ProcurementFinanceInput) =>
  (order.local_order?.items || []).reduce((sum, item) => sum + parseMoneyValue(item.cost_price) * parseMoneyValue(item.quantity), 0)

export const localCostAmount = (order: ProcurementFinanceInput): string | null => {
  const cost = localCostValue(order)
  return Math.abs(cost) <= 0.000001 ? null : cost.toFixed(2)
}

/** profit = sell + upstreamRefund×rate − localCost − localRefund; null when all inputs are zero. */
export const procurementProfit = (order: ProcurementFinanceInput): string | null => {
  const sell = parseMoneyValue(order.local_sell_amount)
  const upstreamRefund = parseMoneyValue(order.upstream_refunded_amount) * procurementExchangeRate(order)
  const localCost = localCostValue(order)
  const localRefund = parseMoneyValue(order.local_order?.refunded_amount)
  if (![sell, upstreamRefund, localCost, localRefund].some((v) => Math.abs(v) > 0.000001)) return null
  return (sell + upstreamRefund - localCost - localRefund).toFixed(2)
}

export interface UpstreamRefundRecordRow {
  id: string
  type: string
  amount: string
  currency: string
  remark: string
  createdAt: string
}

const text = (v: unknown) => (v === null || v === undefined ? '' : String(v).trim())

export const normalizeUpstreamRefundRecords = (records: unknown): UpstreamRefundRecordRow[] => {
  if (!Array.isArray(records)) return []
  return records.map((raw, index) => {
    const r = isRecord(raw) ? raw : {}
    return { id: String(index + 1), type: text(r.type), amount: text(r.amount), currency: text(r.currency), remark: text(r.remark), createdAt: text(r.created_at) }
  })
}

export interface ProcurementStats {
  total: number
  pending: number
  failed: number
  rejected: number
  fulfilled: number
  other: number
}

export const parseProcurementStats = (data: unknown): ProcurementStats => {
  const d = isRecord(data) ? data : {}
  const by = isRecord(d.by_status) ? d.by_status : {}
  const total = Number(d.total ?? 0) || 0
  const pending = Number(by.pending ?? 0) || 0
  const failed = Number(by.failed ?? 0) || 0
  const rejected = Number(by.rejected ?? 0) || 0
  const fulfilled = Number(by.fulfilled ?? 0) || 0
  return { total, pending, failed, rejected, fulfilled, other: Math.max(0, total - pending - failed - rejected - fulfilled) }
}

/** `manual_review` (Zebra Store): the supplier may have charged us; never rolled back automatically. */
export const PROCUREMENT_STATUSES = ['pending', 'accepted', 'manual_review', 'rejected', 'failed', 'partially_refunded', 'fulfilled', 'refunded', 'canceled'] as const

export const procurementStatusTone = (status?: string): BadgeTone => {
  switch (status) {
    case 'pending':
    case 'partially_refunded':
    case 'manual_review':
      return 'warning'
    case 'submitted':
    case 'accepted':
    case 'refunded':
      return 'info'
    case 'rejected':
    case 'failed':
      return 'danger'
    case 'fulfilled':
    case 'completed':
      return 'success'
    default:
      return 'neutral'
  }
}

/** Manual review: retry re-checks / resubmits with the same request number. */
export const canRetryProcurement = (status: string) => ['failed', 'rejected', 'manual_review'].includes(status)
/** Manual review: cancel marks it failed and rolls the local order back (refund). */
export const canCancelProcurement = (status: string) => ['pending', 'submitted', 'accepted', 'failed', 'manual_review'].includes(status)

/** Relative-time bucket: [key suffix, n]. */
export const relativeTimeParts = (raw: string | undefined, now = Date.now()): { key: 'justNow' | 'minutesAgo' | 'hoursAgo' | 'daysAgo'; n: number } | null => {
  if (!raw) return null
  const d = new Date(raw)
  if (Number.isNaN(d.getTime())) return null
  const minutes = Math.floor((now - d.getTime()) / 60000)
  if (minutes < 1) return { key: 'justNow', n: 0 }
  if (minutes < 60) return { key: 'minutesAgo', n: minutes }
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return { key: 'hoursAgo', n: hours }
  return { key: 'daysAgo', n: Math.floor(hours / 24) }
}

// ---------------------------------------------------------------------------
// Reconciliation / API credentials
// ---------------------------------------------------------------------------

export const reconciliationStatusTone = (status?: string): BadgeTone => {
  switch (status) {
    case 'pending':
      return 'warning'
    case 'running':
      return 'info'
    case 'completed':
      return 'success'
    case 'failed':
      return 'danger'
    default:
      return 'neutral'
  }
}

export const mismatchTone = (type?: string): BadgeTone => {
  switch (type) {
    case 'status':
      return 'warning'
    case 'amount':
      return 'secondary'
    case 'both':
      return 'danger'
    default:
      return 'neutral'
  }
}

export const apiCredentialStatusTone = (status?: string): BadgeTone => {
  switch (status) {
    case 'approved':
      return 'success'
    case 'pending_review':
      return 'warning'
    case 'rejected':
      return 'danger'
    default:
      return 'neutral'
  }
}

/** Show an absolute time or "-" for empty/invalid values (original `formatTime`). */
export const formatTime = (raw?: string | null) => {
  if (!raw) return '-'
  const d = new Date(raw)
  return Number.isNaN(d.getTime()) ? '-' : d.toLocaleString()
}
