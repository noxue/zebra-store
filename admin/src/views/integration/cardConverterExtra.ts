import type { AdminCardConverterType } from '@/api/types'

export type ExtraField = NonNullable<AdminCardConverterType['fields']>[number]

export const extraVariables = [
  { label: '订单号', value: 'order.order_no' },
  { label: '购买数量', value: 'order.quantity' },
  { label: '订单金额', value: 'order.total_amount' },
  { label: '站点域名', value: 'site.domain' },
  { label: '站点名称', value: 'site.name' },
  { label: '商品 ID', value: 'product.id' },
  { label: '商品名称', value: 'product.title' },
  { label: 'SKU ID', value: 'sku.id' },
  { label: 'SKU 规格', value: 'sku.specifications' },
  { label: '上游 ID', value: 'source.id' },
  { label: '上游名称', value: 'source.name' },
  { label: '上游协议', value: 'source.protocol' },
] as const

const validVariables = new Set<string>(extraVariables.map((variable) => variable.value))
export const ruleVariables = [...extraVariables, { label: '卡密（每张均匹配）', value: 'card' }] as const
export const ruleOperators = [
  { label: '等于', value: 'eq' }, { label: '不等于', value: 'ne' },
  { label: '大于', value: 'gt' }, { label: '大于等于', value: 'gte' },
  { label: '小于', value: 'lt' }, { label: '小于等于', value: 'lte' },
  { label: '正则匹配', value: 'regex' }, { label: '正则不匹配', value: 'regex_not' },
] as const
export const maxExtraRuleCases = 20
export const maxExtraComparisonBytes = 512

export interface ExtraResult { literal?: unknown; variable?: string }
export interface ExtraCase { when: { variable: string; operator: string; value: string }; then: ExtraResult }
export interface ExtraRule { $zs_rule: { cases: ExtraCase[]; else: ExtraResult } }

export function isExtraRule(value: unknown): value is ExtraRule {
  return value !== null && typeof value === 'object' && !Array.isArray(value) && Object.hasOwn(value, '$zs_rule')
}

export function defaultExtraRule(): ExtraRule {
  return { $zs_rule: { cases: [{ when: { variable: 'source.id', operator: 'eq', value: '' }, then: { literal: '' } }], else: { literal: '' } } }
}
const placeholder = /{{\s*([^{}]+?)\s*}}/g

export function parseExtraObject(text: string): Record<string, unknown> {
  const value: unknown = JSON.parse(text)
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error('extra 必须是 JSON 对象')
  }
  return value as Record<string, unknown>
}

export function validateExtra(extra: Record<string, unknown>, fields: ExtraField[] = []): string | null {
  for (const field of fields) {
    const value = extra[field.key]
    if (field.required && (value === undefined || value === null || value === '')) {
      return `请填写「${field.label}」`
    }
    if (field.kind === 'select' && !isExtraRule(value) && value !== undefined && value !== '' && !field.options?.includes(String(value))) {
      return `「${field.label}」不在可选范围内`
    }
  }

  const check = (value: unknown): string | null => {
    if (isExtraRule(value)) {
      const rule = value.$zs_rule
      if (Object.keys(value).length !== 1 || !rule || typeof rule !== 'object' || Array.isArray(rule) || Object.keys(rule).length !== 2 || !Object.hasOwn(rule, 'else') || !Array.isArray(rule.cases) || rule.cases.length < 1 || rule.cases.length > maxExtraRuleCases) return '条件规则格式不正确，且需要 1 到 20 条条件'
      const resultError = (result: ExtraResult): string | null => {
        if (!result || typeof result !== 'object' || Array.isArray(result) || Object.keys(result).length !== 1) return '规则结果必须是固定值或变量'
        if (Object.hasOwn(result, 'variable')) return typeof result.variable === 'string' && validVariables.has(result.variable) ? null : '规则结果变量不受支持'
        if (Object.hasOwn(result, 'literal')) return check(result.literal)
        return '规则结果必须是固定值或变量'
      }
      for (const item of rule.cases) {
        if (!item || typeof item !== 'object' || Object.keys(item).length !== 2 || !item.when || typeof item.when !== 'object' || Array.isArray(item.when) || Object.keys(item.when).length !== 3) return '条件规则格式不正确'
        const { variable, operator, value: expected } = item.when
        if (!ruleVariables.some((entry) => entry.value === variable)) return `不支持的条件变量：${variable}`
        if (!ruleOperators.some((entry) => entry.value === operator)) return `不支持的比较方式：${operator}`
        if (typeof expected !== 'string' || new TextEncoder().encode(expected).length > maxExtraComparisonBytes) return '比较值必须是 512 字节以内的文本'
        if (operator === 'regex' || operator === 'regex_not') {
          try { new RegExp(expected) } catch { return '正则表达式格式不正确' }
        }
        const error = resultError(item.then)
        if (error) return error
      }
      return resultError(rule.else)
    }
    if (typeof value === 'string') {
      const matches = [...value.matchAll(placeholder)]
      const remainder = value.replace(placeholder, '')
      if (remainder.includes('{{') || remainder.includes('}}')) return '变量占位符格式不正确'
      for (const match of matches) {
        if (!validVariables.has(match[1].trim())) return `不支持的变量：${match[1].trim()}`
      }
    } else if (Array.isArray(value)) {
      for (const item of value) {
        const error = check(item)
        if (error) return error
      }
    } else if (value && typeof value === 'object') {
      for (const item of Object.values(value)) {
        const error = check(item)
        if (error) return error
      }
    }
    return null
  }
  return check(extra)
}

export function variablePlaceholder(path: string): string {
  return `{{${path}}}`
}
