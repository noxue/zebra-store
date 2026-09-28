import { defineComponent, type PropType } from 'vue'
import type { StripeConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** Official Stripe config. */
export const StripeConfigForm = defineComponent({
  name: 'StripeConfigForm',
  props: { config: { type: Object as PropType<StripeConfig>, required: true } },
  setup(props) {
    const { text } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="stripeSection" hint="stripeHint">
          {text(c, 'secret_key', 'stripeSecretKey', { wide: true })}
          {text(c, 'publishable_key', 'stripePublishableKey')}
          {text(c, 'webhook_secret', 'stripeWebhookSecret')}
          {text(c, 'success_url', 'stripeSuccessUrl')}
          {text(c, 'cancel_url', 'stripeCancelUrl')}
          {text(c, 'api_base_url', 'stripeApiBaseUrl')}
          {text(c, 'payment_method_types', 'stripePaymentMethodTypes')}
          {text(c, 'target_currency', 'targetCurrency')}
          {text(c, 'exchange_rate', 'exchangeRate')}
        </ProviderSection>
      )
    }
  },
})
