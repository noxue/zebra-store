import { reactive, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { LOCALE_LABELS, SUPPORTED_LOCALES } from '@/i18n'
import { useUserProfileStore } from '@/stores/userProfile'
import { usePanelAlert } from './usePanelAlert'

/** Nickname + preferred locale form. */
export function useProfilePanel() {
  const { t } = useI18n()
  const store = useUserProfileStore()
  const { alert, show, clear } = usePanelAlert()
  const form = reactive({ nickname: '', locale: 'zh-CN' })
  const localeOptions = SUPPORTED_LOCALES.map((code) => ({ value: code, label: LOCALE_LABELS[code] }))

  watch(
    () => store.profile,
    (profile) => {
      if (!profile) return
      form.nickname = profile.nickname || ''
      form.locale = profile.locale || 'zh-CN'
    },
    { immediate: true },
  )

  const save = async () => {
    clear()
    const ok = await store.saveProfile({ nickname: form.nickname.trim(), locale: form.locale })
    if (!ok) return show('error', store.profileError || t('personalCenter.common.saveFailed'))
    show('success', t('personalCenter.profile.saveSuccess'))
  }

  return { store, form, alert, localeOptions, save }
}
