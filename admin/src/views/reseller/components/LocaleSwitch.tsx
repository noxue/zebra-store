import { defineComponent, type PropType } from 'vue'
import { cn } from '@/components/ui'
import { resellerLocales, type ResellerLocale } from '@/utils/resellerSiteConfig'

export const RESELLER_LOCALE_LABELS: Record<ResellerLocale, string> = {
  'zh-CN': '简体',
  'zh-TW': '繁體',
  'en-US': 'EN',
}

/** Segmented zh-CN / zh-TW / en-US switch (port of ResellerLocaleSwitch.vue). */
export const LocaleSwitch = defineComponent({
  name: 'ResellerLocaleSwitch',
  props: {
    modelValue: { type: String as PropType<ResellerLocale>, default: 'zh-CN' },
  },
  emits: { 'update:modelValue': (_v: ResellerLocale) => true },
  setup(props, { emit }) {
    return () => (
      <div class="inline-flex rounded-zs-sm border border-line bg-surface-muted p-0.5">
        {resellerLocales.map((loc) => (
          <button
            key={loc}
            type="button"
            class={cn(
              'rounded-[8px] px-3 py-1 text-xs font-semibold transition-colors',
              loc === props.modelValue ? 'bg-surface-solid text-primary shadow-zs-sm' : 'text-muted hover:text-fg',
            )}
            onClick={() => emit('update:modelValue', loc)}
          >
            {RESELLER_LOCALE_LABELS[loc]}
          </button>
        ))}
      </div>
    )
  },
})

export default LocaleSwitch
