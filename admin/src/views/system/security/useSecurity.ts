import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { TwoFAStatus } from '@/api/types'
import { useAdminAuthStore } from '@/stores/auth'
import { notifyError, notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import { adminUrl } from '@/utils/adminBase'

/** A 6-digit TOTP code. */
export const isTotpCode = (code: string) => /^\d{6}$/.test(code.trim())

/** Page logic for 安全设置: change password + 2FA status / enable / regenerate / disable. */
export function useSecurity() {
  const t = i18n.global.t
  const auth = useAdminAuthStore()

  const passwordSaving = ref(false)
  const passwordForm = reactive({ old: '', new: '', confirm: '' })

  const totpStatus = ref<TwoFAStatus | null>(null)
  const totpLoading = ref(false)
  const setupOpen = ref(false)
  const recoveryOpen = ref(false)
  const recoveryCodes = ref<string[]>([])
  const disableForm = reactive({ code: '', useRecovery: false, recoveryCode: '' })
  const regenForm = reactive({ code: '' })
  const regenerating = ref(false)
  const disabling = ref(false)

  const forceRelogin = () => {
    auth.logout()
    window.location.href = adminUrl('/login')
  }

  async function refreshTOTPStatus() {
    totpLoading.value = true
    try {
      totpStatus.value = (await adminAPI.get2FAStatus()).data ?? null
    } catch {
      /* already notified */
    } finally {
      totpLoading.value = false
    }
  }

  async function onEnabled(codes: string[]) {
    recoveryCodes.value = codes
    recoveryOpen.value = true
    await refreshTOTPStatus()
  }

  async function changePassword() {
    if (!passwordForm.old || !passwordForm.new || !passwordForm.confirm) {
      notifyError(t('admin.settings.alerts.passwordRequired'))
      return
    }
    if (passwordForm.new !== passwordForm.confirm) {
      notifyError(t('admin.settings.alerts.passwordMismatch'))
      return
    }
    if (!(await confirmAction(t('admin.settings.alerts.confirmChangePassword')))) return
    passwordSaving.value = true
    try {
      await adminAPI.updatePassword({ old_password: passwordForm.old, new_password: passwordForm.new })
      notifySuccess(t('admin.settings.alerts.passwordSuccess'))
      forceRelogin()
    } catch {
      /* already notified */
    } finally {
      passwordSaving.value = false
    }
  }

  async function submitRegen() {
    if (!isTotpCode(regenForm.code)) {
      notifyError(t('admin.twofa.regenerate.codeFormat'))
      return
    }
    regenerating.value = true
    try {
      const res = await adminAPI.regenerateRecoveryCodes({ code: regenForm.code.trim() })
      recoveryCodes.value = res.data?.recovery_codes ?? []
      recoveryOpen.value = true
      regenForm.code = ''
      await refreshTOTPStatus()
    } catch {
      /* already notified */
    } finally {
      regenerating.value = false
    }
  }

  async function submitDisable() {
    const value = disableForm.useRecovery ? disableForm.recoveryCode.trim() : disableForm.code.trim()
    if (!value) {
      notifyError(t('admin.twofa.disable.codeRequired'))
      return
    }
    if (!(await confirmAction({ description: t('admin.twofa.disable.confirm'), variant: 'destructive' }))) return
    disabling.value = true
    try {
      await adminAPI.disable2FA(disableForm.useRecovery ? { recovery_code: value } : { code: value })
      notifySuccess(t('admin.twofa.disable.success'))
      Object.assign(disableForm, { code: '', recoveryCode: '', useRecovery: false })
      forceRelogin()
    } catch {
      /* already notified */
    } finally {
      disabling.value = false
    }
  }

  return {
    passwordForm,
    passwordSaving,
    totpStatus,
    totpLoading,
    setupOpen,
    recoveryOpen,
    recoveryCodes,
    disableForm,
    regenForm,
    regenerating,
    disabling,
    refreshTOTPStatus,
    onEnabled,
    changePassword,
    submitRegen,
    submitDisable,
  }
}
