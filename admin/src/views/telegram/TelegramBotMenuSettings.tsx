import { defineComponent, onMounted } from 'vue'
import { useI18n } from 'vue-i18n'
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-vue-next'
import { Button, Card, FormField, Input, Select, Switch } from '@/components/ui'
import { BotSettingsHeader, LangChip } from './components/BotSettingsHeader'
import { useTelegramBotSettings } from './useTelegramBotSettings'
import type { MenuActionType } from './telegramUtils'

export default defineComponent({
  name: 'TelegramBotMenuSettings',
  setup() {
    const { t } = useI18n()
    const s = useTelegramBotSettings()
    onMounted(() => void s.fetchConfig())

    return () => {
      const items = s.form.value.menu.items
      const lang = s.currentLang.value
      const actionOptions = s.menuActionTypes.map((type) => ({ label: t(`telegramBot.settings.menuActionType_${type}`), value: type }))
      return (
        <div class="space-y-6">
          <BotSettingsHeader title={t('telegramBot.settings.menuTitle')} subtitle={t('telegramBot.settings.menuDesc')} state={s} />

          <Card title={t('telegramBot.settings.menuTitle')} description={t('telegramBot.settings.menuDesc')}>
            {{
              extra: () => (
                <>
                  <LangChip lang={lang} />
                  <span class="zs-num text-xs text-muted">
                    {items.length}/{s.menuItemsMaxCount}
                  </span>
                  <Button size="sm" onClick={s.addMenuItem}>
                    <Plus class="h-4 w-4" />
                    {t('telegramBot.settings.menuAdd')}
                  </Button>
                </>
              ),
              default: () => (
                <div class="space-y-3">
                  {items.length === 0 && (
                    <div class="rounded-zs border border-dashed border-line-strong p-6 text-center text-sm text-muted">{t('telegramBot.settings.menuEmpty')}</div>
                  )}
                  {items.map((item, index) => (
                    <div key={index} class="space-y-3 rounded-zs border border-line bg-surface-strong p-4">
                      <div class="flex flex-wrap items-center gap-2">
                        <span class="zs-num flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary-soft text-xs text-primary">{index + 1}</span>
                        <Switch v-model={item.enabled} />
                        <div class="min-w-[160px] flex-1">
                          <Input v-model={item.key} placeholder={t('telegramBot.settings.menuKeyPlaceholder')} mono />
                        </div>
                        <div class="flex flex-wrap gap-1">
                          <Button size="sm" variant="ghost" disabled={index === 0} onClick={() => s.moveMenuItem(index, 'up')}>
                            <ArrowUp class="h-4 w-4" />
                            {t('telegramBot.settings.menuMoveUp')}
                          </Button>
                          <Button size="sm" variant="ghost" disabled={index === items.length - 1} onClick={() => s.moveMenuItem(index, 'down')}>
                            <ArrowDown class="h-4 w-4" />
                            {t('telegramBot.settings.menuMoveDown')}
                          </Button>
                          <Button size="sm" variant="ghost" onClick={() => s.removeMenuItem(index)}>
                            <Trash2 class="h-4 w-4 text-danger-text" />
                            {t('admin.common.delete')}
                          </Button>
                        </div>
                      </div>
                      <FormField label={t('telegramBot.settings.menuLabel')}>
                        <Input v-model={item.label[lang]} placeholder={t('telegramBot.settings.menuLabelPlaceholder')} />
                      </FormField>
                      <div class="grid grid-cols-1 gap-3 md:grid-cols-3">
                        <FormField label={t('telegramBot.settings.menuActionType')}>
                          <Select
                            modelValue={item.action.type}
                            onUpdate:modelValue={(v) => (item.action.type = v as MenuActionType)}
                            options={actionOptions}
                          />
                        </FormField>
                        <FormField label={t('telegramBot.settings.menuActionValue')} hint={s.getMenuActionValueHint(item.action.type)}>
                          <Input v-model={item.action.value} placeholder={s.getMenuActionValuePlaceholder(item.action.type)} mono />
                        </FormField>
                        <FormField label={t('telegramBot.settings.menuOrder')}>
                          <Input
                            type="number"
                            modelValue={item.order}
                            onUpdate:modelValue={(v: string | number) => (item.order = typeof v === 'number' ? v : Number(v) || 0)}
                          />
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
