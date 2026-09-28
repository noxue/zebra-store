import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { AlertTriangle, Save } from 'lucide-vue-next'
import { Button, Card, FormField, Input, PageHeader } from '@/components/ui'
import { DEFAULT_CALLBACK_ROUTE_PATHS, type CallbackRouteKey } from '@/utils/callbackRoutes'
import { useCallbackRoutes } from './useCallbackRoutes'

const FIELDS: { key: CallbackRouteKey; label: string; placeholder: string }[] = [
  { key: 'payment_callback', label: 'paymentCallback', placeholder: 'paymentCallbackPlaceholder' },
  { key: 'dujiaopay_webhook', label: 'dujiaopayWebhook', placeholder: 'webhookPlaceholder' },
  { key: 'paypal_webhook', label: 'paypalWebhook', placeholder: 'webhookPlaceholder' },
  { key: 'stripe_webhook', label: 'stripeWebhook', placeholder: 'webhookPlaceholder' },
  { key: 'upstream_callback', label: 'upstreamCallback', placeholder: 'callbackPlaceholder' },
]

export default defineComponent({
  name: 'CallbackRoutesView',
  setup() {
    const { t } = useI18n()
    const p = useCallbackRoutes()
    onMounted(() => void p.load())
    const k = (suffix: string) => t(`admin.settings.callbackRoutes.${suffix}`)

    return () => (
      <div class="space-y-6">
        <PageHeader title={k('title')} subtitle={k('subtitle')} />
        <Card>
          <div class="space-y-6">
            <div class="flex items-start gap-2 rounded-zs border border-line bg-warning-soft p-4 text-xs leading-relaxed text-warning-text">
              <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0" />
              <p>{k('warning')}</p>
            </div>
            <div class="grid grid-cols-1 gap-6">
              {FIELDS.map((f) => (
                <FormField key={f.key} label={k(f.label)} hint={`${k('defaultPath')}: ${DEFAULT_CALLBACK_ROUTE_PATHS[f.key]}`}>
                  <Input
                    mono
                    disabled={p.loading.value}
                    modelValue={p.displayValue(f.key)}
                    onUpdate:modelValue={(v: string | number) => p.setValue(f.key, v)}
                    placeholder={k(f.placeholder)}
                  />
                </FormField>
              ))}
            </div>
            <div class="flex justify-end border-t border-line pt-4">
              <Button variant="primary" loading={p.saving.value} onClick={p.save}>
                <Save class="h-4 w-4" />
                {p.saving.value ? t('admin.settings.actions.saving') : t('admin.settings.actions.save')}
              </Button>
            </div>
          </div>
        </Card>
      </div>
    )
  },
})
