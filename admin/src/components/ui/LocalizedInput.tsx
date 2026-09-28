import { defineComponent, ref, type PropType, type VNodeChild } from 'vue'
import { cn } from './cn'
import { Input } from './Input'
import { Textarea } from './Textarea'

export const LOCALES = ['zh-CN', 'zh-TW', 'en-US'] as const
export type LocaleCode = (typeof LOCALES)[number]
export type LocalizedValue = Partial<Record<LocaleCode, string>> & Record<string, string | undefined>

const localeLabel: Record<LocaleCode, string> = { 'zh-CN': '简', 'zh-TW': '繁', 'en-US': 'EN' }

/**
 * Localized text field with zh-CN / zh-TW / en-US tabs. Value is `{lang: text}`.
 * `mode="rich"` lets the caller render its own editor via the `editor` render prop.
 */
export const LocalizedInput = defineComponent({
  name: 'ZsLocalizedInput',
  props: {
    modelValue: { type: Object as PropType<LocalizedValue | null | undefined>, default: () => ({}) },
    mode: { type: String as PropType<'input' | 'textarea' | 'rich'>, default: 'input' },
    rows: { type: Number, default: 3 },
    placeholder: String,
    disabled: Boolean,
    editor: { type: Function as PropType<(value: string, update: (v: string) => void, locale: LocaleCode) => VNodeChild>, default: undefined },
  },
  emits: { 'update:modelValue': (_v: LocalizedValue) => true },
  setup(props, { emit }) {
    const active = ref<LocaleCode>('zh-CN')
    const update = (locale: LocaleCode, v: string) => emit('update:modelValue', { ...(props.modelValue ?? {}), [locale]: v })
    return () => {
      const value = props.modelValue ?? {}
      const current = value[active.value] ?? ''
      return (
        <div class="space-y-1.5">
          <div class="flex items-center gap-1">
            {LOCALES.map((code) => {
              const filled = !!(value[code] ?? '').trim()
              return (
                <button
                  type="button"
                  key={code}
                  title={code}
                  onClick={() => (active.value = code)}
                  class={cn(
                    'relative h-6 rounded-full px-2.5 text-[11px] font-semibold transition-all',
                    active.value === code ? 'zs-gradient-bg text-on-primary' : 'bg-surface-muted text-muted hover:text-primary',
                  )}
                >
                  {localeLabel[code]}
                  {filled && active.value !== code && <span class="absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full bg-success" />}
                </button>
              )
            })}
          </div>
          {props.mode === 'rich' && props.editor ? (
            <div key={active.value}>{props.editor(current, (v) => update(active.value, v), active.value)}</div>
          ) : props.mode === 'textarea' ? (
            <Textarea
              rows={props.rows}
              disabled={props.disabled}
              placeholder={props.placeholder}
              modelValue={current}
              onUpdate:modelValue={(v: string) => update(active.value, v)}
            />
          ) : (
            <Input
              disabled={props.disabled}
              placeholder={props.placeholder}
              modelValue={current}
              onUpdate:modelValue={(v: string | number) => update(active.value, String(v))}
            />
          )}
        </div>
      )
    }
  },
})

export default LocalizedInput
