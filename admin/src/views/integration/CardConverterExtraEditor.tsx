import { computed, defineComponent, ref, type PropType } from 'vue'
import { Button, FormField, Input, Select } from '@/components/ui'
import type { ExtraField, ExtraResult, ExtraRule } from './cardConverterExtra'
import { defaultExtraRule, extraVariables, isExtraRule, parseExtraObject, ruleOperators, ruleVariables, validateExtra, variablePlaceholder } from './cardConverterExtra'

type ExtraObject = Record<string, unknown>
type ValueKind = 'text' | 'number' | 'boolean' | 'json' | 'rule'

function kindOf(value: unknown): ValueKind {
  if (isExtraRule(value)) return 'rule'
  if (typeof value === 'number') return 'number'
  if (typeof value === 'boolean') return 'boolean'
  if (value !== null && typeof value === 'object') return 'json'
  return 'text'
}

export default defineComponent({
  name: 'CardConverterExtraEditor',
  props: {
    modelValue: { type: Object as PropType<ExtraObject>, required: true },
    fields: { type: Array as PropType<ExtraField[]>, default: () => [] },
    sources: { type: Array as PropType<Array<{ id: number; name: string }>>, default: () => [] },
  },
  emits: { 'update:modelValue': (_value: ExtraObject) => true },
  setup(props, { emit, expose }) {
    const mode = ref<'form' | 'json'>('form')
    const jsonText = ref('')
    const error = ref('')
    const newKey = ref('')
    const nestedDrafts = ref<Record<string, string>>({})
    const nestedErrors = ref<Record<string, string>>({})
    const kindOverrides = ref<Record<string, ValueKind>>({})
    const knownKeys = computed(() => new Set(props.fields.map((field) => field.key)))
    const customEntries = computed(() => Object.entries(props.modelValue).filter(([key]) => !knownKeys.value.has(key)))
    const variableOptions = extraVariables.map((variable) => ({ label: `${variable.label} · {{${variable.value}}}`, value: variable.value }))

    const update = (value: ExtraObject) => {
      emit('update:modelValue', value)
      error.value = ''
    }
    const setValue = (key: string, value: unknown) => update({ ...props.modelValue, [key]: value })
    const removeValue = (key: string) => {
      const next = { ...props.modelValue }
      delete next[key]
      const drafts = { ...nestedDrafts.value }
      const errors = { ...nestedErrors.value }
      delete drafts[key]
      delete errors[key]
      nestedDrafts.value = drafts
      nestedErrors.value = errors
      const kinds = { ...kindOverrides.value }
      delete kinds[key]
      kindOverrides.value = kinds
      update(next)
    }
    const addKey = () => {
      const key = newKey.value.trim()
      if (!key || Object.hasOwn(props.modelValue, key) || knownKeys.value.has(key)) {
        error.value = '请输入未使用的自定义字段名'
        return
      }
      setValue(key, '')
      newKey.value = ''
    }
    const appendVariable = (key: string, path: string) => {
      const current = props.modelValue[key]
      setValue(key, `${typeof current === 'string' ? current : ''}${variablePlaceholder(path)}`)
    }
    const setNested = (key: string, text: string) => {
      nestedDrafts.value = { ...nestedDrafts.value, [key]: text }
      try {
        const value: unknown = JSON.parse(text)
        if (value === null || typeof value !== 'object') throw new Error('请输入 JSON 对象或数组')
        const errors = { ...nestedErrors.value }
        delete errors[key]
        nestedErrors.value = errors
        setValue(key, value)
      } catch {
        nestedErrors.value = { ...nestedErrors.value, [key]: 'JSON 格式不正确' }
      }
    }
    const setKind = (key: string, kind: ValueKind) => {
      kindOverrides.value = { ...kindOverrides.value, [key]: kind }
      const old = props.modelValue[key]
      const value = kind === 'rule' ? defaultExtraRule() : kind === 'text' ? (isExtraRule(old) ? '' : String(old ?? '')) : kind === 'number' ? 0 : kind === 'boolean' ? false : {}
      setValue(key, value)
      const drafts = { ...nestedDrafts.value }
      const errors = { ...nestedErrors.value }
      delete drafts[key]
      delete errors[key]
      nestedDrafts.value = drafts
      nestedErrors.value = errors
    }
    const readAdvanced = (): ExtraObject | null => {
      try {
        return parseExtraObject(jsonText.value)
      } catch (cause) {
        error.value = cause instanceof Error ? cause.message : 'JSON 格式不正确'
        return null
      }
    }
    const changeMode = (next: 'form' | 'json') => {
      if (next === mode.value) return
      if (next === 'json') {
        if (Object.keys(nestedErrors.value).length > 0) {
          error.value = '请先修正表单中的 JSON 字段'
          return
        }
        jsonText.value = JSON.stringify(props.modelValue, null, 2)
        mode.value = 'json'
        error.value = ''
        return
      }
      const parsed = readAdvanced()
      if (!parsed) return
      update(parsed)
      nestedDrafts.value = {}
      nestedErrors.value = {}
      kindOverrides.value = {}
      mode.value = 'form'
    }
    const editAdvanced = (text: string) => {
      jsonText.value = text
      const parsed = readAdvanced()
      if (parsed) update(parsed)
    }
    const validate = (): ExtraObject | null => {
      const value = mode.value === 'json' ? readAdvanced() : props.modelValue
      if (!value) return null
      const nestedError = Object.values(nestedErrors.value)[0]
      const emptyNumber = Object.entries(kindOverrides.value).find(([key, kind]) => kind === 'number' && (value[key] === '' || typeof value[key] !== 'number'))
      const validationError = nestedError || (emptyNumber ? `请填写「${emptyNumber[0]}」的数字值` : validateExtra(value, props.fields))
      if (validationError) {
        error.value = validationError
        return null
      }
      error.value = ''
      return value
    }
    expose({ validate })

    const textControl = (key: string, value: unknown) => <div class="space-y-2">
      <Input modelValue={String(value ?? '')} onUpdate:modelValue={(next) => setValue(key, String(next))} placeholder="可输入常量或插入变量" />
      <Select modelValue="" options={variableOptions} onUpdate:modelValue={(path) => appendVariable(key, String(path))} placeholder="插入订单变量" />
    </div>

    const setRule = (key: string, rule: ExtraRule) => setValue(key, rule)
    const setRuleCase = (key: string, rule: ExtraRule, index: number, patch: Partial<ExtraRule['$zs_rule']['cases'][number]>) => {
      const cases = rule.$zs_rule.cases.map((entry, position) => position === index ? { ...entry, ...patch } : entry)
      setRule(key, { $zs_rule: { ...rule.$zs_rule, cases } })
    }
    const resultControl = (result: ExtraResult, onChange: (next: ExtraResult) => void) => <div class="flex flex-wrap gap-2">
      <div class="w-32"><Select modelValue={Object.hasOwn(result, 'variable') ? 'variable' : 'literal'} options={[{ label: '发送固定值', value: 'literal' }, { label: '发送变量值', value: 'variable' }]} onUpdate:modelValue={(kind) => onChange(kind === 'variable' ? { variable: extraVariables[0].value } : { literal: '' })} /></div>
      {Object.hasOwn(result, 'variable')
        ? <div class="min-w-40 flex-1"><Select modelValue={result.variable ?? ''} options={variableOptions} onUpdate:modelValue={(path) => onChange({ variable: String(path) })} /></div>
        : <div class="min-w-40 flex-1"><Input modelValue={String(result.literal ?? '')} onUpdate:modelValue={(value) => onChange({ literal: String(value) })} placeholder="传给转换器的值，如 p1" /></div>}
    </div>
    const comparisonControl = (key: string, rule: ExtraRule, index: number) => {
      const when = rule.$zs_rule.cases[index].when
      const sourceOptions = props.sources.map((source) => ({ label: `${source.name}（ID ${source.id}）`, value: when.variable === 'source.id' ? String(source.id) : source.name }))
      if ((when.variable === 'source.id' || when.variable === 'source.name') && props.sources.length > 0 && (when.operator === 'eq' || when.operator === 'ne')) {
        if (when.value && !sourceOptions.some((option) => option.value === when.value)) sourceOptions.push({ label: `当前配置：${when.value}`, value: when.value })
        return <Select modelValue={when.value} options={sourceOptions} onUpdate:modelValue={(value) => setRuleCase(key, rule, index, { when: { ...when, value: String(value) } })} placeholder="选择商品上游" />
      }
      return <Input modelValue={when.value} onUpdate:modelValue={(value) => setRuleCase(key, rule, index, { when: { ...when, value: String(value) } })} placeholder={when.operator.startsWith('regex') ? '输入正则表达式，如 ^P1-' : '输入比较值'} />
    }
    const ruleControl = (key: string, rule: ExtraRule) => <div class="space-y-3 rounded-zs-sm border border-line p-3">
      <p class="text-xs text-muted">从上到下匹配，第一条命中即使用其结果。选择上游时显示名称，规则建议按稳定 ID 匹配；结果中的固定值会发送给转换器。卡密条件要求本批每张卡密都匹配。</p>
      {rule.$zs_rule.cases.map((entry, index) => <div key={index} class="space-y-2 rounded-zs-sm border border-line p-2">
        <div class="flex flex-wrap items-center gap-2"><span class="text-xs">如果</span>
          <div class="min-w-40 flex-1"><Select modelValue={entry.when.variable} options={ruleVariables.map((variable) => ({ label: variable.label, value: variable.value }))} onUpdate:modelValue={(value) => setRuleCase(key, rule, index, { when: { ...entry.when, variable: String(value), value: '' } })} /></div>
          <div class="w-32"><Select modelValue={entry.when.operator} options={ruleOperators.map((operator) => ({ label: operator.label, value: operator.value }))} onUpdate:modelValue={(value) => setRuleCase(key, rule, index, { when: { ...entry.when, operator: String(value) } })} /></div>
          <div class="min-w-32 flex-1">{comparisonControl(key, rule, index)}</div>
        </div>
        <div class="flex items-center gap-2"><span class="text-xs">则输出</span><div class="flex-1">{resultControl(entry.then, (thenResult) => setRuleCase(key, rule, index, { then: thenResult }))}</div></div>
        {rule.$zs_rule.cases.length > 1 && <Button size="sm" variant="danger" onClick={() => setRule(key, { $zs_rule: { ...rule.$zs_rule, cases: rule.$zs_rule.cases.filter((_, position) => position !== index) } })}>删除条件</Button>}
      </div>)}
      <Button size="sm" onClick={() => setRule(key, { $zs_rule: { ...rule.$zs_rule, cases: [...rule.$zs_rule.cases, defaultExtraRule().$zs_rule.cases[0]] } })}>添加条件</Button>
      <div class="flex items-center gap-2"><span class="text-xs">否则输出</span><div class="flex-1">{resultControl(rule.$zs_rule.else, (elseResult) => setRule(key, { $zs_rule: { ...rule.$zs_rule, else: elseResult } }))}</div></div>
    </div>

    return () => <div class="space-y-4 rounded-zs-md border border-line bg-surface-strong p-4">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div><div class="text-sm font-medium">发送给转换器的自定义参数 extra</div><p class="text-xs text-muted">这里的字段名和求值结果会发给转换器。变量在交付时替换；普通模式和 JSON 模式使用同一份配置。</p></div>
        <div class="flex gap-2"><Button size="sm" variant={mode.value === 'form' ? 'primary' : 'secondary'} onClick={() => changeMode('form')}>普通模式</Button><Button size="sm" variant={mode.value === 'json' ? 'primary' : 'secondary'} onClick={() => changeMode('json')}>高级 JSON</Button></div>
      </div>
      {mode.value === 'json' ? <div class="space-y-2">
        <textarea class="min-h-64 w-full rounded-zs-sm border border-line-strong bg-surface p-3 font-mono text-xs text-fg focus:border-primary focus:outline-none" spellcheck={false} value={jsonText.value} onInput={(event) => editAdvanced((event.target as HTMLTextAreaElement).value)} aria-label="extra JSON" />
        <p class="text-xs text-muted">最外层必须是 JSON 对象。占位符要写在 JSON 字符串内，例如 {String.raw`{"order_no":"{{order.order_no}}"}`}。</p>
      </div> : <div class="space-y-4">
        {props.fields.map((field) => <FormField key={field.key} label={field.label} required={field.required} hint={field.description}>
          <div class="mb-2 w-32"><Select modelValue={isExtraRule(props.modelValue[field.key]) ? 'rule' : 'fixed'} options={[{ label: '固定值', value: 'fixed' }, { label: '条件规则', value: 'rule' }]} onUpdate:modelValue={(next) => setKind(field.key, next === 'rule' ? 'rule' : 'text')} /></div>
          {isExtraRule(props.modelValue[field.key]) ? ruleControl(field.key, props.modelValue[field.key] as ExtraRule) : field.kind === 'select' ? <Select modelValue={String(props.modelValue[field.key] ?? '')} options={(field.options || []).map((value) => ({ label: value, value }))} onUpdate:modelValue={(value) => setValue(field.key, String(value))} placeholder="请选择" />
            : field.kind === 'boolean' ? <Select modelValue={props.modelValue[field.key] === undefined ? '' : String(props.modelValue[field.key])} options={[{ label: '是', value: 'true' }, { label: '否', value: 'false' }]} onUpdate:modelValue={(value) => setValue(field.key, value === 'true')} placeholder="请选择" />
              : field.kind === 'number' ? <div class="space-y-2"><Input modelValue={String(props.modelValue[field.key] ?? '')} onUpdate:modelValue={(value) => { const text = String(value); setValue(field.key, text.trim() !== '' && Number.isFinite(Number(text)) ? Number(text) : text) }} placeholder="输入数字或插入变量" /><Select modelValue="" options={variableOptions} onUpdate:modelValue={(path) => appendVariable(field.key, String(path))} placeholder="插入订单变量" /></div>
                : textControl(field.key, props.modelValue[field.key])}
        </FormField>)}
        {customEntries.value.map(([key, value]) => <div key={key} class="rounded-zs-sm border border-line p-3 space-y-2">
          <div class="flex flex-wrap items-center gap-2"><span class="min-w-28 flex-1 break-all font-mono text-xs">发送字段：{key}</span><div class="w-28"><Select modelValue={kindOverrides.value[key] ?? kindOf(value)} options={[{ label: '文本', value: 'text' }, { label: '数字', value: 'number' }, { label: '布尔值', value: 'boolean' }, { label: 'JSON', value: 'json' }, { label: '条件规则', value: 'rule' }]} onUpdate:modelValue={(next) => setKind(key, String(next) as ValueKind)} /></div><Button size="sm" variant="danger" onClick={() => removeValue(key)}>删除</Button></div>
          {isExtraRule(value) ? ruleControl(key, value) : (kindOverrides.value[key] ?? kindOf(value)) === 'text' ? textControl(key, value) : (kindOverrides.value[key] ?? kindOf(value)) === 'number' ? <Input type="number" modelValue={value as string | number} onUpdate:modelValue={(next) => setValue(key, next)} /> : (kindOverrides.value[key] ?? kindOf(value)) === 'boolean' ? <Select modelValue={String(value)} options={[{ label: '是', value: 'true' }, { label: '否', value: 'false' }]} onUpdate:modelValue={(next) => setValue(key, next === 'true')} /> : <textarea class="min-h-24 w-full rounded-zs-sm border border-line-strong bg-surface p-2 font-mono text-xs text-fg" value={nestedDrafts.value[key] ?? JSON.stringify(value, null, 2)} onInput={(event) => setNested(key, (event.target as HTMLTextAreaElement).value)} aria-label={`${key} JSON`} />}
          {nestedErrors.value[key] && <p class="text-xs text-danger-text">{nestedErrors.value[key]}</p>}
        </div>)}
        <div class="flex gap-2"><Input modelValue={newKey.value} onUpdate:modelValue={(value) => { newKey.value = String(value) }} placeholder="转换器要求的字段名，如渠道字段" /><Button size="sm" onClick={addKey}>添加发送字段</Button></div>
      </div>}
      {error.value && <p class="text-xs text-danger-text" role="alert">{error.value}</p>}
    </div>
  },
})
