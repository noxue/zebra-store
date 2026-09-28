import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Switch, cn } from '@/components/ui'
import { SUPPORTED_LANGS, type LangCode } from './settingsUtils'

/** 简体中文 / 繁體中文 / English segmented switch used by the settings pages. */
export const LangSwitcher = defineComponent({
  name: 'SettingsLangSwitcher',
  props: { modelValue: { type: String as PropType<LangCode>, default: 'zh-CN' } },
  emits: { 'update:modelValue': (_v: LangCode) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const labels: Record<LangCode, string> = { 'zh-CN': 'admin.common.lang.zhCN', 'zh-TW': 'admin.common.lang.zhTW', 'en-US': 'admin.common.lang.enUS' }
    return () => (
      <div class="zs-glass flex max-w-full overflow-x-auto rounded-zs p-1" role="tablist">
        {SUPPORTED_LANGS.map((code) => (
          <button
            key={code}
            type="button"
            role="tab"
            aria-selected={props.modelValue === code}
            data-lang={code}
            class={cn(
              'h-7 shrink-0 rounded-[10px] px-3 text-xs font-medium transition-colors',
              props.modelValue === code ? 'bg-primary-soft text-primary' : 'text-muted hover:text-fg',
            )}
            onClick={() => emit('update:modelValue', code)}
          >
            {t(labels[code])}
          </button>
        ))}
      </div>
    )
  },
})

export const LangTag = defineComponent({
  name: 'SettingsLangTag',
  props: { lang: { type: String, required: true } },
  setup(props) {
    return () => <span class="rounded-zs-sm bg-surface-muted px-2 py-0.5 font-mono text-[11px] text-muted">{props.lang}</span>
  },
})

/** Bordered row: switch + label + description (the original's `rounded-lg border bg-muted/20` toggle rows). */
export const ToggleRow = defineComponent({
  name: 'SettingsToggleRow',
  props: {
    modelValue: Boolean,
    label: { type: String, required: true },
    description: String,
    disabled: Boolean,
    /** switch on the right instead of the left */
    trailing: Boolean,
  },
  emits: { 'update:modelValue': (_v: boolean) => true },
  setup(props, { emit }) {
    return () => {
      const sw = <Switch modelValue={props.modelValue} disabled={props.disabled} onUpdate:modelValue={(v: boolean) => emit('update:modelValue', v)} />
      return (
        <div
          class={cn(
            'flex gap-3 rounded-zs border border-line bg-surface-muted/60 px-4 py-3',
            props.trailing ? 'items-center justify-between' : 'items-center',
          )}
        >
          {!props.trailing && sw}
          <div class="min-w-0">
            <p class="text-sm font-medium text-fg">{props.label}</p>
            {props.description && <p class="mt-0.5 text-xs text-muted">{props.description}</p>}
          </div>
          {props.trailing && sw}
        </div>
      )
    }
  },
})

/** Inner block with a tinted header (the original's nested `rounded-xl border` sections). */
export const SubSection = defineComponent({
  name: 'SettingsSubSection',
  props: { title: String, description: String, bodyClass: String },
  setup(props, { slots }) {
    return () => (
      <div class="overflow-hidden rounded-zs border border-line">
        {(props.title || slots.extra) && (
          <div class="flex flex-wrap items-center justify-between gap-3 border-b border-line bg-surface-muted/70 px-4 py-3">
            <div>
              {props.title && <h4 class="text-sm font-semibold text-fg">{props.title}</h4>}
              {props.description && <p class="mt-0.5 text-xs text-muted">{props.description}</p>}
            </div>
            {slots.extra?.()}
          </div>
        )}
        <div class={cn('p-4', props.bodyClass)}>{slots.default?.()}</div>
      </div>
    )
  },
})

export const EmptyHint = defineComponent({
  name: 'SettingsEmptyHint',
  props: { text: { type: String, required: true } },
  setup(props) {
    return () => <div class="rounded-zs border border-dashed border-line-strong px-3 py-6 text-center text-xs text-muted">{props.text}</div>
  },
})
