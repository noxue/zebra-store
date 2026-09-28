import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { apiCredentialAPI } from '@/api/credential'
import type { ApiCredentialData } from '@/api/types'
import { maskSecret } from '@/utils/personal'
import { usePanelAlert } from './usePanelAlert'

const SECRET_VIEWED_KEY = 'api_secret_viewed'

const readViewed = (): string | null => {
  try {
    return localStorage.getItem(SECRET_VIEWED_KEY)
  } catch {
    return null
  }
}

/** API credential: apply, review states, secret generation (shown once), active switch. */
export function useApiPanel() {
  const { t } = useI18n()
  const { alert, show, clear } = usePanelAlert()
  const loading = ref(true)
  const submitting = ref(false)
  const credential = ref<ApiCredentialData | null>(null)
  const newSecret = ref('')
  const confirmOpen = ref(false)
  const hasViewedSecret = ref(false)
  const maskedSecret = ref(maskSecret())

  const load = async () => {
    loading.value = true
    try {
      const data = (await apiCredentialAPI.getMy()).data
      if (!data || data.status === 'none') {
        credential.value = null
      } else {
        credential.value = data
        maskedSecret.value = data.api_secret_masked || maskSecret(data.api_secret_tail)
        hasViewedSecret.value = data.id !== undefined && readViewed() === String(data.id)
      }
    } catch {
      credential.value = null
    } finally {
      loading.value = false
    }
  }

  const apply = async () => {
    submitting.value = true
    clear()
    try {
      await apiCredentialAPI.apply()
      show('success', t('personalCenter.apiPanel.applySuccess'))
      await load()
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.apiPanel.applyFailed')))
    } finally {
      submitting.value = false
    }
  }

  const generate = async (successKey: string) => {
    submitting.value = true
    clear()
    try {
      const data = (await apiCredentialAPI.regenerate()).data
      if (data?.api_secret) newSecret.value = data.api_secret
      if (credential.value?.id !== undefined) {
        try {
          localStorage.setItem(SECRET_VIEWED_KEY, String(credential.value.id))
        } catch {
          // storage unavailable
        }
        hasViewedSecret.value = true
      }
      await load()
      show('success', t(successKey))
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.apiPanel.regenerateFailed')))
    } finally {
      submitting.value = false
      confirmOpen.value = false
    }
  }

  const firstGenerate = () => generate('personalCenter.apiPanel.generateSuccess')
  const askRegenerate = () => {
    newSecret.value = ''
    confirmOpen.value = true
  }
  const confirmRegenerate = () => generate('personalCenter.apiPanel.regenerateSuccess')

  const toggleActive = async (next: boolean) => {
    if (!credential.value) return
    submitting.value = true
    clear()
    try {
      await apiCredentialAPI.updateStatus({ is_active: next })
      credential.value = { ...credential.value, is_active: next }
      show('success', next ? t('personalCenter.apiPanel.enabled') : t('personalCenter.apiPanel.disabled'))
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.apiPanel.toggleFailed')))
    } finally {
      submitting.value = false
    }
  }

  onMounted(() => void load())

  return {
    alert,
    loading,
    submitting,
    credential,
    newSecret,
    confirmOpen,
    hasViewedSecret,
    maskedSecret,
    apply,
    firstGenerate,
    askRegenerate,
    confirmRegenerate,
    toggleActive,
    reload: load,
  }
}
