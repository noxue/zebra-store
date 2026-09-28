import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Switch, Textarea } from '@/components/ui'
import { BotSettingsHeader, LangChip, SwitchRow } from './components/BotSettingsHeader'
import { useTelegramBotSettings } from './useTelegramBotSettings'

export default defineComponent({
  name: 'TelegramBotHelpCenter',
  setup() {
    const { t } = useI18n()
    const s = useTelegramBotSettings()
    onMounted(() => void s.fetchConfig())

    return () => {
      const help = s.form.value.help
      const lang = s.currentLang.value
      return (
        <div class="space-y-6">
          <BotSettingsHeader title={t('telegramBot.settings.helpTitle')} subtitle={t('telegramBot.settings.helpDesc')} state={s} />

          <Card title={t('telegramBot.settings.helpTitle')} description={t('telegramBot.settings.helpDesc')}>
            {{
              extra: () => (
                <>
                  <LangChip lang={lang} />
                  <span class="zs-num text-xs text-muted">
                    {help.items.length}/{s.helpItemsMaxCount}
                  </span>
                  <Button size="sm" onClick={s.addHelpItem}>
                    <Plus class="h-4 w-4" />
                    {t('telegramBot.settings.helpAdd')}
                  </Button>
                </>
              ),
              default: () => (
                <div class="space-y-4">
                  <SwitchRow v-model={help.enabled} label={t('telegramBot.settings.helpEnabled')} />
                  <FormField label={t('telegramBot.settings.helpCenterTitle')}>
                    <Input v-model={help.title[lang]} placeholder={t('telegramBot.settings.helpCenterTitlePlaceholder')} />
                  </FormField>
                  <FormField label={t('telegramBot.settings.helpIntro')}>
                    <Textarea v-model={help.intro[lang]} rows={2} placeholder={t('telegramBot.settings.helpIntroPlaceholder')} />
                  </FormField>
                  <FormField label={t('telegramBot.settings.helpCenterHint')}>
                    <Textarea v-model={help.center_hint[lang]} rows={2} placeholder={t('telegramBot.settings.helpCenterHintPlaceholder')} />
                  </FormField>
                  <FormField label={t('telegramBot.settings.helpSupportHint')}>
                    <Textarea v-model={help.support_hint[lang]} rows={2} placeholder={t('telegramBot.settings.helpSupportHintPlaceholder')} />
                  </FormField>

                  {help.items.length === 0 && (
                    <div class="rounded-zs border border-dashed border-line-strong p-6 text-center text-sm text-muted">{t('telegramBot.settings.helpEmpty')}</div>
                  )}
                  {help.items.map((item, index) => (
                    <div key={`help-${index}`} class="space-y-3 rounded-zs border border-line bg-surface-strong p-4">
                      <div class="flex flex-wrap items-center gap-2">
                        <span class="zs-num flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary-soft text-xs text-primary">{index + 1}</span>
                        <Switch v-model={item.enabled} />
                        <div class="min-w-[120px] flex-1">
                          <Input v-model={item.key} placeholder={t('telegramBot.settings.helpKeyPlaceholder')} mono />
                        </div>
                        <Button size="icon-sm" variant="ghost" disabled={index === 0} onClick={() => s.moveHelpItem(index, 'up')}>
                          <ArrowUp class="h-4 w-4" />
                        </Button>
                        <Button size="icon-sm" variant="ghost" disabled={index === help.items.length - 1} onClick={() => s.moveHelpItem(index, 'down')}>
                          <ArrowDown class="h-4 w-4" />
                        </Button>
                        <Button size="icon-sm" variant="ghost" onClick={() => s.removeHelpItem(index)}>
                          <Trash2 class="h-4 w-4 text-danger-text" />
                        </Button>
                      </div>
                      <FormField label={t('telegramBot.settings.helpSummary')}>
                        <Input v-model={item.summary[lang]} placeholder={t('telegramBot.settings.helpSummaryPlaceholder')} />
                      </FormField>
                      <FormField label={t('telegramBot.settings.helpItemTitle')}>
                        <Input v-model={item.title[lang]} placeholder={t('telegramBot.settings.helpItemTitlePlaceholder')} />
                      </FormField>
                      <FormField label={t('telegramBot.settings.helpContent')}>
                        <Textarea v-model={item.content[lang]} rows={4} placeholder={t('telegramBot.settings.helpContentPlaceholder')} />
                      </FormField>
                      <div class="grid grid-cols-1 items-end gap-3 md:grid-cols-2">
                        <FormField label={t('telegramBot.settings.helpOrder')}>
                          <Input
                            type="number"
                            modelValue={item.order}
                            onUpdate:modelValue={(v: string | number) => (item.order = typeof v === 'number' ? v : Number(v) || 0)}
                          />
                        </FormField>
                        <div class="pb-2">
                          <Switch v-model={item.show_support_link} label={t('telegramBot.settings.helpShowSupportLink')} />
                        </div>
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
