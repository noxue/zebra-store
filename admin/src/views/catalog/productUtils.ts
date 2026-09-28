// Pure helpers for 商品管理 (list + edit modal).
import type { AdminProduct, AdminProductSKU } from '@/api/types'

export const PRODUCT_LOCALES = ['zh-CN', 'zh-TW', 'en-US'] as const
export type ProductLocale = (typeof PRODUCT_LOCALES)[number]
export type LocaleForm = Record<ProductLocale, string>

export const emptyLocaleText = (): LocaleForm => ({ 'zh-CN': '', 'zh-TW': '', 'en-US': '' })

export const toSafeInt = (value: unknown) => {
  const num = Number(value)
  if (!Number.isFinite(num)) return 0
  return Math.max(Math.floor(num), 0)
}

/** Stock total where -1 means "unlimited"; anything else is clamped to >= 0. */
export const toSafeStockTotal = (value: unknown) => {
  const num = Number(value)
  if (!Number.isFinite(num)) return 0
  const integer = Math.floor(num)
  if (integer === -1) return -1
  return Math.max(integer, 0)
}

type StockSource = Pick<AdminProduct, 'manual_stock_total' | 'manual_stock_locked' | 'manual_stock_sold'> & {
  skus?: Array<Pick<AdminProductSKU, 'is_active' | 'manual_stock_total' | 'manual_stock_locked' | 'manual_stock_sold'>>
}

/** Manual-fulfilment stock totals, aggregated over active SKUs when present. */
export const resolveManualStockMetrics = (product: StockSource) => {
  const skuRows = Array.isArray(product?.skus) ? product.skus : []
  const activeRows = skuRows.filter((item) => Boolean(item?.is_active))
  if (!activeRows.length) {
    return {
      total: toSafeStockTotal(product?.manual_stock_total),
      locked: toSafeInt(product?.manual_stock_locked),
      sold: toSafeInt(product?.manual_stock_sold),
    }
  }
  const locked = activeRows.reduce((sum, item) => sum + toSafeInt(item?.manual_stock_locked), 0)
  const sold = activeRows.reduce((sum, item) => sum + toSafeInt(item?.manual_stock_sold), 0)
  if (activeRows.some((item) => toSafeStockTotal(item?.manual_stock_total) === -1)) {
    return { total: -1, locked, sold }
  }
  const total = activeRows.reduce((sum, item) => sum + toSafeStockTotal(item?.manual_stock_total), 0)
  return { total, locked, sold }
}

// ---- list page: ?wholesale= / is_active query mapping ----

export type TriState = 'all' | 'enabled' | 'disabled'

export const parseWholesaleQuery = (value: unknown): TriState => {
  const raw = Array.isArray(value) ? value[0] : value
  const normalized = String(raw ?? '')
    .trim()
    .toLowerCase()
  if (['1', 'true', 'yes', 'enabled'].includes(normalized)) return 'enabled'
  if (['0', 'false', 'no', 'disabled'].includes(normalized)) return 'disabled'
  return 'all'
}

export const wholesaleQueryValue = (status: string) => (status === 'enabled' ? '1' : status === 'disabled' ? '0' : undefined)

/** 上架状态三态：'all' 不下发参数，'active' → '1'，'inactive' → '0' */
export const statusQueryValue = (status: string) => (status === 'active' ? '1' : status === 'inactive' ? '0' : undefined)

// ---- edit modal ----

export interface SKUFormItem {
  id: number
  sku_code: string
  spec_values: Record<string, string>
  price_amount: number | ''
  cost_price_amount: number | ''
  manual_stock_total: number | ''
  is_active: boolean
  sort_order: number | ''
}

export interface ManualFormField {
  key: string
  type: string
  required: boolean
  label: LocaleForm
  placeholder: LocaleForm
  regex: string
  min: string
  max: string
  max_len: string
  options_text: string
}

export const MANUAL_FIELD_TYPES = ['text', 'textarea', 'phone', 'email', 'number', 'select', 'radio', 'checkbox'] as const
export const STOCK_DISPLAY_MODES = ['exact', 'status', 'range', 'hidden'] as const

export const isTextLikeField = (type: string) => type === 'text' || type === 'textarea' || type === 'phone' || type === 'email'
export const isOptionField = (type: string) => type === 'select' || type === 'radio' || type === 'checkbox'

export const createSKUFormItem = (raw?: Partial<Omit<AdminProductSKU, 'price_amount' | 'cost_price_amount'>> & { price_amount?: unknown; cost_price_amount?: unknown }): SKUFormItem => ({
  id: Number(raw?.id || 0),
  sku_code: String(raw?.sku_code || '').trim(),
  spec_values: {
    ...emptyLocaleText(),
    ...Object.keys(raw?.spec_values || {}).reduce((result: Record<string, string>, key) => {
      const text = String(raw?.spec_values?.[key] ?? '').trim()
      if (text) result[key] = text
      return result
    }, {}),
  },
  price_amount: Number(raw?.price_amount || 0),
  cost_price_amount: Number(raw?.cost_price_amount || 0),
  manual_stock_total: toSafeStockTotal(raw?.manual_stock_total),
  is_active: raw?.is_active ?? true,
  sort_order: Number(raw?.sort_order || 0),
})

export const createManualFormField = (): ManualFormField => ({
  key: '',
  type: 'text',
  required: false,
  label: emptyLocaleText(),
  placeholder: emptyLocaleText(),
  regex: '',
  min: '',
  max: '',
  max_len: '',
  options_text: '',
})

export const parsePaymentChannelIDs = (raw: unknown): number[] => {
  if (!raw) return []
  if (Array.isArray(raw)) return raw.filter((id): id is number => typeof id === 'number' && Number.isFinite(id) && id > 0)
  if (typeof raw === 'string') {
    const trimmed = raw.trim()
    if (!trimmed || trimmed === '[]') return []
    try {
      const parsed: unknown = JSON.parse(trimmed)
      if (Array.isArray(parsed)) return parsed.filter((id): id is number => typeof id === 'number' && id > 0)
    } catch {
      /* ignore */
    }
  }
  return []
}

const normalizeLocaleValue = (val: unknown): LocaleForm => {
  if (!val) return emptyLocaleText()
  if (typeof val === 'string') return { 'zh-CN': val, 'zh-TW': '', 'en-US': '' }
  if (typeof val === 'object') {
    const obj = val as Record<string, unknown>
    return { 'zh-CN': String(obj['zh-CN'] || ''), 'zh-TW': String(obj['zh-TW'] || ''), 'en-US': String(obj['en-US'] || '') }
  }
  return emptyLocaleText()
}

export const normalizeSeoMeta = (raw: unknown) => {
  const obj = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : null
  if (!obj) return { keywords: emptyLocaleText(), description: emptyLocaleText() }
  return { keywords: normalizeLocaleValue(obj.keywords), description: normalizeLocaleValue(obj.description) }
}

/** Full 3-locale object for editing, keeping the raw values (not trimmed). */
export const toLocaleForm = (value: unknown): LocaleForm => {
  const out = emptyLocaleText()
  if (value && typeof value === 'object') {
    const src = value as Record<string, unknown>
    for (const code of PRODUCT_LOCALES) out[code] = typeof src[code] === 'string' ? (src[code] as string) : ''
  }
  return out
}

const normalizeLocaleText = (value: unknown) => {
  const result: Record<string, string> = {}
  const obj = (value && typeof value === 'object' ? value : {}) as Record<string, unknown>
  for (const code of PRODUCT_LOCALES) {
    const text = String(obj[code] || '').trim()
    if (text) result[code] = text
  }
  return result
}

export const normalizeSpecValues = (value: unknown) => {
  const result: Record<string, string> = {}
  if (!value || typeof value !== 'object') return result
  const obj = value as Record<string, unknown>
  Object.keys(obj).forEach((key) => {
    const text = String(obj[key] ?? '').trim()
    if (text) result[key] = text
  })
  return result
}

const parseManualOptions = (value: unknown) =>
  Array.isArray(value) ? value.map((item) => String(item || '').trim()).filter(Boolean) : []

export const parseManualFormSchemaForEdit = (rawSchema: unknown): { fields: ManualFormField[] } => {
  const schema = rawSchema && typeof rawSchema === 'object' ? (rawSchema as Record<string, unknown>) : null
  const rawFields = Array.isArray(schema?.fields) ? (schema.fields as unknown[]) : []
  const fields = rawFields.map((item): ManualFormField => {
    const rawField = (item && typeof item === 'object' ? item : {}) as Record<string, unknown>
    const type = String(rawField.type || 'text').trim() || 'text'
    return {
      key: String(rawField.key || '').trim(),
      type,
      required: Boolean(rawField.required),
      label: { ...emptyLocaleText(), ...((rawField.label || {}) as Record<string, string>) },
      placeholder: { ...emptyLocaleText(), ...((rawField.placeholder || {}) as Record<string, string>) },
      regex: String(rawField.regex || '').trim(),
      min: rawField.min == null ? '' : String(rawField.min),
      max: rawField.max == null ? '' : String(rawField.max),
      max_len: rawField.max_len == null ? '' : String(rawField.max_len),
      options_text: parseManualOptions(rawField.options).join('\n'),
    }
  })
  return { fields }
}

export const normalizeManualFormSchemaForSubmit = (fulfillmentType: string, fields: ManualFormField[]) => {
  if (fulfillmentType !== 'manual') return { fields: [] as Record<string, unknown>[] }
  const out: Record<string, unknown>[] = []
  for (const field of fields || []) {
    const key = String(field?.key || '').trim()
    const type = String(field?.type || 'text').trim() || 'text'
    if (!key) continue
    const normalized: Record<string, unknown> = { key, type, required: Boolean(field?.required) }
    const label = normalizeLocaleText(field?.label)
    if (Object.keys(label).length) normalized.label = label
    const placeholder = normalizeLocaleText(field?.placeholder)
    if (Object.keys(placeholder).length) normalized.placeholder = placeholder
    const regex = String(field?.regex || '').trim()
    if (regex) normalized.regex = regex
    const min = Number(field?.min)
    if (Number.isFinite(min) && field?.min !== '') normalized.min = min
    const max = Number(field?.max)
    if (Number.isFinite(max) && field?.max !== '') normalized.max = max
    const maxLen = Number(field?.max_len)
    if (Number.isInteger(maxLen) && maxLen > 0) normalized.max_len = maxLen
    if (isOptionField(type)) {
      const options = String(field?.options_text || '')
        .split(/\n|,/)
        .map((item) => item.trim())
        .filter(Boolean)
      if (options.length) normalized.options = Array.from(new Set(options))
    }
    out.push(normalized)
  }
  return { fields: out }
}

export interface NormalizedSKU {
  id?: number
  sku_code: string
  spec_values: Record<string, string>
  price_amount: number
  cost_price_amount: number
  manual_stock_total: number
  is_active: boolean
  sort_order: number
}

/** Translator used by validation helpers (keeps these functions pure/testable). */
export type Translate = (key: string, params?: Record<string, unknown>) => string

/** Validate + normalise the SKU editor rows; throws Error(message) on invalid input. */
export const normalizeSKUsForSubmit = (skus: SKUFormItem[], fulfillmentType: string, t: Translate): NormalizedSKU[] => {
  if (!skus.length) return []
  const seenCode = new Set<string>()
  const normalized = skus.map((item, index): NormalizedSKU => {
    const skuCode = String(item.sku_code || '').trim()
    if (!skuCode) throw new Error(t('admin.products.errors.skuCodeRequired', { index: index + 1 }))
    const codeKey = skuCode.toLowerCase()
    if (seenCode.has(codeKey)) throw new Error(t('admin.products.errors.skuCodeDuplicate', { code: skuCode }))
    seenCode.add(codeKey)
    const priceAmount = Number(item.price_amount)
    if (item.price_amount === '' || !Number.isFinite(priceAmount) || priceAmount <= 0) {
      throw new Error(t('admin.products.errors.skuPriceInvalid', { index: index + 1 }))
    }
    return {
      id: item.id > 0 ? item.id : undefined,
      sku_code: skuCode,
      spec_values: normalizeSpecValues(item.spec_values),
      price_amount: priceAmount,
      cost_price_amount: Number(item.cost_price_amount) || 0,
      manual_stock_total: fulfillmentType === 'manual' ? toSafeStockTotal(item.manual_stock_total) : 0,
      is_active: Boolean(item.is_active),
      sort_order: Number(item.sort_order) || 0,
    }
  })
  if (!normalized.some((item) => item.is_active)) throw new Error(t('admin.products.errors.skuNeedActive'))
  return normalized
}

export interface ProductForm {
  id: number
  title: LocaleForm
  slug: string
  seo_meta: { keywords: LocaleForm; description: LocaleForm }
  description: LocaleForm
  content: LocaleForm
  instructions: LocaleForm
  price_amount: number | ''
  cost_price_amount: number | ''
  images: string[]
  tags: string[]
  purchase_type: string
  min_purchase_quantity: number | ''
  max_purchase_quantity: number | ''
  stock_display_mode: string
  fulfillment_type: string
  manual_stock_total: number | ''
  skus: SKUFormItem[]
  category_id: number | null
  payment_channel_ids: number[]
  is_affiliate_enabled: boolean
  is_active: boolean
  sort_order: number | ''
  manual_form_schema: { fields: ManualFormField[] }
}

export const emptyProductForm = (): ProductForm => ({
  id: 0,
  title: emptyLocaleText(),
  slug: '',
  seo_meta: { keywords: emptyLocaleText(), description: emptyLocaleText() },
  description: emptyLocaleText(),
  content: emptyLocaleText(),
  instructions: emptyLocaleText(),
  price_amount: 0,
  cost_price_amount: 0,
  images: [],
  tags: [],
  purchase_type: 'member',
  min_purchase_quantity: '',
  max_purchase_quantity: '',
  stock_display_mode: 'exact',
  fulfillment_type: 'manual',
  manual_stock_total: 0,
  skus: [],
  category_id: null,
  payment_channel_ids: [],
  is_affiliate_enabled: false,
  is_active: true,
  sort_order: 0,
  manual_form_schema: { fields: [] },
})

const extractStringList = (raw: unknown, nestedKey: string): string[] => {
  if (Array.isArray(raw)) return raw.map((v) => String(v))
  if (raw && typeof raw === 'object') {
    const nested = (raw as Record<string, unknown>)[nestedKey]
    if (Array.isArray(nested)) return nested.map((v) => String(v))
  }
  return []
}

const positiveIntOrEmpty = (value: unknown): number | '' => (Number(value || 0) > 0 ? Math.floor(Number(value || 0)) : '')

/** Map an API product into the edit form (populateForm in the original). */
export const productToForm = (product: AdminProduct): ProductForm => ({
  id: product.id,
  title: toLocaleForm(product.title),
  slug: product.slug,
  seo_meta: normalizeSeoMeta(product.seo_meta),
  description: toLocaleForm(product.description),
  content: toLocaleForm(product.content),
  instructions: toLocaleForm(product.instructions),
  price_amount: Number(product.price_amount || 0),
  cost_price_amount: Number(product.cost_price_amount || 0),
  images: extractStringList(product.images, 'images'),
  tags: extractStringList(product.tags, 'tags'),
  purchase_type: product.purchase_type || 'member',
  min_purchase_quantity: positiveIntOrEmpty(product.min_purchase_quantity),
  max_purchase_quantity: positiveIntOrEmpty(product.max_purchase_quantity),
  stock_display_mode: product.stock_display_mode || 'exact',
  fulfillment_type: product.fulfillment_type || 'manual',
  manual_stock_total: resolveManualStockMetrics(product).total,
  skus: Array.isArray(product.skus) ? product.skus.map((item) => createSKUFormItem(item)) : [],
  category_id: Number(product.category_id || 0) || null,
  payment_channel_ids: parsePaymentChannelIDs(product.payment_channel_ids),
  is_affiliate_enabled: Boolean(product.is_affiliate_enabled),
  is_active: product.is_active ?? true,
  sort_order: Number(product.sort_order || 0),
  manual_form_schema: parseManualFormSchemaForEdit(product.manual_form_schema),
})

export interface ProductPayloadMessages {
  categoryRequired: string
  categoryLeaf: string
}

/**
 * Validate the form and build the create/update payload (identical field set to the original;
 * money values are JSON numbers exactly like the original). Throws Error(message) on invalid input.
 */
export const buildProductPayload = (
  form: ProductForm,
  t: Translate,
  opts: { isCategorySelectable: (id: number) => boolean; messages: ProductPayloadMessages },
) => {
  const normalizedCategoryID = Number(form.category_id)
  if (!Number.isFinite(normalizedCategoryID) || normalizedCategoryID <= 0) throw new Error(opts.messages.categoryRequired)
  if (!opts.isCategorySelectable(normalizedCategoryID)) throw new Error(opts.messages.categoryLeaf)

  const normalizedSKUs = normalizeSKUsForSubmit(form.skus, form.fulfillment_type, t)
  const activeSKU = normalizedSKUs.find((item) => item.is_active)
  let effectivePrice = Number(form.price_amount) || 0
  let effectiveCostPrice = Number(form.cost_price_amount) || 0
  if (normalizedSKUs.length > 0) {
    const priceSource = activeSKU || normalizedSKUs[0]!
    effectivePrice = Number(priceSource.price_amount)
    effectiveCostPrice = Number(priceSource.cost_price_amount || 0)
  }
  const minQ = Number(form.min_purchase_quantity)
  const maxQ = Number(form.max_purchase_quantity)
  const minPurchaseQuantity = form.min_purchase_quantity !== '' && Number.isFinite(minQ) && minQ > 0 ? Math.floor(minQ) : 0
  const maxPurchaseQuantity = form.max_purchase_quantity !== '' && Number.isFinite(maxQ) && maxQ > 0 ? Math.floor(maxQ) : 0
  if (minPurchaseQuantity > 0 && maxPurchaseQuantity > 0 && minPurchaseQuantity > maxPurchaseQuantity) {
    throw new Error(t('admin.products.form.purchaseLimitInvalid'))
  }
  const effectiveManualStockTotal = normalizedSKUs.length
    ? (() => {
        const activeRows = normalizedSKUs.filter((item) => item.is_active)
        if (activeRows.some((item) => toSafeStockTotal(item.manual_stock_total) === -1)) return -1
        return activeRows.reduce((sum, item) => sum + toSafeStockTotal(item.manual_stock_total), 0)
      })()
    : toSafeStockTotal(form.manual_stock_total)

  return {
    slug: String(form.slug || '').trim(),
    category_id: Math.floor(normalizedCategoryID),
    seo_meta: { keywords: { ...form.seo_meta.keywords }, description: { ...form.seo_meta.description } },
    title: { ...form.title },
    description: { ...form.description },
    content: { ...form.content },
    instructions: { ...form.instructions },
    price_amount: effectivePrice,
    cost_price_amount: effectiveCostPrice,
    images: [...form.images],
    tags: [...form.tags],
    purchase_type: form.purchase_type,
    min_purchase_quantity: minPurchaseQuantity,
    max_purchase_quantity: maxPurchaseQuantity,
    stock_display_mode: form.stock_display_mode,
    fulfillment_type: form.fulfillment_type,
    manual_stock_total: effectiveManualStockTotal,
    skus: normalizedSKUs,
    payment_channel_ids: form.payment_channel_ids.length > 0 ? [...form.payment_channel_ids] : [],
    is_affiliate_enabled: form.is_affiliate_enabled,
    is_active: form.is_active,
    sort_order: Number(form.sort_order) || 0,
    manual_form_schema: normalizeManualFormSchemaForSubmit(form.fulfillment_type, form.manual_form_schema.fields),
  }
}

export type ProductPayload = ReturnType<typeof buildProductPayload>
