import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { Card, FormField, Input, Select, Textarea } from '@/components/ui'
import { MediaPicker } from '@/components/MediaPicker'
import { BotSettingsHeader, LangChip, SwitchRow } from './components/BotSettingsHeader'
import { useTelegramBotSettings } from './useTelegramBotSettings'
import type { SupportedLanguage } from './telegramUtils'

export default defineComponent({
  name: 'TelegramBotSettings',
  setup() {
    const { t } = useI18n()
    const s = useTelegramBotSettings()
    onMounted(() => void s.fetchConfig())

    return () => {
      const form = s.form.value
      const lang = s.currentLang.value
      return (
        <div class="space-y-6">
          <BotSettingsHeader title={t('telegramBot.settings.title')} subtitle={t('telegramBot.settings.subtitle')} state={s} />

          <Card title={t('telegramBot.settings.globalTitle')} description={t('telegramBot.settings.globalDesc')}>
            <div class="grid grid-cols-1 items-end gap-4 md:grid-cols-2">
              <SwitchRow v-model={form.enabled} label={t('telegramBot.settings.enabled')} />
              <FormField label={t('telegramBot.settings.defaultLocale')}>
                <Select
                  modelValue={form.default_locale}
                  onUpdate:modelValue={(v) => (form.default_locale = v as SupportedLanguage)}
                  options={[
                    { label: '简体中文', value: 'zh-CN' },
                    { label: '繁體中文', value: 'zh-TW' },
                    { label: 'English', value: 'en-US' },
                  ]}
                />
              </FormField>
            </div>
          </Card>

          <Card title={t('telegramBot.settings.basicInfo')} description={t('telegramBot.settings.basicInfoDesc')}>
            {{
              extra: () => <LangChip lang={lang} />,
              default: () => (
                <div class="space-y-4">
                  <FormField label={t('telegramBot.settings.displayName')}>
                    <Input v-model={form.basic.display_name} placeholder={t('telegramBot.settings.displayNamePlaceholder')} />
                  </FormField>
                  <FormField label={t('telegramBot.settings.description')}>
                    <Textarea v-model={form.basic.description[lang]} rows={2} placeholder={t('telegramBot.settings.descriptionPlaceholder')} />
                  </FormField>
                  <FormField label={t('telegramBot.settings.supportUrl')}>
                    <Input v-model={form.basic.support_url} placeholder={t('telegramBot.settings.supportUrlPlaceholder')} />
                  </FormField>
                  <FormField label={t('telegramBot.settings.coverUrl')}>
                    <MediaPicker
                      modelValue={form.basic.cover_url}
                      onUpdate:modelValue={(v: string | string[]) => (form.basic.cover_url = Array.isArray(v) ? (v[0] ?? '') : v)}
                      scene="telegram"
                    />
                  </FormField>
                </div>
              ),
            }}
          </Card>

          <Card title={t('telegramBot.settings.welcomeTitle')} description={t('telegramBot.settings.welcomeDesc')}>
            {{
              extra: () => <LangChip lang={lang} />,
              default: () => (
                <div class="space-y-4">
                  <SwitchRow v-model={form.welcome.enabled} label={t('telegramBot.settings.welcomeEnabled')} />
                  <FormField label={t('telegramBot.settings.welcomeMessage')}>
                    <Textarea v-model={form.welcome.message[lang]} rows={3} placeholder={t('telegramBot.settings.welcomeMessagePlaceholder')} />
                  </FormField>
                </div>
              ),
            }}
          </Card>
        </div>
      )
    }
  },
})
