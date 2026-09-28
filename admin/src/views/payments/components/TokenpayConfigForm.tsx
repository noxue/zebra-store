import { defineComponent, type PropType } from 'vue'
import type { TokenpayConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** TokenPay config. */
export const TokenpayConfigForm = defineComponent({
  name: 'TokenpayConfigForm',
  props: { config: { type: Object as PropType<TokenpayConfig>, required: true } },
  setup(props) {
    const { text } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="tokenpaySection" hint="tokenpayHint">
          {text(c, 'gateway_url', 'tokenpayGatewayUrl', { wide: true })}
          {text(c, 'notify_secret', 'tokenpayNotifySecret', { wide: true })}
          {text(c, 'currency', 'tokenpayCurrency')}
          {text(c, 'base_currency', 'tokenpayBaseCurrency')}
          {text(c, 'notify_url', 'tokenpayNotifyUrl')}
          {text(c, 'redirect_url', 'tokenpayRedirectUrl')}
        </ProviderSection>
      )
    }
  },
})
