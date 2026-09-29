import { defineComponent, type PropType } from 'vue'
import type { HuifuConfig } from '../paymentChannelRules'
import { ExternalLink } from 'lucide-vue-next'
import { ProviderSection, useConfigFields } from './configFields'

const HUIFU_APPLICATION_URL = 'https://paas.huifu.com/'

/** Huifu hosted Alipay/WeChat H5 and PC payment configuration. */
export const HuifuConfigForm = defineComponent({
  name: 'HuifuConfigForm',
  props: {
    config: { type: Object as PropType<HuifuConfig>, required: true },
  },
  setup(props) {
    const { m, text } = useConfigFields()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="huifuSection" hint="huifuHint">
          {text(c, 'api_base_url', 'huifuApiBaseUrl', { wide: true })}
          {text(c, 'sys_id', 'huifuSysId', { required: true })}
          {text(c, 'product_id', 'huifuProductId', { required: true })}
          {text(c, 'huifu_id', 'huifuMerchantId', { required: true })}
          {text(c, 'skill_source', 'huifuSkillSource')}
          {text(c, 'project_id', 'huifuProjectId', { required: true })}
          {text(c, 'project_title', 'huifuProjectTitle')}
          {text(c, 'merchant_private_key', 'huifuMerchantPrivateKey', { wide: true, rows: 5, required: true })}
          {text(c, 'huifu_public_key', 'huifuPublicKey', { wide: true, rows: 5, required: true })}
          {text(c, 'notify_url', 'huifuNotifyUrl', { wide: true, required: true })}
          {text(c, 'return_url', 'huifuReturnUrl', { wide: true, required: true })}
          <div class="md:col-span-2 rounded-zs border border-line bg-surface p-3 text-xs leading-relaxed text-muted">
            <p>{m('huifuApplicationHint')}</p>
            <a
              class="mt-2 inline-flex items-center gap-1 font-medium text-primary hover:underline"
              href={HUIFU_APPLICATION_URL}
              target="_blank"
              rel="noopener noreferrer"
            >
              {m('huifuApplicationLink')}
              <ExternalLink class="h-3.5 w-3.5" />
            </a>
          </div>
        </ProviderSection>
      )
    }
  },
})
