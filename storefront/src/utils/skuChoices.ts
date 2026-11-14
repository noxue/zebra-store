import type { ProductSku } from '@/api/types'
import { localizedText } from './localized'

const LOCALES = ['zh-CN', 'zh-TW', 'en-US'] as const
const MAX_DIMENSIONS = 8
const record = (value: unknown): value is Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value)
const localized = (value: unknown): value is Record<string, string> => record(value) && Object.keys(value).length > 0 && Object.entries(value).every(([key, text]) => LOCALES.includes(key as typeof LOCALES[number]) && typeof text === 'string')
interface ChoiceValue { key: string; label: string }
export interface SkuChoiceRow { sku: ProductSku; values: string[] }
export interface SkuChoices { dimensions: string[]; values: ChoiceValue[][]; rows: SkuChoiceRow[] }

function choiceValue(value: unknown, locale: string): ChoiceValue | null {
  if (typeof value === 'string' && value.trim()) return { key: JSON.stringify(value), label: value.trim() }
  if (typeof value === 'number' && Number.isFinite(value)) return { key: JSON.stringify(value), label: String(value) }
  if (!localized(value)) return null
  const label = localizedText(value, locale)
  if (!label) return null
  // PRV-08: a language object is one value; its identity never depends on locale.
  return { key: JSON.stringify(Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)), label }
}

/** Uses actual, unambiguous SKU combinations. Legacy shapes retain explicit cards. */
export function buildSkuChoices(skus: ProductSku[], locale: string): SkuChoices | null {
  const active = skus.filter((sku) => sku.is_active)
  if (active.length < 2) return null
  const ids = new Set<number>()
  const combinations = new Set<string>()
  let dimensions: string[] | null = null
  const rows: SkuChoiceRow[] = []
  let options: Map<string, string>[] = []
  for (const sku of active) {
    if (!Number.isSafeInteger(sku.id) || sku.id <= 0 || ids.has(sku.id) || !record(sku.spec_values)) return null
    ids.add(sku.id)
    const specs = localized(sku.spec_values) ? { '': sku.spec_values } : sku.spec_values
    const keys = Object.keys(specs).sort()
    if (!keys.length || keys.length > MAX_DIMENSIONS || keys.some((key) => key !== '' && !key.trim())) return null
    if (dimensions === null) { dimensions = keys; options = keys.map(() => new Map<string, string>()) }
    else if (JSON.stringify(keys) !== JSON.stringify(dimensions)) return null
    const values: string[] = []
    for (const [index, key] of keys.entries()) {
      const value = choiceValue(specs[key], locale)
      const choices = options[index]
      if (!value || !choices) return null
      if ([...choices].some(([id, label]) => id !== value.key && label === value.label)) return null
      choices.set(value.key, value.label)
      values.push(value.key)
    }
    const combination = JSON.stringify(values)
    if (combinations.has(combination)) return null
    combinations.add(combination)
    rows.push({ sku, values })
  }
  if (!dimensions) return null
  const names = dimensions
  // Shared attributes are shown in the selected SKU, rather than needing another click.
  const varying = names.map((_, i) => i).filter((i) => (options[i]?.size ?? 0) > 1)
  if (!varying.length) return null
  return {
    dimensions: varying.map((i) => names[i] ?? ''),
    values: varying.map((i) => [...(options[i] ?? new Map<string, string>())].map(([key, label]) => ({ key, label }))),
    rows: rows.map((row) => ({ sku: row.sku, values: varying.map((i) => row.values[i] ?? '') })),
  }
}

export function skuChoiceRows(model: SkuChoices, index: number, selections: readonly string[]): SkuChoiceRow[] {
  if (!Number.isInteger(index) || index < 0 || index >= model.dimensions.length) return []
  return model.rows.filter((row) => row.values.slice(0, index).every((value, i) => selections[i] === value))
}
export function resolveSkuChoice(model: SkuChoices, selections: readonly string[]): ProductSku | null {
  if (selections.length !== model.dimensions.length) return null
  const rows = model.rows.filter((row) => row.values.every((value, i) => selections[i] === value))
  return rows.length === 1 ? rows[0]?.sku ?? null : null
}
export function initialSkuSelection(skus: ProductSku[], previous: number, keepCurrent: boolean): number {
  const active = skus.filter((sku) => sku.is_active)
  if (active.some((sku) => !Number.isSafeInteger(sku.id) || sku.id <= 0) || new Set(active.map((sku) => sku.id)).size !== active.length) return 0
  if (active.length === 1) return active[0]?.id ?? 0
  return keepCurrent && active.some((sku) => sku.id === previous) ? previous : 0
}
