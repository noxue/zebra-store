import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus, Trash2 } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Select, Switch, cn } from '@/components/ui'
import { NAV_CUSTOM_MAX, type LangCode } from './settingsUtils'
import { NAV_PRESET_ICONS, type NavigationSettingsModel } from './useNavigationSettings'
import { EmptyHint } from './SettingsUi'

export default defineComponent({
  name: 'SettingsNavigationTab',
  props: {
    model: { type: Object as PropType<NavigationSettingsModel>, required: true },
    lang: { type: String as PropType<LangCode>, required: true },
  },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.navigation.custom.fields.${s}`)
    return () => {
      const m = props.model
      const f = m.form
      const full = f.customItems.length >= NAV_CUSTOM_MAX
      return (
        <div class="space-y-6">
          <Card title={t('admin.settings.navigation.builtin.title')} description={t('admin.settings.navigation.builtin.subtitle')}>
            <div class="divide-y divide-line">
              {(['blog', 'notice', 'about'] as const).map((key) => (
                <div key={key} class="flex items-center justify-between py-3 first:pt-0 last:pb-0">
                  <span class="text-sm font-medium text-fg">{t(`admin.settings.navigation.builtin.${key}`)}</span>
                  <Switch v-model={f.builtin[key]} />
                </div>
              ))}
            </div>
          </Card>

          <Card title={t('admin.settings.navigation.custom.title')} description={t('admin.settings.navigation.custom.subtitle', { max: NAV_CUSTOM_MAX })}>
            {{
              extra: () => (
                <Button size="sm" variant="primary" disabled={full} onClick={m.addItem}>
                  <Plus class="h-3.5 w-3.5" />
                  {full ? t('admin.settings.navigation.custom.maxReached') : t('admin.settings.navigation.custom.add')}
                </Button>
              ),
              default: () =>
                f.customItems.length === 0 ? (
                  <EmptyHint text={t('admin.settings.navigation.custom.empty')} />
                ) : (
                  <div class="space-y-4">
                    {f.customItems.map((item, index) => (
                      <div key={item.id} class="space-y-4 rounded-zs border border-line bg-surface-muted/40 p-4">
                        <div class="flex items-center justify-between gap-3">
                          <span class="zs-num text-sm font-medium text-muted">#{index + 1}</span>
                          <div class="flex items-center gap-3">
                            <Switch size="sm" v-model={item.enabled} label={k('enabled')} />
                            <Button size="xs" variant="ghost" class="text-danger-text" onClick={() => m.removeItem(index)}>
                              <Trash2 class="h-3.5 w-3.5" />
                              {t('admin.settings.navigation.custom.delete')}
                            </Button>
                          </div>
                        </div>
                        <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                          <div class="sm:col-span-2">
                            <FormField label={k('icon')}>
                              <div class="flex flex-wrap gap-1.5">
                                {NAV_PRESET_ICONS.map((preset) => (
                                  <button
                                    key={preset.key}
                                    type="button"
                                    title={preset.label}
                                    onClick={() => (item.icon = preset.key)}
                                    class={cn(
                                      'flex h-8 w-8 items-center justify-center rounded-zs-sm border transition-colors',
                                      item.icon === preset.key ? 'border-primary bg-primary-soft text-primary' : 'border-line-strong bg-surface-strong text-muted hover:border-primary',
                                    )}
                                  >
                                    <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d={preset.path} />
                                    </svg>
                                  </button>
                                ))}
                              </div>
                            </FormField>
                          </div>
                          <FormField label={`${k('title')} (${props.lang})`}>
                            <Input v-model={item.title[props.lang]} placeholder={k('title')} />
                          </FormField>
                          <FormField label={k('linkType')}>
                            <Select
                              modelValue={item.link_type}
                              onUpdate:modelValue={(v) => (item.link_type = v === 'external' ? 'external' : 'internal')}
                              options={[
                                { label: k('linkTypeInternal'), value: 'internal' },
                                { label: k('linkTypeExternal'), value: 'external' },
                              ]}
                            />
                          </FormField>
                          <FormField label={k('url')}>
                            <Input v-model={item.url} placeholder={item.link_type === 'internal' ? k('urlPlaceholderInternal') : k('urlPlaceholderExternal')} />
                          </FormField>
                          <FormField label={k('target')}>
                            <Select
                              modelValue={item.target}
                              onUpdate:modelValue={(v) => (item.target = v === '_blank' ? '_blank' : '_self')}
                              options={[
                                { label: k('targetSelf'), value: '_self' },
                                { label: k('targetBlank'), value: '_blank' },
                              ]}
                            />
                          </FormField>
                          <FormField label={k('sortOrder')}>
                            <Input type="number" v-model={item.sort_order} placeholder="0" />
                          </FormField>
                        </div>
                      </div>
                    ))}
                  </div>
                ),
            }}
          </Card>
        </div>
      )
    }
  },
})
