import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Card, DateTimeInput, FormField, Input, Select } from '@/components/ui'
import { RichEditor } from '@/components/RichEditor'
import type { LangCode } from './settingsUtils'
import type { AnnouncementType, HomeAnnouncementSettingsModel } from './useHomeAnnouncementSettings'
import { LangTag, ToggleRow } from './SettingsUi'

export default defineComponent({
  name: 'SettingsHomeAnnouncementTab',
  props: {
    model: { type: Object as PropType<HomeAnnouncementSettingsModel>, required: true },
    lang: { type: String as PropType<LangCode>, required: true },
  },
  setup(props) {
    const { t } = useI18n()
    const k = (s: string) => t(`admin.settings.homeAnnouncement.${s}`)
    return () => {
      const f = props.model.form
      const lang = props.lang
      return (
        <Card title={k('title')} description={k('subtitle')}>
          <div class="space-y-5">
            <ToggleRow v-model={f.enabled} label={k('enabled')} description={k('enabledDesc')} trailing />
            <div class="sm:w-60">
              <FormField label={k('type')}>
                <Select
                  modelValue={f.type}
                  onUpdate:modelValue={(v) => (f.type = String(v) as AnnouncementType)}
                  options={[
                    { label: k('typeNormal'), value: 'normal' },
                    { label: k('typeInfo'), value: 'info' },
                    { label: k('typeWarning'), value: 'warning' },
                  ]}
                />
              </FormField>
            </div>
            <FormField>
              {{
                label: () => (
                  <>
                    {k('announceTitle')} <LangTag lang={lang} />
                  </>
                ),
                default: () => <Input v-model={f.title[lang]} placeholder={k('announceTitlePlaceholder')} />,
              }}
            </FormField>
            <FormField>
              {{
                label: () => (
                  <>
                    {k('content')} <LangTag lang={lang} />
                  </>
                ),
                default: () => <RichEditor key={`announcement-${lang}`} v-model={f.content[lang]} placeholder={k('contentPlaceholder')} />,
              }}
            </FormField>
            <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
              <FormField label={k('startAt')}>
                <DateTimeInput v-model={f.start_at} />
              </FormField>
              <FormField label={k('endAt')}>
                <DateTimeInput v-model={f.end_at} />
              </FormField>
            </div>
            <p class="text-xs text-muted">{k('scheduleHint')}</p>
          </div>
        </Card>
      )
    }
  },
})
