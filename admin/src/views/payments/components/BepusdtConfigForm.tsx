import { defineComponent, type PropType } from 'vue'
import type { BepusdtConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** BEpusdt config: transaction mode uses trade_type, cashier mode an optional currencies whitelist. */
export const BepusdtConfigForm = defineComponent({
  name: 'BepusdtConfigForm',
  props: { config: { type: Object as PropType<BepusdtConfig>, required: true } },
  setup(props) {
    const { text, select } = useConfigFields()
    return () => {
      const c = props.config
      const cashier = c.order_mode === 'cashier'
      return (
        <ProviderSection title="bepusdtSection" hint="bepusdtHint">
          {text(c, 'gateway_url', 'bepusdtGatewayUrl', { wide: true })}
          {text(c, 'auth_token', 'bepusdtAuthToken', { wide: true })}
          {select(c, 'order_mode', 'bepusdtOrderMode', [
            { value: 'transaction', label: 'bepusdtOrderModeTransaction' },
            { value: 'cashier', label: 'bepusdtOrderModeCashier' },
          ])}
          {!cashier && text(c, 'trade_type', 'bepusdtTradeType')}
          {cashier && text(c, 'currencies', 'bepusdtCurrencies', { wide: true })}
          {text(c, 'fiat', 'bepusdtFiat')}
          {text(c, 'notify_url', 'bepusdtNotifyUrl')}
          {text(c, 'return_url', 'bepusdtReturnUrl')}
        </ProviderSection>
      )
    }
  },
})
