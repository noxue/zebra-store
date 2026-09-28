import { defineComponent, type PropType } from 'vue'
import type { OkpayConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** OKPAY config (coin is derived from the channel type on save). */
export const OkpayConfigForm = defineComponent({
  name: 'OkpayConfigForm',
  props: { config: { type: Object as PropType<OkpayConfig>, required: true } },
  setup(props) {
    const { text } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="okpaySection" hint="okpayHint">
          {text(c, 'gateway_url', 'okpayGatewayUrl', { wide: true })}
          {text(c, 'merchant_id', 'okpayMerchantId')}
          {text(c, 'merchant_token', 'okpayMerchantToken')}
          {text(c, 'exchange_rate', 'okpayExchangeRate', { type: 'number', step: '0.00000001', min: '0.00000001' })}
          {text(c, 'callback_url', 'okpayCallbackUrl')}
          {text(c, 'return_url', 'okpayReturnUrl')}
          {text(c, 'display_name', 'okpayDisplayName', { wide: true })}
        </ProviderSection>
      )
    }
  },
})
