import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle } from 'lucide-vue-next'
import { Card, FormField, Input, Switch } from '@/components/ui'
import type { UpstreamSyncSettingsModel } from './useUpstreamSyncSettings'

export default defineComponent({
  name: 'SettingsUpstreamSyncTab',
  props: { model: { type: Object as PropType<UpstreamSyncSettingsModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.upstreamSync.${s}`)
    return () => {
      const f = props.model.form
      return (
        <div class="space-y-6">
          <div class="zs-glass rounded-zs-lg p-5 shadow-zs">
            <h3 class="zs-display text-base text-fg">{k('title')}</h3>
            <p class="mt-1 text-xs text-muted">{k('subtitle')}</p>
          </div>
          <Card title={k('interval.title')} description={k('interval.subtitle')}>
            <div class="space-y-4">
              <FormField label={k('interval.minutesLabel')} hint={k('interval.minutesHint')}>
                <Input type="number" min={5} max={1440} v-model={f.interval_minutes} />
              </FormField>
              <div class="flex items-start gap-2 rounded-zs-sm border border-line bg-warning-soft px-3 py-2 text-xs text-warning-text">
                <AlertTriangle class="mt-0.5 h-3.5 w-3.5 shrink-0" />
                {k('interval.restartHint')}
              </div>
            </div>
          </Card>
          <Card title={k('preOrderCheck.title')} description={k('preOrderCheck.subtitle')}>
            <div class="space-y-2">
              <Switch v-model={f.pre_order_stock_check_enabled} label={k('preOrderCheck.enabled')} />
              <p class="text-xs text-muted">{k('preOrderCheck.enabledHint')}</p>
            </div>
          </Card>
          <Card title={k('pageSize.title')} description={k('pageSize.subtitle')}>
            <FormField label={k('pageSize.label')} hint={k('pageSize.hint')}>
              <Input type="number" min={10} max={200} v-model={f.sync_page_size} />
            </FormField>
          </Card>
          <Card title={k('maxPages.title')} description={k('maxPages.subtitle')}>
            <FormField label={k('maxPages.label')} hint={k('maxPages.hint')}>
              <Input type="number" min={10} max={500} v-model={f.sync_max_pages} />
            </FormField>
          </Card>
          <Card title={k('concurrency.title')} description={k('concurrency.subtitle')}>
            <FormField label={k('concurrency.label')} hint={k('concurrency.hint')}>
              <Input type="number" min={1} max={10} v-model={f.sync_conn_concurrency} />
            </FormField>
          </Card>
        </div>
      )
    }
  },
})
