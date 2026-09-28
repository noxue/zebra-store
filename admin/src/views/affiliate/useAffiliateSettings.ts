import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { formToSettings, settingsToForm, type AffiliateSettingsForm } from './affiliateUtils'

/** Page logic for 推广返利设置 (`GET/PUT /admin/settings/affiliate`). */
export function useAffiliateSettings() {
  const t = i18n.global.t
  const loading = ref(false)
  const saving = ref(false)
  const form = reactive<AffiliateSettingsForm>(settingsToForm(null))

  const fetchSettings = async () => {
    loading.value = true
    try {
      const res = await adminAPI.getAffiliateSettings()
      if (res.data) Object.assign(form, settingsToForm(res.data))
    } catch {
      /* toast shown by client */
    } finally {
      loading.value = false
    }
  }

  const save = async () => {
    saving.value = true
    try {
      const payload = formToSettings(form)
      const res = await adminAPI.updateAffiliateSettings(payload)
      if (res.data) Object.assign(form, settingsToForm(res.data, payload))
      notifySuccess(t('admin.settings.alerts.saveSuccess'))
    } catch {
      /* toast shown by client */
    } finally {
      saving.value = false
    }
  }

  return { loading, saving, form, fetchSettings, save }
}
