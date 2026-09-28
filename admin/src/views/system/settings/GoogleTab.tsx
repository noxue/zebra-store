import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Info } from 'lucide-vue-next'
import { Card, FormField, Input } from '@/components/ui'
import type { GoogleAuthSettingsModel } from './useGoogleAuthSettings'
import { ToggleRow } from './SettingsUi'

export default defineComponent({
  name: 'SettingsGoogleTab',
  props: { model: { type: Object as PropType<GoogleAuthSettingsModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.google.${s}`)
    return () => {
      const f = props.model.form
      return (
        <Card title={k('title')} description={k('subtitle')}>
          <div class="space-y-6">
            <ToggleRow v-model={f.enabled} label={k('enabled')} />
            <FormField label={k('clientID')} hint={k('clientIDHint')}>
              <Input v-model={f.client_id} placeholder={k('clientIDPlaceholder')} />
            </FormField>
            <div class="flex gap-3 rounded-zs border border-line bg-accent-soft/60 px-4 py-3 text-xs leading-5 text-info-text">
              <Info class="mt-0.5 h-4 w-4 shrink-0" />
              <div>
                <p>{k('credentialHint')}</p>
                <p class="mt-1">{k('originHint')}</p>
                <p class="mt-1">{k('redirectHint')}</p>
              </div>
            </div>
          </div>
        </Card>
      )
    }
  },
})
