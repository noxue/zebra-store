import { defineComponent, type PropType } from 'vue'
import type { EpusdtConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** Epusdt config: token + network are required in transaction mode, hidden for the cashier. */
export const EpusdtConfigForm = defineComponent({
  name: 'EpusdtConfigForm',
  props: { config: { type: Object as PropType<EpusdtConfig>, required: true } },
  setup(props) {
    const { text, select } = useConfigFields()
    return () => {
      const c = props.config
      const cashier = c.order_mode === 'cashier'
      return (
        <ProviderSection title="epusdtSection" hint="epusdtHint">
          {text(c, 'gateway_url', 'epusdtGatewayUrl', { wide: true })}
          {text(c, 'pid', 'epusdtPid')}
          {text(c, 'secret_key', 'epusdtSecretKey')}
          {select(c, 'order_mode', 'epusdtOrderMode', [
            { value: 'transaction', label: 'epusdtOrderModeTransaction' },
            { value: 'cashier', label: 'epusdtOrderModeCashier' },
          ])}
          {!cashier && text(c, 'token', 'epusdtToken', { required: true })}
          {!cashier && text(c, 'network', 'epusdtNetwork', { required: true })}
          {text(c, 'currency', 'epusdtCurrency')}
          {text(c, 'notify_url', 'epusdtNotifyUrl')}
          {text(c, 'return_url', 'epusdtReturnUrl')}
        </ProviderSection>
      )
    }
  },
})
