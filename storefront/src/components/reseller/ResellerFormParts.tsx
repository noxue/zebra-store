import { computed, defineComponent, ref, watch, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { ImageOff, ImagePlus, Trash2 } from 'lucide-vue-next'
import type { ResellerLocalizedText, ResellerProductSettingPayloadItem } from '@/api/types'
import { useResellerImageUpload } from '@/composables/reseller/useResellerSiteConfig'
import { Badge, Button, Input, Select, Switch, cn } from '@/components/ui'
import { getImageUrl } from '@/utils/image'
import { RESELLER_PRICING_MODES } from '@/utils/reseller/constants'
import { getResellerPricingModeLabelKey } from '@/utils/reseller/productSettings'
import { resellerLocales, type ResellerLocale } from '@/utils/reseller/siteConfig'

const LOCALE_SHORT: Record<ResellerLocale, string> = { 'zh-CN': '简体', 'zh-TW': '繁體', 'en-US': 'EN' }

/** Small segmented control for the edited language. */
export const ResellerLocaleTabs = defineComponent({
  name: 'ResellerLocaleTabs',
  props: { modelValue: { type: String as PropType<ResellerLocale>, default: 'zh-CN' } },
  emits: { 'update:modelValue': (_v: ResellerLocale) => true },
  setup(props, { emit }) {
    return () => (
      <div class="inline-flex gap-1 rounded-full border border-line bg-surface p-1">
        {resellerLocales.map((loc) => (
          <button
            key={loc}
            type="button"
            class={cn('h-7 rounded-full px-3 text-xs font-bold transition', props.modelValue === loc ? 'zs-gradient-bg text-on-primary' : 'text-muted hover:bg-primary-soft')}
            onClick={() => emit('update:modelValue', loc)}
          >
            {LOCALE_SHORT[loc]}
          </button>
        ))}
      </div>
    )
  },
})

/** Input bound to one locale of a localized text object; emits a new object via v-model. */
export const LocalizedInput = defineComponent({
  name: 'LocalizedInput',
  props: {
    modelValue: { type: Object as PropType<ResellerLocalizedText>, default: () => ({ 'zh-CN': '', 'zh-TW': '', 'en-US': '' }) },
    locale: { type: String as PropType<ResellerLocale>, required: true },
    placeholder: { type: String, default: '' },
    disabled: Boolean,
    multiline: Boolean,
    rows: { type: Number, default: 4 },
  },
  emits: { 'update:modelValue': (_v: ResellerLocalizedText) => true },
  setup(props, { emit }) {
    const set = (text: string) => emit('update:modelValue', { ...props.modelValue, [props.locale]: text })
    const fieldCls =
      'w-full rounded-zs border border-line bg-surface-strong px-4 text-sm text-fg outline-none transition focus:border-primary focus:shadow-[var(--zs-ring)] disabled:opacity-60'
    return () =>
      props.multiline ? (
        <textarea
          class={cn(fieldCls, 'py-3 leading-relaxed')}
          rows={props.rows}
          value={props.modelValue[props.locale]}
          placeholder={props.placeholder}
          disabled={props.disabled}
          onInput={(e: Event) => set((e.target as HTMLTextAreaElement).value)}
        />
      ) : (
        <input
          class={cn(fieldCls, 'h-11')}
          value={props.modelValue[props.locale]}
          placeholder={props.placeholder}
          disabled={props.disabled}
          onInput={(e: Event) => set((e.target as HTMLInputElement).value)}
        />
      )
  },
})

/** Image URL field with upload button and preview. */
export const ResellerImageField = defineComponent({
  name: 'ResellerImageField',
  props: {
    modelValue: { type: String, default: '' },
    disabled: Boolean,
  },
  emits: { 'update:modelValue': (_v: string) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const { uploading, error, upload } = useResellerImageUpload()
    const input = ref<HTMLInputElement | null>(null)
    const broken = ref(false)
    watch(
      () => props.modelValue,
      () => {
        broken.value = false
      },
    )
    const preview = computed(() => (props.modelValue ? getImageUrl(props.modelValue) : ''))
    const onFile = async (e: Event) => {
      const el = e.target as HTMLInputElement
      const file = el.files?.[0]
      el.value = ''
      if (!file) return
      const url = await upload(file)
      if (url) emit('update:modelValue', url)
    }
    return () => (
      <div class="flex items-start gap-3">
        <div class="flex size-16 shrink-0 items-center justify-center overflow-hidden rounded-zs border border-line bg-surface-muted">
          {preview.value && !broken.value ? (
            <img src={preview.value} alt="" class="size-full object-contain" onError={() => (broken.value = true)} />
          ) : (
            <ImageOff class="size-5 text-muted" />
          )}
        </div>
        <div class="min-w-0 flex-1 space-y-2">
          <Input modelValue={props.modelValue} disabled={props.disabled} placeholder="/uploads/... or https://" onUpdate:modelValue={(v: string) => emit('update:modelValue', v)} />
          <div class="flex flex-wrap items-center gap-2">
            <input ref={input} type="file" accept="image/*" class="hidden" onChange={onFile} />
            <Button size="xs" variant="soft" disabled={props.disabled} loading={uploading.value} onClick={() => input.value?.click()}>
              <ImagePlus class="size-3.5" />
              {uploading.value ? t('zsReseller.uploading') : props.modelValue ? t('personalCenter.reseller.siteConfig.replace') : t('personalCenter.reseller.siteConfig.upload')}
            </Button>
            {props.modelValue && (
              <Button size="xs" variant="ghost" disabled={props.disabled} onClick={() => emit('update:modelValue', '')}>
                <Trash2 class="size-3.5" />
                {t('personalCenter.reseller.siteConfig.actions.remove')}
              </Button>
            )}
            {error.value && <span class="text-xs text-danger-text">{error.value}</span>}
          </div>
        </div>
      </div>
    )
  },
})

/** One pricing rule editor row (product level or a SKU). */
export const ResellerRuleEditor = defineComponent({
  name: 'ResellerRuleEditor',
  props: {
    label: { type: String, required: true },
    basePrice: { type: String, default: '-' },
    effectivePrice: { type: String, default: '' },
    invalid: Boolean,
    errorCode: { type: String, default: '' },
    modelValue: { type: Object as PropType<ResellerProductSettingPayloadItem>, required: true },
  },
  emits: { 'update:modelValue': (_v: ResellerProductSettingPayloadItem) => true },
  setup(props, { emit }) {
    const { t } = useI18n()
    const k = (key: string) => t(`personalCenter.reseller.productSettings.${key}`)
    const update = <K extends keyof ResellerProductSettingPayloadItem>(key: K, value: ResellerProductSettingPayloadItem[K]) =>
      emit('update:modelValue', { ...props.modelValue, [key]: value })
    const modeOptions = computed(() => RESELLER_PRICING_MODES.map((m) => ({ value: m, label: k(getResellerPricingModeLabelKey(m)) })))
    const valueField = computed(() => {
      const mode = props.modelValue.pricing_mode
      if (mode === 'markup_percent') return { key: 'markup_percent' as const, placeholder: k('markupPercentField'), suffix: '%' }
      if (mode === 'fixed_markup') return { key: 'fixed_markup_amount' as const, placeholder: k('fixedMarkupField'), suffix: '' }
      if (mode === 'fixed_price') return { key: 'fixed_price_amount' as const, placeholder: k('fixedPriceField'), suffix: '' }
      return null
    })
    return () => {
      const field = valueField.value
      return (
        <div class={cn('space-y-3 rounded-zs border p-4', props.invalid ? 'border-danger/50 bg-danger-soft/40' : 'border-line bg-surface-strong')}>
          <div class="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
            <div class="min-w-0">
              <div class="truncate text-sm font-bold text-fg">{props.label}</div>
              <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                <Badge tone="neutral" size="xs">
                  {k('basePrice')} {props.basePrice}
                </Badge>
                <Badge tone={props.invalid ? 'danger' : 'accent'} size="xs">
                  {k('effectivePrice')} {props.effectivePrice || '-'}
                </Badge>
                {props.invalid && (
                  <span class="text-xs font-bold text-danger-text">{props.errorCode === 'markup_exceeded' ? k('markupExceededHint') : k('priceInvalidHint')}</span>
                )}
              </div>
            </div>
            <label class="inline-flex items-center gap-2 text-sm text-fg">
              <Switch modelValue={props.modelValue.is_listed} label={k('listed')} onUpdate:modelValue={(v: boolean) => update('is_listed', v)} />
              {k('listed')}
            </label>
          </div>
          <div class="grid gap-3 sm:grid-cols-[220px_minmax(0,1fr)]">
            <Select modelValue={props.modelValue.pricing_mode} options={modeOptions.value} onUpdate:modelValue={(v: string | number) => update('pricing_mode', String(v))} />
            {field && (
              <Input
                modelValue={props.modelValue[field.key]}
                inputmode="decimal"
                placeholder={field.placeholder}
                onUpdate:modelValue={(v: string) => update(field.key, v.trim())}
              >
                {{ suffix: field.suffix ? () => <span class="pr-2 text-sm">{field.suffix}</span> : undefined }}
              </Input>
            )}
          </div>
        </div>
      )
    }
  },
})
