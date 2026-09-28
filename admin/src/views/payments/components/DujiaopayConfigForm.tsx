import { defineComponent, type PropType } from 'vue'
import { FormField, Input } from '@/components/ui'
import type { DujiaopayConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/**
 * DujiaoPay config. In transaction mode the token_id is typed in by hand and stored as the
 * channel_type (v-model:channelType); cashier mode offers an allowed-methods whitelist instead.
 */
export const DujiaopayConfigForm = defineComponent({
  name: 'DujiaopayConfigForm',
  props: {
    config: { type: Object as PropType<DujiaopayConfig>, required: true },
    channelType: { type: String, default: '' },
  },
  emits: { 'update:channelType': (_v: string) => true },
  setup(props, { emit }) {
    const { m, text, select } = useConfigFields()
    return () => {
      const c = props.config
      const cashier = c.order_mode === 'cashier'
      return (
        <ProviderSection title="dujiaopaySection" hint="dujiaopayHint">
          {select(c, 'order_mode', 'dujiaopayOrderMode', [
            { value: 'transaction', label: 'dujiaopayOrderModeTransaction' },
            { value: 'cashier', label: 'dujiaopayOrderModeCashier' },
          ])}
          {!cashier && (
            <FormField label={m('dujiaopayFixedMethod')}>
              <Input
                modelValue={props.channelType}
                onUpdate:modelValue={(v: string | number) => emit('update:channelType', String(v))}
                placeholder={m('dujiaopayFixedMethodPlaceholder')}
              />
            </FormField>
          )}
          {cashier && text(c, 'allowed_methods', 'dujiaopayAllowedMethods', { wide: true })}
          {text(c, 'api_base_url', 'dujiaopayApiBaseUrl', { wide: true })}
          {text(c, 'api_key_id', 'dujiaopayApiKeyId')}
          {text(c, 'fiat_currency', 'dujiaopayFiatCurrency')}
          {text(c, 'api_secret', 'dujiaopayApiSecret', { wide: true, rows: 3 })}
          {text(c, 'webhook_secret', 'dujiaopayWebhookSecret', { wide: true, rows: 3 })}
          {text(c, 'success_url', 'dujiaopaySuccessUrl')}
          {text(c, 'cancel_url', 'dujiaopayCancelUrl')}
        </ProviderSection>
      )
    }
  },
})
