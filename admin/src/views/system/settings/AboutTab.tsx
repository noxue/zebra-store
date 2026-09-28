import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Plus } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Textarea } from '@/components/ui'
import type { LangCode } from './settingsUtils'
import type { SiteSettingsModel } from './useSiteSettings'
import { EmptyHint, LangTag, SubSection } from './SettingsUi'

export default defineComponent({
  name: 'SettingsAboutTab',
  props: {
    model: { type: Object as PropType<SiteSettingsModel>, required: true },
    lang: { type: String as PropType<LangCode>, required: true },
    langName: { type: String, required: true },
  },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const m = props.model
      const about = m.form.about
      const lang = props.lang
      return (
        <Card title={t('admin.settings.about.title')} description={t('admin.settings.about.subtitle', { lang: props.langName })}>
          {{
            extra: () => <LangTag lang={lang} />,
            default: () => (
              <div class="space-y-5">
                <SubSection title={t('admin.settings.about.heroTitle')} bodyClass="grid grid-cols-1 gap-4 md:grid-cols-2">
                  <FormField label={t('admin.settings.about.heroMainTitle')}>
                    <Input v-model={about.hero.title[lang]} placeholder={t('admin.settings.about.heroMainTitlePlaceholder')} />
                  </FormField>
                  <FormField label={t('admin.settings.about.heroSubtitle')}>
                    <Input v-model={about.hero.subtitle[lang]} placeholder={t('admin.settings.about.heroSubtitlePlaceholder')} />
                  </FormField>
                </SubSection>
                <SubSection title={t('admin.settings.about.introductionTitle')} description={t('admin.settings.about.introductionSubtitle')}>
                  <Textarea v-model={about.introduction[lang]} rows={5} placeholder={t('admin.settings.about.introductionPlaceholder')} />
                </SubSection>
                <SubSection title={t('admin.settings.about.servicesTitle')} bodyClass="space-y-4">
                  {{
                    extra: () => (
                      <Button size="sm" onClick={m.addAboutServiceItem}>
                        <Plus class="h-3.5 w-3.5" />
                        {t('admin.settings.about.addServiceItem')}
                      </Button>
                    ),
                    default: () => (
                      <>
                        <FormField label={t('admin.settings.about.servicesBlockTitle')}>
                          <Input v-model={about.services.title[lang]} placeholder={t('admin.settings.about.servicesBlockTitlePlaceholder')} />
                        </FormField>
                        {about.services.items.length === 0 && <EmptyHint text={t('admin.settings.about.servicesEmpty')} />}
                        {about.services.items.map((item, index) => (
                          <div key={`about-service-${index}`} class="space-y-2 rounded-zs border border-line bg-surface-muted/40 p-3">
                            <div class="flex items-center justify-between gap-3">
                              <span class="text-xs font-medium text-muted">{t('admin.settings.about.serviceItem', { index: index + 1 })}</span>
                              <Button size="xs" variant="danger" onClick={() => m.removeAboutServiceItem(index)}>
                                {t('admin.common.delete')}
                              </Button>
                            </div>
                            <Input v-model={item[lang]} placeholder={t('admin.settings.about.serviceItemPlaceholder')} />
                          </div>
                        ))}
                      </>
                    ),
                  }}
                </SubSection>
                <SubSection title={t('admin.settings.about.contactTitle')} description={t('admin.settings.about.contactSubtitle')} bodyClass="space-y-4">
                  <FormField label={t('admin.settings.about.contactBlockTitle')}>
                    <Input v-model={about.contact.title[lang]} placeholder={t('admin.settings.about.contactBlockTitlePlaceholder')} />
                  </FormField>
                  <FormField label={t('admin.settings.about.contactText')}>
                    <Textarea v-model={about.contact.text[lang]} rows={4} placeholder={t('admin.settings.about.contactTextPlaceholder')} />
                  </FormField>
                </SubSection>
              </div>
            ),
          }}
        </Card>
      )
    }
  },
})
