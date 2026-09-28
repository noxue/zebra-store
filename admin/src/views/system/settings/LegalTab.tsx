import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Card } from '@/components/ui'
import { RichEditor } from '@/components/RichEditor'
import type { LangCode } from './settingsUtils'
import type { SiteSettingsModel } from './useSiteSettings'
import { LangTag } from './SettingsUi'

export default defineComponent({
  name: 'SettingsLegalTab',
  props: {
    model: { type: Object as PropType<SiteSettingsModel>, required: true },
    lang: { type: String as PropType<LangCode>, required: true },
    langName: { type: String, required: true },
  },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const legal = props.model.form.legal
      const lang = props.lang
      return (
        <div class="space-y-6">
          <Card title={t('admin.settings.legal.termsTitle')} description={t('admin.settings.legal.termsSubtitle', { lang: props.langName })}>
            {{
              extra: () => <LangTag lang={lang} />,
              default: () => <RichEditor key={`terms-${lang}`} v-model={legal.terms[lang]} placeholder={t('admin.settings.legal.termsPlaceholder')} />,
            }}
          </Card>
          <Card title={t('admin.settings.legal.privacyTitle')} description={t('admin.settings.legal.privacySubtitle', { lang: props.langName })}>
            {{
              extra: () => <LangTag lang={lang} />,
              default: () => <RichEditor key={`privacy-${lang}`} v-model={legal.privacy[lang]} placeholder={t('admin.settings.legal.privacyPlaceholder')} />,
            }}
          </Card>
        </div>
      )
    }
  },
})
