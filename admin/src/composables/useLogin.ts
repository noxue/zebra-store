import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import i18n from '@/i18n'
import { useAdminAuthStore } from '@/stores/auth'
import { useAppStore } from '@/stores/app'
import type { CaptchaPayload } from '@/api/types'
import type { ImageCaptchaValue } from '@/components/captcha/ImageCaptcha'

/** Seconds left until an ISO expiry, never negative. */
export function secondsUntil(expiresAt: string, now = Date.now()): number {
  const ts = Date.parse(expiresAt)
  if (Number.isNaN(ts)) return 0
  return Math.max(0, Math.floor((ts - now) / 1000))
}

export function useLogin() {
  const t = i18n.global.t
  const router = useRouter()
  const auth = useAdminAuthStore()
  const app = useAppStore()

  const username = ref('')
  const password = ref('')
  const error = ref('')
  const imageCaptcha = ref<ImageCaptchaValue>({ captcha_id: '', captcha_code: '' })
  const turnstileToken = ref('')
  const totpCode = ref('')
  const recoveryCode = ref('')
  const useRecovery = ref(false)
  const remaining = ref(0)
  let timer: ReturnType<typeof setInterval> | undefined

  const captchaProvider = computed(() => {
    const cfg = app.config?.captcha
    return cfg?.scenes?.login ? String(cfg.provider || 'none') : 'none'
  })
  const turnstileSiteKey = computed(() => {
    const cfg = app.config?.captcha as { turnstile?: { site_key?: string } } | undefined
    const top = (app.config as { turnstile?: { site_key?: string } } | null)?.turnstile
    return cfg?.turnstile?.site_key || top?.site_key || ''
  })

  const stopTimer = () => {
    if (timer) clearInterval(timer)
    timer = undefined
  }
  const startTimer = () => {
    stopTimer()
    const tick = () => {
      remaining.value = secondsUntil(auth.challengeExpiresAt)
      if (remaining.value <= 0) {
        stopTimer()
        auth.clearChallenge()
        error.value = t('admin.login.totp.expired')
      }
    }
    tick()
    timer = setInterval(tick, 1000)
  }
  watch(
    () => auth.requiresTotp,
    (v) => (v ? startTimer() : stopTimer()),
  )
  onBeforeUnmount(stopTimer)

  const buildCaptcha = (): CaptchaPayload | undefined | null => {
    if (captchaProvider.value === 'image') {
      if (!imageCaptcha.value.captcha_code.trim()) return null
      return { captcha_id: imageCaptcha.value.captcha_id, captcha_code: imageCaptcha.value.captcha_code.trim() }
    }
    if (captchaProvider.value === 'turnstile') {
      if (!turnstileToken.value) return null
      return { turnstile_token: turnstileToken.value }
    }
    return undefined
  }

  const submitPassword = async (onCaptchaReset?: () => void) => {
    error.value = ''
    const captcha = buildCaptcha()
    if (captcha === null) {
      error.value = t('admin.login.captchaRequired')
      return
    }
    try {
      const res = await auth.login({ username: username.value.trim(), password: password.value, captcha_payload: captcha })
      if (!res.requiresTotp) await router.replace('/')
    } catch (err) {
      error.value = err instanceof Error ? err.message : t('admin.login.errors.invalidCredentials')
      onCaptchaReset?.()
    }
  }

  const submitTotp = async () => {
    error.value = ''
    if (useRecovery.value) {
      if (!recoveryCode.value.trim()) {
        error.value = t('admin.login.totp.recoveryRequired')
        return
      }
    } else if (!/^\d{6}$/.test(totpCode.value.trim())) {
      error.value = t('admin.login.totp.codeFormat')
      return
    }
    try {
      await auth.verify2FA(useRecovery.value ? { recovery_code: recoveryCode.value.trim() } : { code: totpCode.value.trim() })
      await router.replace('/')
    } catch (err) {
      error.value = err instanceof Error ? err.message : t('admin.login.totp.verifyFailed')
    }
  }

  const backToPassword = () => {
    auth.clearChallenge()
    totpCode.value = ''
    recoveryCode.value = ''
    error.value = ''
  }

  return {
    auth,
    app,
    username,
    password,
    error,
    imageCaptcha,
    turnstileToken,
    totpCode,
    recoveryCode,
    useRecovery,
    remaining,
    captchaProvider,
    turnstileSiteKey,
    submitPassword,
    submitTotp,
    backToPassword,
  }
}
