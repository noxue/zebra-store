import { defineComponent, type PropType } from 'vue'
import type { AlipayConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** Official Alipay config. */
export const AlipayConfigForm = defineComponent({
  name: 'AlipayConfigForm',
  props: { config: { type: Object as PropType<AlipayConfig>, required: true } },
  setup(props) {
    const { text } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="alipaySection" hint="alipayHint">
          {text(c, 'app_id', 'alipayAppId')}
          {text(c, 'sign_type', 'alipaySignType')}
          {text(c, 'private_key', 'alipayPrivateKey', { wide: true, rows: 4 })}
          {text(c, 'alipay_public_key', 'alipayPublicKey', { wide: true, rows: 4 })}
          {text(c, 'gateway_url', 'alipayGatewayUrl')}
          {text(c, 'notify_url', 'alipayNotifyUrl')}
          {text(c, 'return_url', 'alipayReturnUrl')}
          {text(c, 'app_cert_sn', 'alipayAppCertSn')}
          {text(c, 'alipay_root_cert_sn', 'alipayRootCertSn', { wide: true })}
          {text(c, 'target_currency', 'targetCurrency')}
          {text(c, 'exchange_rate', 'exchangeRate')}
        </ProviderSection>
      )
    }
  },
})
