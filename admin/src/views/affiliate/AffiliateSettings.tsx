import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Save } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Loader, PageHeader, Switch, Textarea } from '@/components/ui'
import { useAffiliateSettings } from './useAffiliateSettings'

export default defineComponent({
  name: 'AffiliateSettingsView',
  setup() {
    const { t } = useI18n()
    const s = useAffiliateSettings()
    onMounted(() => void s.fetchSettings())

    return () => (
      <div class="space-y-6">
        <PageHeader title={t('admin.settings.affiliate.title')} subtitle={t('admin.settings.affiliate.subtitle')} />
        <Card padded>
          {{
            default: () =>
              s.loading.value ? (
                <Loader />
              ) : (
                <div class="space-y-6">
                  <div class="rounded-zs border border-line bg-surface-muted px-4 py-3">
                    <Switch v-model={s.form.enabled} label={t('admin.settings.affiliate.enabled')} />
                  </div>
                  <div class="grid grid-cols-1 gap-6 md:grid-cols-3">
                    <FormField label={t('admin.settings.affiliate.commissionRate')} hint={t('admin.settings.affiliate.commissionRateHint')}>
                      <Input type="number" v-model={s.form.commission_rate} min={0} max={100} step="0.01" />
                    </FormField>
                    <FormField label={t('admin.settings.affiliate.confirmDays')} hint={t('admin.settings.affiliate.confirmDaysHint')}>
                      <Input type="number" v-model={s.form.confirm_days} min={0} max={3650} step={1} />
                    </FormField>
                    <FormField label={t('admin.settings.affiliate.minWithdrawAmount')} hint={t('admin.settings.affiliate.minWithdrawAmountHint')}>
                      <Input type="number" v-model={s.form.min_withdraw_amount} min={0} step="0.01" />
                    </FormField>
                  </div>
                  <FormField label={t('admin.settings.affiliate.withdrawChannels')} hint={t('admin.settings.affiliate.withdrawChannelsHint')}>
                    <Textarea v-model={s.form.withdraw_channels_text} rows={5} placeholder={t('admin.settings.affiliate.withdrawChannelsPlaceholder')} />
                  </FormField>
                </div>
              ),
            footer: () => (
              <div class="flex justify-end">
                <Button variant="primary" loading={s.saving.value} disabled={s.loading.value} onClick={s.save}>
                  <Save class="h-4 w-4" />
                  {t('admin.settings.actions.save')}
                </Button>
              </div>
            ),
          }}
        </Card>
      </div>
    )
  },
})
