import { defineComponent, type PropType } from 'vue'
import { useI18n } from 'vue-i18n'
import { Save } from 'lucide-vue-next'
import { Button, PageHeader, Switch, Tabs } from '@/components/ui'
import type { TelegramBotSettingsState } from '../useTelegramBotSettings'
import type { SupportedLanguage } from '../telegramUtils'

/** Page header for the bot settings pages: title, language switch and Save button. */
export const BotSettingsHeader = defineComponent({
  name: 'BotSettingsHeader',
  props: {
    title: { type: String, required: true },
    subtitle: String,
    state: { type: Object as PropType<TelegramBotSettingsState>, required: true },
  },
  setup(props) {
    const { t } = useI18n()
    return () => {
      const s = props.state
      return (
        <PageHeader title={props.title} subtitle={props.subtitle}>
          {{
            actions: () => (
              <>
                <Tabs
                  modelValue={s.currentLang.value}
                  onUpdate:modelValue={(v: string) => (s.currentLang.value = v as SupportedLanguage)}
                  items={s.languages.value.map((l) => ({ key: l.code, label: l.name }))}
                />
                <Button variant="primary" loading={s.saving.value} disabled={s.loading.value} onClick={() => void s.saveConfig()}>
                  {!s.saving.value && <Save class="h-4 w-4" />}
                  {t('telegramBot.settings.save')}
                </Button>
              </>
            ),
          }}
        </PageHeader>
      )
    }
  },
})

/** Small language chip shown in card headers (current editing language). */
export const LangChip = (props: { lang: string }) => (
  <span class="rounded-zs-sm bg-surface-muted px-2 py-1 font-mono text-xs text-muted">{props.lang}</span>
)

/** Bordered row containing a switch + label (the original's "switch box"). */
export const SwitchRow = defineComponent({
  name: 'BotSwitchRow',
  props: {
    modelValue: Boolean,
    label: { type: String, required: true },
  },
  emits: { 'update:modelValue': (_v: boolean) => true },
  setup(props, { emit }) {
    return () => (
      <div class="flex items-center rounded-zs border border-line bg-surface-muted px-4 py-3 text-sm">
        <Switch modelValue={props.modelValue} label={props.label} onUpdate:modelValue={(v: boolean) => emit('update:modelValue', v)} />
      </div>
    )
  },
})
