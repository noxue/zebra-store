import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { FormField, Input, Select, Switch, cn } from '@/components/ui'
import type { AdminResellerProductSettingPayloadItem } from '@/api/types'
import { RESELLER_PRICING_MODES, pricingModeKey, type PreviewEntry } from '../resellerUtils'

/** One editable pricing rule (product-level or per SKU) with its live effective-price preview. */
export const SettingRuleRow = defineComponent({
  name: 'ResellerSettingRuleRow',
  props: {
    label: { type: String, required: true },
    name: { type: String, required: true },
    form: {
      type: Object as PropType<AdminResellerProductSettingPayloadItem>,
      required: true,
    },
    preview: {
      type: Object as PropType<PreviewEntry | undefined>,
      default: undefined,
    },
    hint: String,
  },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const f = props.form
      const invalid = props.preview?.valid === false
      return (
        <div class="grid gap-3 lg:grid-cols-[minmax(160px,1fr)_80px_170px_repeat(3,minmax(100px,1fr))_minmax(130px,160px)] lg:items-start">
          <FormField label={props.label}>
            <div class="flex min-h-9 items-center rounded-zs-sm border border-line bg-surface-muted px-3 py-1.5 text-sm">{props.name}</div>
          </FormField>
          <FormField label={t('admin.resellerProductSettings.columns.listed')}>
            <div class="flex h-9 items-center">
              <Switch v-model={f.is_listed} />
            </div>
          </FormField>
          <FormField label={t('admin.resellerProductSettings.columns.pricingMode')}>
            <Select
              v-model={f.pricing_mode}
              options={RESELLER_PRICING_MODES.map((m) => ({
                label: t(`admin.resellerProductSettings.modes.${pricingModeKey(m)}`),
                value: m,
              }))}
            />
          </FormField>
          <FormField label={t('admin.resellerProductSettings.modes.markupPercent')}>
            <Input v-model={f.markup_percent} mono />
          </FormField>
          <FormField label={t('admin.resellerProductSettings.modes.fixedMarkup')}>
            <Input v-model={f.fixed_markup_amount} mono />
          </FormField>
          <FormField label={t('admin.resellerProductSettings.modes.fixedPrice')}>
            <Input v-model={f.fixed_price_amount} mono />
          </FormField>
          <FormField label={t('admin.resellerProductSettings.preview.effectivePrice')} error={invalid ? props.hint : undefined}>
            <div
              class={cn(
                'zs-num flex h-9 items-center rounded-zs-sm border px-3 text-sm font-semibold',
                invalid ? 'border-danger/40 bg-danger-soft text-danger-text' : 'border-line bg-primary-soft/60 text-primary',
              )}
            >
              {props.preview?.effective || '-'}
            </div>
          </FormField>
        </div>
      )
    }
  },
})

export default SettingRuleRow
