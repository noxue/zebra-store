import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { FormField, MultiSelect, Select } from '@/components/ui'
import type { HuifuConfig } from '../paymentChannelRules'
import { ExternalLink } from 'lucide-vue-next'
import { ProviderSection, useConfigFields } from './configFields'

const HUIFU_APPLICATION_GUIDE_URL = 'https://zebra-store-docs.noxue.com/payment/huifu#个人申请资料'

/** Huifu direct Alipay QR or hosted Alipay/WeChat configuration. */
export const HuifuConfigForm = defineComponent({
  name: 'HuifuConfigForm',
  props: {
    config: { type: Object as PropType<HuifuConfig>, required: true },
    onCollectionModeChange: { type: Function as PropType<(mode: string) => void>, required: false },
  },
  setup(props) {
    const { m, text } = useConfigFields()
    const { t } = useI18n()
    return () => {
      const c = props.config
      return (
        <ProviderSection title="huifuSection" hint="huifuHint">
          <FormField label={m('huifuCollectionMode')} required>
            <Select
              modelValue={c.collection_mode}
              options={[
                { value: 'direct_qr', label: m('huifuDirectQr') },
                { value: 'hosted', label: m('huifuHosted') },
              ]}
              onUpdate:modelValue={(value: string | number) => {
                c.collection_mode = String(value)
                if (c.collection_mode === 'direct_qr') c.supported_channel_types = ['alipay']
                props.onCollectionModeChange?.(c.collection_mode)
              }}
            />
          </FormField>
          <FormField label={m('huifuMethods')} required>
            <MultiSelect
              modelValue={c.supported_channel_types}
              options={c.collection_mode === 'direct_qr' ? [
                { value: 'alipay', label: t('admin.paymentChannels.channelTypes.alipay') },
              ] : [
                { value: 'wechat', label: t('admin.paymentChannels.channelTypes.wechat') },
                { value: 'alipay', label: t('admin.paymentChannels.channelTypes.alipay') },
              ]}
              onUpdate:modelValue={(values: (string | number)[]) => {
                if (values.length > 0) c.supported_channel_types = values.map(String)
              }}
            />
          </FormField>
          {text(c, 'api_base_url', 'huifuApiBaseUrl', { wide: true })}
          {text(c, 'sys_id', 'huifuSysId', { required: true })}
          {text(c, 'product_id', 'huifuProductId', { required: true })}
          {text(c, 'huifu_id', 'huifuMerchantId', { required: true })}
          {c.collection_mode === 'hosted' && text(c, 'project_id', 'huifuProjectId', { required: true, hint: 'huifuProjectIdHint' })}
          {c.collection_mode === 'hosted' && text(c, 'project_title', 'huifuProjectTitle')}
          {text(c, 'merchant_private_key', 'huifuMerchantPrivateKey', { wide: true, rows: 5, required: true })}
          {text(c, 'huifu_public_key', 'huifuPublicKey', { wide: true, rows: 5, required: true })}
          {text(c, 'notify_url', 'huifuNotifyUrl', { wide: true, required: true })}
          {c.collection_mode === 'hosted' && text(c, 'return_url', 'huifuReturnUrl', { wide: true, required: true })}
          <div class="md:col-span-2 rounded-zs border border-line bg-surface p-3 text-xs leading-relaxed text-muted">
            {c.supported_channel_types.includes('wechat') && (
              <p class="mb-2">{m('huifuWechatConfigHint')}</p>
            )}
            <p>{m('huifuApplicationHint')}</p>
            <a
              class="mt-2 inline-flex items-center gap-1 font-medium text-primary hover:underline"
              href={HUIFU_APPLICATION_GUIDE_URL}
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
