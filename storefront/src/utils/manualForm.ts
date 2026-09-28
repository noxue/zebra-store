import type { LocalizedText, ManualFormSchema } from '@/api/types'
import { localizedText } from './localized'

export interface NormalizedManualField {
  key: string
  type: string
  required: boolean
  label?: LocalizedText
  placeholder?: LocalizedText
  regex?: string
  min?: number
  max?: number
  max_len?: number
  options: string[]
  /** Supplier message for a refused pattern (acg-faka widget `error`, LQA-I2). */
  errorMessage?: LocalizedText
}

export interface ManualFormProduct {
  /** String(productId) – key of `manual_form_data` sent to the backend. */
  itemKey: string
  productId: number
  title: LocalizedText
  fields: NormalizedManualField[]
  skuCount: number
}

export type ManualFieldValue = string | string[]
export type ManualFormValues = Record<string, Record<string, ManualFieldValue>>

export interface ManualFormLine {
  productId: number
  title: LocalizedText
  fulfillmentType?: string
  manualFormSchema?: ManualFormSchema | null
}

const MANUAL_FIELD_TYPES = new Set(['text', 'textarea', 'phone', 'email', 'number', 'select', 'radio', 'checkbox'])
const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/
const PHONE_PATTERN = /^\+?[0-9\-()\s]{6,20}$/

const findLastUnescapedSlash = (value: string) => {
  for (let index = value.length - 1; index > 0; index -= 1) {
    if (value[index] !== '/') continue
    let backslashes = 0
    for (let cursor = index - 1; cursor >= 0 && value[cursor] === '\\'; cursor -= 1) backslashes += 1
    if (backslashes % 2 === 0) return index
  }
  return -1
}

/** Accepts `/pattern/flags` or a bare pattern; null when invalid. */
export const compileManualRegex = (rawRegex?: string): RegExp | null => {
  const text = String(rawRegex || '').trim()
  if (!text) return null
  if (text.startsWith('/')) {
    const last = findLastUnescapedSlash(text)
    if (last > 0) {
      const pattern = text.slice(1, last)
      const flags = text.slice(last + 1)
      if (/^[gimsuy]*$/.test(flags)) {
        try {
          return new RegExp(pattern, flags)
        } catch {
          return null
        }
      }
    }
  }
  try {
    return new RegExp(text)
  } catch {
    return null
  }
}

const optionValue = (option: unknown): string => {
  if (option && typeof option === 'object' && 'value' in option) return String((option as { value: unknown }).value ?? '').trim()
  return String(option ?? '').trim()
}

const finiteOrUndefined = (value: unknown) => {
  if (value === null || value === undefined || value === '') return undefined
  const n = Number(value)
  return Number.isFinite(n) ? n : undefined
}

export const normalizeManualFormSchema = (schema: ManualFormSchema | null | undefined): NormalizedManualField[] => {
  const raw = Array.isArray(schema?.fields) ? schema.fields : []
  const out: NormalizedManualField[] = []
  for (const field of raw) {
    const key = String(field?.key || '').trim()
    const type = String(field?.type || '').trim()
    if (!key || !MANUAL_FIELD_TYPES.has(type)) continue
    const options = Array.isArray(field.options) ? field.options.map(optionValue).filter(Boolean) : []
    out.push({
      key,
      type,
      required: Boolean(field.required),
      label: field.label || undefined,
      placeholder: field.placeholder || undefined,
      regex: String(field.regex || '').trim() || undefined,
      min: finiteOrUndefined(field.min),
      max: finiteOrUndefined(field.max),
      max_len: finiteOrUndefined(field.max_len),
      options: Array.from(new Set(options)),
      errorMessage: field.error_message || undefined,
    })
  }
  return out
}

/**
 * Groups lines with a form schema by product (one form per product). The schema alone
 * decides: upstream products that ask the buyer for input are displayed as `auto`
 * (LQA-I2), and the API never sends a schema for a local auto product.
 */
export const buildManualFormProducts = (lines: ManualFormLine[]): ManualFormProduct[] => {
  const grouped = new Map<number, ManualFormProduct>()
  for (const line of lines) {
    const fields = normalizeManualFormSchema(line.manualFormSchema)
    if (fields.length === 0) continue
    const productId = Math.trunc(Number(line.productId))
    if (!Number.isFinite(productId) || productId <= 0) continue
    const existing = grouped.get(productId)
    if (existing) {
      existing.skuCount += 1
      continue
    }
    grouped.set(productId, { itemKey: String(productId), productId, title: line.title, fields, skuCount: 1 })
  }
  return Array.from(grouped.values())
}

/** Re-shapes current values to the fields of `products` (drops stale keys). */
export const syncManualFormValues = (products: ManualFormProduct[], current: ManualFormValues): ManualFormValues => {
  const next: ManualFormValues = {}
  for (const product of products) {
    const values = current[product.itemKey] || {}
    const row: Record<string, ManualFieldValue> = {}
    for (const field of product.fields) {
      const value = values[field.key]
      if (field.type === 'checkbox') row[field.key] = Array.isArray(value) ? value.map(String).filter(Boolean) : []
      else row[field.key] = value == null || Array.isArray(value) ? '' : String(value)
    }
    next[product.itemKey] = row
  }
  return next
}

export const manualFieldErrorKey = (itemKey: string, fieldKey: string) => `${itemKey}:${fieldKey}`

export type ManualTranslate = (key: string, params: Record<string, unknown>) => string

export interface ManualFormValidation {
  valid: boolean
  errors: Record<string, string>
  firstError: string
}

export const manualFieldLabel = (field: NormalizedManualField, locale: string) => localizedText(field.label, locale) || field.key

const toList = (value: ManualFieldValue | undefined) => (Array.isArray(value) ? value.map((v) => String(v).trim()).filter(Boolean) : [])

/** Same rules and message keys as the original checkout. */
export const validateManualForm = (
  products: ManualFormProduct[],
  data: ManualFormValues,
  t: ManualTranslate,
  locale: string,
): ManualFormValidation => {
  const errors: Record<string, string> = {}
  let firstError = ''
  const setError = (itemKey: string, field: NormalizedManualField, message: string) => {
    const key = manualFieldErrorKey(itemKey, field.key)
    if (errors[key]) return
    errors[key] = message
    if (!firstError) firstError = message
  }
  for (const product of products) {
    const values = data[product.itemKey] || {}
    for (const field of product.fields) {
      const name = manualFieldLabel(field, locale)
      const raw = values[field.key]
      if (field.type === 'checkbox') {
        const list = toList(raw)
        if (field.required && list.length === 0) {
          setError(product.itemKey, field, t('checkout.manualFormFieldRequired', { name }))
          continue
        }
        if (list.length > 0 && field.options.length > 0 && list.some((item) => !field.options.includes(item))) {
          setError(product.itemKey, field, t('checkout.manualFormFieldOptionInvalid', { name }))
        }
        continue
      }
      const text = raw == null || Array.isArray(raw) ? '' : String(raw).trim()
      if (field.required && !text) {
        setError(product.itemKey, field, t('checkout.manualFormFieldRequired', { name }))
        continue
      }
      if (!text) continue
      if (['text', 'textarea', 'phone', 'email'].includes(field.type) && field.max_len && text.length > field.max_len) {
        setError(product.itemKey, field, t('checkout.manualFormFieldMaxLength', { name, max: field.max_len }))
        continue
      }
      if ((field.type === 'phone' && !PHONE_PATTERN.test(text)) || (field.type === 'email' && !EMAIL_PATTERN.test(text))) {
        setError(product.itemKey, field, t('checkout.manualFormFieldInvalid', { name }))
        continue
      }
      if (field.type === 'number') {
        const n = Number(text)
        if (!Number.isFinite(n)) {
          setError(product.itemKey, field, t('checkout.manualFormFieldNumberInvalid', { name }))
          continue
        }
        if ((field.min !== undefined && n < field.min) || (field.max !== undefined && n > field.max)) {
          setError(product.itemKey, field, t('checkout.manualFormFieldNumberRange', { name }))
          continue
        }
      }
      if ((field.type === 'select' || field.type === 'radio') && field.options.length > 0 && !field.options.includes(text)) {
        setError(product.itemKey, field, t('checkout.manualFormFieldOptionInvalid', { name }))
        continue
      }
      if (field.regex) {
        const regex = compileManualRegex(field.regex)
        if (!regex || !regex.test(text)) {
          const custom = localizedText(field.errorMessage, locale)
          setError(product.itemKey, field, custom || t('checkout.manualFormFieldInvalid', { name }))
        }
      }
    }
  }
  return { valid: Object.keys(errors).length === 0, errors, firstError }
}

/** `manual_form_data` payload: `{ [productId]: { field: value } }`, empty values omitted. */
export const buildManualFormDataPayload = (products: ManualFormProduct[], data: ManualFormValues): Record<string, Record<string, unknown>> => {
  const payload: Record<string, Record<string, unknown>> = {}
  for (const product of products) {
    const values = data[product.itemKey] || {}
    const row: Record<string, unknown> = {}
    for (const field of product.fields) {
      const raw = values[field.key]
      if (field.type === 'checkbox') {
        const list = toList(raw)
        if (list.length > 0) row[field.key] = list
        continue
      }
      const text = raw == null || Array.isArray(raw) ? '' : String(raw).trim()
      if (text) row[field.key] = text
    }
    payload[product.itemKey] = row
  }
  return payload
}

export const isValidEmail = (value: string) => EMAIL_PATTERN.test(value.trim())
