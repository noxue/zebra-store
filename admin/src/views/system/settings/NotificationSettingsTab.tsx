import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, Search, X } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Select, Switch, Textarea } from '@/components/ui'
import { FEISHU_RECEIVE_ID_TYPES, buildProductLabel, NOTIFICATION_SCENES, NOTIFICATION_SCENE_LABEL_KEYS, type NotificationCenterModel } from './useNotificationCenter'
import { LangTag, SubSection } from './SettingsUi'

/** Port of components/SettingsNotificationTab.vue. */
export default defineComponent({
  name: 'NotificationSettingsTab',
  props: { model: { type: Object as PropType<NotificationCenterModel>, required: true } },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.notification.${s}`)

    return () => {
      const m = props.model
      const f = m.form
      const lang = m.currentLang.value
      return (
        <Card title={k('title')} description={k('subtitle')}>
          <div class="space-y-6">
            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={k('defaultLocale')}>
                <Select
                  v-model={f.default_locale}
                  options={[
                    { label: t('admin.common.lang.zhCN'), value: 'zh-CN' },
                    { label: t('admin.common.lang.zhTW'), value: 'zh-TW' },
                    { label: t('admin.common.lang.enUS'), value: 'en-US' },
                  ]}
                />
              </FormField>
              <FormField label={k('dedupeTTLSeconds')}>
                <Input type="number" min={30} max={86400} v-model={f.dedupe_ttl_seconds} />
              </FormField>
            </div>

            <SubSection title={k('inventory.title')} bodyClass="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={k('inventory.intervalSeconds')} hint={k('inventory.intervalHint')}>
                <Input type="number" min={60} max={604800} v-model={f.inventory_alert_interval_seconds} />
              </FormField>
              <FormField label={k('inventory.ignoredProducts')} hint={k('inventory.ignoredProductIDsHint')}>
                <div class="space-y-3 rounded-zs border border-line bg-surface-strong p-3">
                  <div class="flex flex-col gap-2 sm:flex-row">
                    <Input icon={Search} v-model={m.productKeyword.value} placeholder={k('inventory.searchPlaceholder')} onEnter={m.searchProducts} />
                    <Button loading={m.productOptionsLoading.value} onClick={m.searchProducts}>
                      {m.productOptionsLoading.value ? t('admin.common.loading') : k('inventory.searchAction')}
                    </Button>
                  </div>
                  <div class="flex flex-col gap-2 sm:flex-row">
                    <Select
                      v-model={m.selectedIgnoredProduct.value}
                      placeholder={k('inventory.selectPlaceholder')}
                      options={m.productOptions.value.map((p) => ({
                        label: buildProductLabel(p),
                        value: String(p.id),
                      }))}
                    />
                    <Button variant="soft" onClick={m.addIgnoredProduct}>
                      <Plus class="h-3.5 w-3.5" />
                      {k('inventory.addAction')}
                    </Button>
                  </div>
                  <div class="flex flex-wrap gap-2">
                    {m.ignoredProducts.value.map((p) => (
                      <button
                        key={p.id}
                        type="button"
                        class="inline-flex items-center gap-1.5 rounded-full border border-line bg-primary-soft px-3 py-1 text-xs text-fg hover:border-primary"
                        onClick={() => m.removeIgnoredProduct(p.id)}
                      >
                        {p.label}
                        <X class="h-3 w-3 text-muted" />
                      </button>
                    ))}
                    {m.ignoredProducts.value.length === 0 && <span class="text-xs text-muted">{k('inventory.emptyIgnoredProducts')}</span>}
                  </div>
                </div>
              </FormField>
            </SubSection>

            <SubSection title={k('paymentOrder.title')} bodyClass="grid grid-cols-1 gap-5 md:grid-cols-2">
              <FormField label={k('paymentOrder.checkIntervalSeconds')} hint={k('paymentOrder.checkIntervalHint')}>
                <Input type="number" min={60} max={604800} v-model={f.payment_order_alert_check_interval_seconds} />
              </FormField>
              <FormField label={k('paymentOrder.intervalSeconds')} hint={k('paymentOrder.intervalHint')}>
                <Input type="number" min={60} max={604800} v-model={f.payment_order_alert_interval_seconds} />
              </FormField>
            </SubSection>

            <div class="grid grid-cols-1 gap-5 md:grid-cols-2">
              <SubSection title={k('channels.email.title')} bodyClass="space-y-3">
                <Switch v-model={f.channels.email.enabled} label={k('channels.email.enabled')} />
                <FormField label={k('channels.email.recipients')}>
                  <Textarea v-model={f.channels.email.recipients_text} rows={5} placeholder={k('channels.email.recipientsPlaceholder')} />
                </FormField>
              </SubSection>
              <SubSection title={k('channels.telegram.title')} bodyClass="space-y-3">
                <Switch v-model={f.channels.telegram.enabled} label={k('channels.telegram.enabled')} />
                <FormField label={k('channels.telegram.recipients')}>
                  <Textarea v-model={f.channels.telegram.recipients_text} rows={5} placeholder={k('channels.telegram.recipientsPlaceholder')} />
                </FormField>
              </SubSection>
              <div class="md:col-span-2">
                <SubSection title={k('channels.feishu.title')} description={k('channels.feishu.subtitle')} bodyClass="space-y-4">
                  <Switch v-model={f.channels.feishu.enabled} label={k('channels.feishu.enabled')} />
                  <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
                    <FormField label={k('channels.feishu.appID')}>
                      <Input v-model={f.channels.feishu.app_id} placeholder={k('channels.feishu.appIDPlaceholder')} autocomplete="off" />
                    </FormField>
                    <FormField
                      label={k('channels.feishu.appSecret')}
                      hint={f.channels.feishu.has_app_secret ? k('channels.feishu.appSecretHintKeep') : k('channels.feishu.appSecretHintEmpty')}
                    >
                      <Input type="password" v-model={f.channels.feishu.app_secret} placeholder={k('channels.feishu.appSecretPlaceholder')} autocomplete="new-password" />
                    </FormField>
                    <FormField label={k('channels.feishu.receiveIDType')}>
                      <Select
                        v-model={f.channels.feishu.receive_id_type}
                        options={FEISHU_RECEIVE_ID_TYPES.map(([value, label]) => ({ value, label: k(`channels.feishu.receiveIDTypes.${label}`) }))}
                      />
                    </FormField>
                    <FormField label={k('channels.feishu.recipients')}>
                      <Textarea v-model={f.channels.feishu.recipients_text} rows={4} placeholder={k('channels.feishu.recipientsPlaceholder')} />
                    </FormField>
                  </div>
                  <p class="text-xs text-muted">{k('channels.feishu.permissionHint')}</p>
                </SubSection>
              </div>
            </div>

            <SubSection title={k('scenes.title')}>
              <div class="grid grid-cols-1 gap-3 md:grid-cols-2">
                {NOTIFICATION_SCENES.map((s) => (
                  <Switch key={s} v-model={f.scenes[s]} label={t(NOTIFICATION_SCENE_LABEL_KEYS[s])} />
                ))}
              </div>
              <p class="mt-3 text-xs text-muted">{k('scenes.exceptionThresholdHint')}</p>
            </SubSection>

            <SubSection title={k('templates.title')} bodyClass="space-y-4">
              {{
                extra: () => <LangTag lang={lang} />,
                default: () =>
                  NOTIFICATION_SCENES.map((s) => (
                    <div key={s} class="space-y-2 rounded-zs border border-line bg-surface-muted/40 p-4">
                      <h5 class="text-sm font-medium text-fg">{t(NOTIFICATION_SCENE_LABEL_KEYS[s])}</h5>
                      <Input v-model={f.templates[s][lang].title} placeholder={k('templates.titlePlaceholder')} />
                      <Textarea v-model={f.templates[s][lang].body} rows={4} placeholder={k('templates.bodyPlaceholder')} />
                      {s === 'exception_alert' && <p class="text-xs text-muted">{k('templates.variableHint')}</p>}
                    </div>
                  )),
              }}
            </SubSection>
          </div>
        </Card>
      )
    }
  },
})
