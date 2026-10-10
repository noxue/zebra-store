import { describe, expect, it } from 'vitest'
import { defaultExtraRule, extraVariables, maxExtraComparisonBytes, maxExtraRuleCases, parseExtraObject, ruleOperators, ruleVariables, validateExtra } from './cardConverterExtra'
import contract from '../../../../docs/protocol/card-converter-extra-contract.json'

const fields = [{ key: 'provider', label: '上游来源', required: true, kind: 'select', options: ['p1', 'p2'] }]

describe('converter extra editor data', () => {
  it('uses the same variable names and rule bounds as the backend contract', () => {
    expect(extraVariables.map((item) => item.value)).toEqual(contract.variables)
    expect(ruleVariables.filter((item) => !contract.variables.includes(item.value as typeof contract.variables[number])).map((item) => item.value)).toEqual(contract.condition_only_variables)
    expect(ruleOperators.map((item) => item.value)).toEqual(contract.operators)
    expect(maxExtraRuleCases).toBe(contract.max_cases)
    expect(maxExtraComparisonBytes).toBe(contract.max_comparison_bytes)
  })
  it('parses advanced JSON into form values and preserves nested values', () => {
    const extra = parseExtraObject('{"provider":"p2","metadata":{"order":"{{order.order_no}}"},"enabled":true}')
    expect(extra).toEqual({ provider: 'p2', metadata: { order: '{{order.order_no}}' }, enabled: true })
    expect(validateExtra(extra, fields)).toBeNull()
  })

  it('rejects malformed JSON, missing required fields, and unsupported placeholders', () => {
    expect(() => parseExtraObject('{"provider":')).toThrow()
    expect(() => parseExtraObject('[]')).toThrow('JSON 对象')
    expect(validateExtra({}, fields)).toBe('请填写「上游来源」')
    expect(validateExtra({ provider: 'p3' }, fields)).toBe('「上游来源」不在可选范围内')
    expect(validateExtra({ provider: 'p1', order: '{{buyer.email}}' }, fields)).toBe('不支持的变量：buyer.email')
    expect(validateExtra({ provider: 'p1', order: '{{order.order_no}' }, fields)).toBe('变量占位符格式不正确')
  })

  it('round trips condition rules and validates regex and result variables', () => {
    const rule = defaultExtraRule()
    rule.$zs_rule.cases[0].when = { variable: 'card', operator: 'regex', value: '^P1-' }
    rule.$zs_rule.cases[0].then = { literal: 'p1' }
    rule.$zs_rule.else = { variable: 'source.name' }
    const extra = parseExtraObject(JSON.stringify({ provider: rule }))
    expect(validateExtra(extra, fields)).toBeNull()
    rule.$zs_rule.cases[0].when.value = '['
    expect(validateExtra({ provider: rule }, fields)).toBe('正则表达式格式不正确')
    rule.$zs_rule.cases[0].when.value = '^P1-'
    rule.$zs_rule.else = { variable: 'buyer.email' }
    expect(validateExtra({ provider: rule }, fields)).toBe('规则结果变量不受支持')
  })

  it('validates rule limits, unknown operators, and nested placeholders', () => {
    const rule = defaultExtraRule()
    rule.$zs_rule.cases[0].when.value = '7'
    rule.$zs_rule.cases[0].then = { literal: { note: '{{site.domain}}', enabled: true } }
    expect(validateExtra({ route: rule })).toBeNull()
    rule.$zs_rule.cases[0].then = { literal: '{{buyer.email}}' }
    expect(validateExtra({ route: rule })).toBe('不支持的变量：buyer.email')
    rule.$zs_rule.cases[0].then = { literal: 'p1' }
    rule.$zs_rule.cases[0].when.operator = 'execute'
    expect(validateExtra({ route: rule })).toBe('不支持的比较方式：execute')
    rule.$zs_rule.cases[0].when.operator = 'eq'
    rule.$zs_rule.cases = Array.from({ length: 21 }, () => ({ when: { variable: 'source.id', operator: 'eq', value: '7' }, then: { literal: 'p1' } }))
    expect(validateExtra({ route: rule })).toContain('1 到 20 条条件')
  })

  it('uses UTF-8 byte limits for comparisons, matching the Rust validator', () => {
    const rule = defaultExtraRule()
    rule.$zs_rule.cases[0].when.value = '中'.repeat(171)
    expect(validateExtra({ route: rule })).toContain('512 字节')
    rule.$zs_rule.cases[0].when.value = 'a'.repeat(512)
    expect(validateExtra({ route: rule })).toBeNull()
  })
})
