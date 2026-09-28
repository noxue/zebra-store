import { onMounted, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { userTotpAPI } from '@/api/auth'
import { errorMessage } from '@/api/client'
import type { TotpEnableWithTokenResult, TotpSetupResult, TotpStatus } from '@/api/types'
import { useUserAuthStore } from '@/stores/userAuth'
import { usePanelAlert } from './usePanelAlert'

export type TwoFactorMode = 'idle' | 'disable' | 'regenerate'

/** TOTP 2FA: status, setup + enable, disable (code/recovery), regenerate recovery codes. */
export function useTwoFactor() {
  const { t } = useI18n()
  const auth = useUserAuthStore()
  const { alert, show, clear } = usePanelAlert()
  const status = ref<TotpStatus | null>(null)
  const setup = ref<TotpSetupResult | null>(null)
  const enableCode = ref('')
  const recoveryCodes = ref<string[]>([])
  const loading = ref(false)
  const mode = ref<TwoFactorMode>('idle')
  const disableWith = ref<'code' | 'recovery'>('code')
  const disableCode = ref('')
  const disableRecovery = ref('')
  const regenerateCode = ref('')

  const refresh = async () => {
    try {
      status.value = (await userTotpAPI.status()).data
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.security.twofa.loadFailed')))
    }
  }

  const busy = async (fn: () => Promise<void>, failKey: string) => {
    loading.value = true
    try {
      await fn()
    } catch (err) {
      show('error', errorMessage(err, t(failKey)))
    } finally {
      loading.value = false
    }
  }

  const startSetup = () => {
    clear()
    return busy(async () => {
      setup.value = (await userTotpAPI.setup()).data
      enableCode.value = ''
    }, 'personalCenter.security.twofa.setupFailed')
  }

  const cancelSetup = () => {
    setup.value = null
    enableCode.value = ''
    clear()
  }

  const submitEnable = () => {
    clear()
    const code = enableCode.value.trim()
    if (!code) return show('warning', t('personalCenter.security.twofa.codeRequired'))
    return busy(async () => {
      const data = (await userTotpAPI.enable({ code })).data as TotpEnableWithTokenResult | null
      recoveryCodes.value = data?.recovery_codes || []
      if (data?.token) {
        // enabling 2FA rotates the session token
        localStorage.setItem('user_token', data.token)
        auth.token = data.token
      }
      setup.value = null
      enableCode.value = ''
      show('success', t('personalCenter.security.twofa.enableSuccess'))
      await refresh()
    }, 'personalCenter.security.twofa.enableFailed')
  }

  const openMode = (next: TwoFactorMode) => {
    mode.value = next
    disableWith.value = 'code'
    disableCode.value = ''
    disableRecovery.value = ''
    regenerateCode.value = ''
    clear()
  }

  const submitDisable = () => {
    clear()
    const payload: { code?: string; recovery_code?: string } = {}
    if (disableWith.value === 'code') {
      if (!disableCode.value.trim()) return show('warning', t('personalCenter.security.twofa.codeRequired'))
      payload.code = disableCode.value.trim()
    } else {
      if (!disableRecovery.value.trim()) return show('warning', t('personalCenter.security.twofa.recoveryRequired'))
      payload.recovery_code = disableRecovery.value.trim()
    }
    return busy(async () => {
      await userTotpAPI.disable(payload)
      openMode('idle')
      show('success', t('personalCenter.security.twofa.disableSuccess'))
      await refresh()
    }, 'personalCenter.security.twofa.disableFailed')
  }

  const submitRegenerate = () => {
    clear()
    const code = regenerateCode.value.trim()
    if (!code) return show('warning', t('personalCenter.security.twofa.codeRequired'))
    return busy(async () => {
      recoveryCodes.value = (await userTotpAPI.regenerateRecoveryCodes({ code })).data?.recovery_codes || []
      openMode('idle')
      show('success', t('personalCenter.security.twofa.regenerateSuccess'))
      await refresh()
    }, 'personalCenter.security.twofa.regenerateFailed')
  }

  const acknowledgeRecovery = () => {
    recoveryCodes.value = []
  }

  onMounted(() => void refresh())

  return {
    alert,
    status,
    setup,
    enableCode,
    recoveryCodes,
    loading,
    mode,
    disableWith,
    disableCode,
    disableRecovery,
    regenerateCode,
    startSetup,
    cancelSetup,
    submitEnable,
    openMode,
    submitDisable,
    submitRegenerate,
    acknowledgeRecovery,
  }
}
