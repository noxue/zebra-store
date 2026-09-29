import { defineComponent, type PropType } from 'vue'
import type { HuifuConfig } from '../paymentChannelRules'
import { ProviderSection, useConfigFields } from './configFields'

/** Huifu hosted Alipay/WeChat H5 and PC payment configuration. */
export const HuifuConfigForm = defineComponent({
  name: 'HuifuConfigForm',
  props: {
    config: { type: Object as PropType<HuifuConfig>, required: true },
  },
  setup(props) {
    const { text } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="huifuSection" hint="huifuHint">
          {text(c, 'api_base_url', 'huifuApiBaseUrl', { wide: true })}
          {text(c, 'sys_id', 'huifuSysId')}
          {text(c, 'product_id', 'huifuProductId')}
          {text(c, 'huifu_id', 'huifuMerchantId')}
          {text(c, 'skill_source', 'huifuSkillSource')}
          {text(c, 'project_id', 'huifuProjectId')}
          {text(c, 'project_title', 'huifuProjectTitle')}
          {text(c, 'merchant_private_key', 'huifuMerchantPrivateKey', { wide: true, rows: 5 })}
          {text(c, 'huifu_public_key', 'huifuPublicKey', { wide: true, rows: 5 })}
          {text(c, 'notify_url', 'huifuNotifyUrl', { wide: true })}
          {text(c, 'return_url', 'huifuReturnUrl', { wide: true })}
        </ProviderSection>
      )
    }
  },
})
