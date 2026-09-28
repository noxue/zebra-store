import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Badge, Card, FormField, Input } from '@/components/ui'
import type { TelegramAuthSettingsModel } from './useTelegramAuthSettings'
import { ToggleRow } from './SettingsUi'

export default defineComponent({
  name: 'SettingsTelegramTab',
  props: { model: { type: Object as PropType<TelegramAuthSettingsModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.telegram.${s}`)
    return () => {
      const f = props.model.form
      const modeLabel = f.mode === 'oidc' ? k('modeOidc') : f.mode === 'widget' ? k('modeWidget') : k('modeDisabled')
      return (
        <Card title={k('title')} description={k('subtitle')}>
          <div class="space-y-6">
            <ToggleRow v-model={f.enabled} label={k('enabled')} />
            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={k('botUsername')}>
                <Input v-model={f.bot_username} placeholder={k('botUsernamePlaceholder')} />
              </FormField>
              <FormField label={k('botToken')} hint={f.has_bot_token ? k('botTokenHintKeep') : k('botTokenHintEmpty')}>
                <Input type="password" v-model={f.bot_token} placeholder={k('botTokenPlaceholder')} autocomplete="new-password" />
              </FormField>
              <div class="md:col-span-2">
                <FormField label={k('modeLabel')}>
                  <Badge tone={f.mode === 'oidc' || f.mode === 'widget' ? 'success' : 'neutral'} size="md">
                    {modeLabel}
                  </Badge>
                </FormField>
              </div>
              <FormField label={k('clientSecret')} hint={f.has_client_secret ? k('clientSecretHintKeep') : k('clientSecretHintEmpty')}>
                <Input type="password" v-model={f.client_secret} placeholder={k('clientSecretPlaceholder')} autocomplete="new-password" />
              </FormField>
              <FormField label={k('oidcRedirectURI')} hint={k('oidcRedirectURIHint')}>
                <Input v-model={f.oidc_redirect_uri} placeholder={k('oidcRedirectURIPlaceholder')} />
              </FormField>
              <div class="md:col-span-2">
                <FormField label={k('miniAppURL')} hint={k('miniAppURLHint')}>
                  <Input v-model={f.mini_app_url} placeholder={k('miniAppURLPlaceholder')} />
                </FormField>
              </div>
              <FormField label={k('loginExpireSeconds')}>
                <Input type="number" min={30} max={86400} v-model={f.login_expire_seconds} />
              </FormField>
              <FormField label={k('replayTTLSeconds')}>
                <Input type="number" min={60} max={86400} v-model={f.replay_ttl_seconds} />
              </FormField>
            </div>
          </div>
        </Card>
      )
    }
  },
})
