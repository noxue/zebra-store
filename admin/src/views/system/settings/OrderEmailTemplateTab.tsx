import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { RotateCcw } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Select, Textarea } from '@/components/ui'
import { orderEmailSceneKeys } from '@/utils/orderEmailTemplates'
import type { LangCode } from './settingsUtils'
import { ORDER_EMAIL_VARIABLES, type OrderEmailScene, type OrderEmailTemplateSettingsModel } from './useOrderEmailTemplateSettings'
import { LangTag } from './SettingsUi'

export default defineComponent({
  name: 'SettingsOrderEmailTemplateTab',
  props: {
    model: { type: Object as PropType<OrderEmailTemplateSettingsModel>, required: true },
    lang: { type: String as PropType<LangCode>, required: true },
  },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.orderEmailTemplate.${s}`)
    return () => {
      const m = props.model
      const f = m.form
      const lang = props.lang
      const tpl = f.templates[m.currentScene.value][lang]
      return (
        <Card title={k('title')} description={k('subtitle')}>
          {{
            extra: () => <LangTag lang={lang} />,
            default: () => (
              <div class="space-y-6">
                <div class="rounded-zs border border-line bg-accent-soft/50 p-4">
                  <h4 class="mb-2 text-sm font-medium text-info-text">{k('variables')}</h4>
                  <div class="flex flex-wrap gap-2">
                    {ORDER_EMAIL_VARIABLES.map((v) => (
                      <span key={v} class="inline-flex items-center gap-1.5 rounded-zs-sm bg-surface-strong px-2 py-1 text-xs">
                        <code class="font-mono text-accent">{`{{${v}}}`}</code>
                        <span class="text-muted">{k(`variableList.${v}`)}</span>
                      </span>
                    ))}
                  </div>
                </div>
                <div class="sm:w-64">
                  <FormField label={k('scene')}>
                    <Select
                      modelValue={m.currentScene.value}
                      onUpdate:modelValue={(v) => (m.currentScene.value = String(v) as OrderEmailScene)}
                      options={orderEmailSceneKeys.map((key) => ({ label: k(`scenes.${key}`), value: key }))}
                    />
                  </FormField>
                </div>
                <div class="space-y-4 rounded-zs border border-line p-4">
                  <FormField label={k('subject')}>
                    <Input v-model={tpl.subject} />
                  </FormField>
                  <FormField label={k('body')}>
                    <Textarea v-model={tpl.body} rows={8} mono />
                  </FormField>
                </div>
                <FormField label={k('guestTip')} hint={k('guestTipDesc')}>
                  <Textarea v-model={f.guest_tip[lang]} rows={2} mono />
                </FormField>
                <div class="flex flex-wrap items-center gap-3">
                  <Button variant="primary" loading={m.submitting.value} disabled={m.submitting.value || m.resetting.value} onClick={m.save}>
                    {m.submitting.value ? t('admin.settings.actions.saving') : t('admin.settings.actions.save')}
                  </Button>
                  <Button loading={m.resetting.value} disabled={m.submitting.value || m.resetting.value} onClick={m.resetToDefault}>
                    <RotateCcw class="h-3.5 w-3.5" />
                    {k('resetToDefault')}
                  </Button>
                </div>
              </div>
            ),
          }}
        </Card>
      )
    }
  },
})
