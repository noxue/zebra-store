import { defineComponent, type PropType } from 'vue'
import type { PaypalConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** Official PayPal config. */
export const PaypalConfigForm = defineComponent({
  name: 'PaypalConfigForm',
  props: { config: { type: Object as PropType<PaypalConfig>, required: true } },
  setup(props) {
    const { text } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="paypalSection" hint="paypalHint">
          {text(c, 'client_id', 'paypalClientId')}
          {text(c, 'client_secret', 'paypalClientSecret')}
          {text(c, 'base_url', 'paypalBaseUrl')}
          {text(c, 'return_url', 'returnUrl')}
          {text(c, 'cancel_url', 'paypalCancelUrl')}
          {text(c, 'webhook_id', 'paypalWebhookId')}
          {text(c, 'brand_name', 'paypalBrandName')}
          {text(c, 'locale', 'paypalLocale')}
          {text(c, 'target_currency', 'paypalTargetCurrency')}
          {text(c, 'exchange_rate', 'paypalExchangeRate')}
        </ProviderSection>
      )
    }
  },
})
