import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import { notifySuccess } from '@/utils/notify'
import { notifyFailure, tr } from './common'
import { asRecord, asString } from './settingsUtils'

/** Google 登录 (`settings/google-auth`). */
export function useGoogleAuthSettings() {
  const submitting = ref(false)
  const form = reactive({ enabled: false, client_id: '' })

  const load = (raw: unknown) => {
    const d = asRecord(raw)
    form.enabled = !!d.enabled
    form.client_id = asString(d.client_id)
  }

  const save = async () => {
    submitting.value = true
    try {
      const res = await adminAPI.updateGoogleAuthSettings({ enabled: form.enabled, client_id: form.client_id.trim() })
      load(res.data)
      notifySuccess(tr('admin.settings.alerts.saveSuccess'))
    } catch (err) {
      notifyFailure(err)
    } finally {
      submitting.value = false
    }
  }

  return { form, submitting, load, save }
}

export type GoogleAuthSettingsModel = ReturnType<typeof useGoogleAuthSettings>
