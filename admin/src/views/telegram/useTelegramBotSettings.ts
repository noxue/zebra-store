import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import type { JsonObject } from '@/api/types'
import { notifyError, notifySuccess } from '@/utils/notify'
import {
  createHelpItem,
  createMenuItem,
  createTelegramBotSettingsForm,
  helpItemsMaxCount,
  menuActionTypes,
  menuItemsMaxCount,
  moveItem,
  parseTelegramBotSettings,
  type MenuActionType,
  type SupportedLanguage,
  type TelegramBotSettingsForm,
} from './telegramUtils'

export type { MenuActionType, SupportedLanguage, TelegramBotSettingsForm }

/** Shared state + actions for the Telegram Bot settings pages (settings / help center / menu). */
export function useTelegramBotSettings() {
  const { t } = useI18n()

  const currentLang = ref<SupportedLanguage>('zh-CN')
  const loading = ref(false)
  const saving = ref(false)
  const uploadingCover = ref(false)
  const form = ref<TelegramBotSettingsForm>(createTelegramBotSettingsForm())

  const languages = computed(() => [
    { code: 'zh-CN' as SupportedLanguage, name: t('admin.common.lang.zhCN') },
    { code: 'zh-TW' as SupportedLanguage, name: t('admin.common.lang.zhTW') },
    { code: 'en-US' as SupportedLanguage, name: t('admin.common.lang.enUS') },
  ])

  const fetchConfig = async () => {
    loading.value = true
    try {
      form.value = createTelegramBotSettingsForm()
      const res = await adminAPI.getTelegramBotSettings()
      form.value = parseTelegramBotSettings(res.data)
    } catch {
      /* already notified */
    } finally {
      loading.value = false
    }
  }

  const saveConfig = async () => {
    saving.value = true
    try {
      await adminAPI.updateTelegramBotSettings({ ...form.value } as unknown as JsonObject)
      notifySuccess(t('telegramBot.settings.saveSuccess'))
    } catch {
      /* already notified */
    } finally {
      saving.value = false
    }
  }

  /** Upload a cover image file (scene `telegram`) and use it as `basic.cover_url`. */
  const handleUploadCover = async (file: File | null) => {
    if (!file) return
    uploadingCover.value = true
    try {
      const res = await adminAPI.upload(file, 'telegram')
      form.value.basic.cover_url = res.data?.url || ''
    } catch {
      /* already notified */
    } finally {
      uploadingCover.value = false
    }
  }

  const addMenuItem = () => {
    if (form.value.menu.items.length >= menuItemsMaxCount) {
      notifyError(t('telegramBot.settings.menuMaxHint', { max: menuItemsMaxCount }))
      return
    }
    form.value.menu.items.push(createMenuItem())
  }
  const removeMenuItem = (index: number) => {
    form.value.menu.items.splice(index, 1)
  }
  const moveMenuItem = (index: number, direction: 'up' | 'down') => {
    moveItem(form.value.menu.items, index, direction)
  }

  const addHelpItem = () => {
    if (form.value.help.items.length >= helpItemsMaxCount) {
      notifyError(t('telegramBot.settings.helpMaxHint', { max: helpItemsMaxCount }))
      return
    }
    form.value.help.items.push(createHelpItem())
  }
  const removeHelpItem = (index: number) => {
    form.value.help.items.splice(index, 1)
  }
  const moveHelpItem = (index: number, direction: 'up' | 'down') => {
    moveItem(form.value.help.items, index, direction)
  }

  const getMenuActionValuePlaceholder = (type: MenuActionType) => t(`telegramBot.settings.menuActionValuePlaceholder_${type}`)
  const getMenuActionValueHint = (type: MenuActionType) => t(`telegramBot.settings.menuActionValueHint_${type}`)

  return {
    currentLang,
    fetchConfig,
    form,
    handleUploadCover,
    addHelpItem,
    addMenuItem,
    getMenuActionValueHint,
    getMenuActionValuePlaceholder,
    languages,
    loading,
    menuActionTypes,
    menuItemsMaxCount,
    helpItemsMaxCount,
    moveHelpItem,
    moveMenuItem,
    removeHelpItem,
    removeMenuItem,
    saveConfig,
    saving,
    uploadingCover,
  }
}

export type TelegramBotSettingsState = ReturnType<typeof useTelegramBotSettings>
