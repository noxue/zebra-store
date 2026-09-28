import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Card, FormField, Input, Select, Switch } from '@/components/ui'
import type { CaptchaSettingsModel } from './useCaptchaSettings'
import { SubSection } from './SettingsUi'

const SCENE_LABELS = [
  ['login', 'login'],
  ['register_send_code', 'registerSendCode'],
  ['reset_send_code', 'resetSendCode'],
  ['guest_create_order', 'guestCreateOrder'],
  ['gift_card_redeem', 'giftCardRedeem'],
] as const

export default defineComponent({
  name: 'SettingsCaptchaTab',
  props: { model: { type: Object as PropType<CaptchaSettingsModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.captcha.${s}`)
    return () => {
      const f = props.model.form
      return (
        <Card title={k('title')} description={k('subtitle')}>
          <div class="space-y-6">
            <FormField label={k('provider')}>
              <Select
                v-model={f.provider}
                options={[
                  { label: k('providerNone'), value: 'none' },
                  { label: k('providerImage'), value: 'image' },
                  { label: k('providerTurnstile'), value: 'turnstile' },
                ]}
              />
            </FormField>
            <SubSection title={k('scenesTitle')} bodyClass="grid grid-cols-1 gap-3 md:grid-cols-2">
              {SCENE_LABELS.map(([key, label]) => (
                <Switch key={key} v-model={f.scenes[key]} label={k(`scenes.${label}`)} />
              ))}
            </SubSection>
            {f.provider === 'image' && (
              <SubSection title={k('image.title')} bodyClass="grid grid-cols-1 gap-4 md:grid-cols-4">
                <FormField label={k('image.length')}>
                  <Input type="number" min={4} max={8} v-model={f.image.length} />
                </FormField>
                <FormField label={k('image.width')}>
                  <Input type="number" min={100} v-model={f.image.width} />
                </FormField>
                <FormField label={k('image.height')}>
                  <Input type="number" min={40} v-model={f.image.height} />
                </FormField>
                <FormField label={k('image.expireSeconds')}>
                  <Input type="number" min={30} max={3600} v-model={f.image.expire_seconds} />
                </FormField>
                <FormField label={k('image.noiseCount')}>
                  <Input type="number" min={0} v-model={f.image.noise_count} />
                </FormField>
                <FormField label={k('image.showLine')}>
                  <Input type="number" min={0} v-model={f.image.show_line} />
                </FormField>
                <FormField label={k('image.maxStore')}>
                  <Input type="number" min={100} v-model={f.image.max_store} />
                </FormField>
              </SubSection>
            )}
            {f.provider === 'turnstile' && (
              <SubSection title={k('turnstile.title')} bodyClass="grid grid-cols-1 gap-4 md:grid-cols-2">
                <div class="md:col-span-2">
                  <FormField label={k('turnstile.siteKey')}>
                    <Input v-model={f.turnstile.site_key} />
                  </FormField>
                </div>
                <div class="md:col-span-2">
                  <FormField label={k('turnstile.secretKey')} hint={f.turnstile.has_secret ? k('turnstile.secretHintKeep') : k('turnstile.secretHintEmpty')}>
                    <Input type="password" v-model={f.turnstile.secret_key} placeholder={k('turnstile.secretKeyPlaceholder')} autocomplete="new-password" />
                  </FormField>
                </div>
                <div class="md:col-span-2">
                  <FormField label={k('turnstile.verifyURL')}>
                    <Input v-model={f.turnstile.verify_url} />
                  </FormField>
                </div>
                <FormField label={k('turnstile.timeoutMS')}>
                  <Input type="number" min={500} max={10000} v-model={f.turnstile.timeout_ms} />
                </FormField>
              </SubSection>
            )}
          </div>
        </Card>
      )
    }
  },
})
