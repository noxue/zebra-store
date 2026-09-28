import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Send } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Switch } from '@/components/ui'
import type { SmtpSettingsModel } from './useSmtpSettings'
import { SubSection, ToggleRow } from './SettingsUi'

export default defineComponent({
  name: 'SettingsSmtpTab',
  props: { model: { type: Object as PropType<SmtpSettingsModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.smtp.${s}`)
    return () => {
      const m = props.model
      const f = m.form
      return (
        <Card title={k('title')} description={k('subtitle')}>
          <div class="space-y-6">
            <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
              <ToggleRow v-model={f.enabled} label={k('enabled')} />
              <div class="flex flex-wrap items-center gap-x-6 gap-y-2 rounded-zs border border-line bg-surface-muted/60 px-4 py-3">
                <Switch v-model={f.use_tls} label={k('useTLS')} />
                <Switch v-model={f.use_ssl} label={k('useSSL')} />
              </div>
              <ToggleRow
                v-model={f.order_notification_enabled}
                disabled={!f.enabled}
                label={k('orderNotificationEnabled')}
                description={k('orderNotificationHint')}
              />
            </div>
            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={k('host')}>
                <Input v-model={f.host} placeholder={k('hostPlaceholder')} />
              </FormField>
              <FormField label={k('port')}>
                <Input type="number" v-model={f.port} placeholder={k('portPlaceholder')} />
              </FormField>
              <FormField label={k('username')}>
                <Input v-model={f.username} placeholder={k('usernamePlaceholder')} autocomplete="off" />
              </FormField>
              <FormField label={k('password')} hint={f.has_password ? k('passwordHintKeep') : k('passwordHintEmpty')}>
                <Input type="password" v-model={f.password} placeholder={k('passwordPlaceholder')} autocomplete="new-password" />
              </FormField>
              <FormField label={k('from')}>
                <Input v-model={f.from} placeholder={k('fromPlaceholder')} />
              </FormField>
              <FormField label={k('fromName')}>
                <Input v-model={f.from_name} placeholder={k('fromNamePlaceholder')} />
              </FormField>
            </div>
            <SubSection title={k('verifyCode.title')} bodyClass="grid grid-cols-1 gap-4 md:grid-cols-4">
              <FormField label={k('verifyCode.expireMinutes')}>
                <Input type="number" min={1} v-model={f.verify_code.expire_minutes} />
              </FormField>
              <FormField label={k('verifyCode.sendIntervalSeconds')}>
                <Input type="number" min={1} v-model={f.verify_code.send_interval_seconds} />
              </FormField>
              <FormField label={k('verifyCode.maxAttempts')}>
                <Input type="number" min={1} v-model={f.verify_code.max_attempts} />
              </FormField>
              <FormField label={k('verifyCode.length')}>
                <Input type="number" min={4} max={10} v-model={f.verify_code.length} />
              </FormField>
            </SubSection>
            <div class="rounded-zs border border-line bg-accent-soft/40 p-4">
              <h4 class="text-sm font-semibold text-fg">{k('testTitle')}</h4>
              <p class="mt-1 text-xs text-muted">{k('testSubtitle')}</p>
              <div class="mt-3 flex flex-col gap-3 md:flex-row">
                <Input v-model={f.test_email} placeholder={k('testEmailPlaceholder')} onEnter={m.sendTest} />
                <Button variant="soft" loading={m.testing.value} disabled={m.testing.value} onClick={m.sendTest}>
                  <Send class="h-3.5 w-3.5" />
                  {m.testing.value ? k('testing') : k('testButton')}
                </Button>
              </div>
            </div>
          </div>
        </Card>
      )
    }
  },
})
