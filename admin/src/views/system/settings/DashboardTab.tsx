import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Card, FormField, Input } from '@/components/ui'
import type { DashboardSettingsModel } from './useDashboardSettings'
import { SubSection, ToggleRow } from './SettingsUi'

export default defineComponent({
  name: 'SettingsDashboardTab',
  props: { model: { type: Object as PropType<DashboardSettingsModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.dashboard.${s}`)
    return () => {
      const f = props.model.form
      return (
        <Card title={k('title')} description={k('subtitle')}>
          <div class="space-y-5">
            <SubSection title={k('accounting.title')}>
              <ToggleRow
                v-model={f.accounting.refund_reverses_cost}
                label={k('accounting.refundReversesCost')}
                description={k('accounting.refundReversesCostHint')}
                trailing
              />
            </SubSection>
            <SubSection title={k('alert.title')} bodyClass="grid grid-cols-1 gap-4 md:grid-cols-2">
              <FormField label={k('alert.lowStockThreshold')} hint={k('alert.lowStockThresholdHint')}>
                <Input type="number" min={1} max={500} v-model={f.alert.low_stock_threshold} />
              </FormField>
              <FormField label={k('alert.outOfStockThreshold')} hint={k('alert.outOfStockThresholdHint')}>
                <Input type="number" min={1} max={10000} v-model={f.alert.out_of_stock_products_threshold} />
              </FormField>
              <FormField label={k('alert.pendingOrderThreshold')} hint={k('alert.pendingOrderThresholdHint')}>
                <Input type="number" min={1} max={100000} v-model={f.alert.pending_payment_orders_threshold} />
              </FormField>
              <FormField label={k('alert.paymentFailedThreshold')} hint={k('alert.paymentFailedThresholdHint')}>
                <Input type="number" min={1} max={100000} v-model={f.alert.payments_failed_threshold} />
              </FormField>
            </SubSection>
            <SubSection title={k('ranking.title')} bodyClass="grid grid-cols-1 gap-4 md:grid-cols-2">
              <FormField label={k('ranking.topProductsLimit')} hint={k('ranking.topProductsLimitHint')}>
                <Input type="number" min={1} max={20} v-model={f.ranking.top_products_limit} />
              </FormField>
              <FormField label={k('ranking.topChannelsLimit')} hint={k('ranking.topChannelsLimitHint')}>
                <Input type="number" min={1} max={20} v-model={f.ranking.top_channels_limit} />
              </FormField>
            </SubSection>
          </div>
        </Card>
      )
    }
  },
})
