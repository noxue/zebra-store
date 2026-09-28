import { defineComponent, type PropType } from 'vue'
import type { EpayConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** 易支付 config (v1 merchant key / v2 RSA keys). */
export const EpayConfigForm = defineComponent({
  name: 'EpayConfigForm',
  props: { config: { type: Object as PropType<EpayConfig>, required: true } },
  setup(props) {
    const { text, select } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="epaySection" hint="epayHint">
          {select(c, 'epay_version', 'epayVersion', [
            { value: 'v1', label: 'v1', literal: true },
            { value: 'v2', label: 'v2', literal: true },
          ])}
          {text(c, 'gateway_url', 'gatewayUrl')}
          {text(c, 'merchant_id', 'merchantId')}
          {c.epay_version === 'v1' ? text(c, 'merchant_key', 'merchantKey') : text(c, 'private_key', 'privateKey', { wide: true, rows: 4 })}
          {c.epay_version === 'v2' && text(c, 'platform_public_key', 'platformPublicKey', { wide: true, rows: 4 })}
          {text(c, 'notify_url', 'notifyUrl')}
          {text(c, 'return_url', 'returnUrl')}
          {text(c, 'target_currency', 'targetCurrency')}
          {text(c, 'exchange_rate', 'exchangeRate')}
        </ProviderSection>
      )
    }
  },
})
