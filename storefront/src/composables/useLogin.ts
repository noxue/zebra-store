import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { userAuthAPI } from '@/api/auth'
import { useAppStore } from '@/stores/app'
import { useUserAuthStore } from '@/stores/userAuth'
import { detectGoogleIdentityUXMode } from '@/utils/auth/googleIdentity'
import {
  createGoogleRedirectIntent,
  createGoogleRedirectPreparedIntent,
  getSessionStorage,
  normalizeReturnPath,
  shouldResumeGoogleRedirect2FA,
  storeGoogleRedirectIntent,
  tryBuildGoogleRedirectCredentialCallbackURL,
  type GoogleRedirectPreparedIntent,
} from '@/utils/auth/googleRedirect'
import { buildTelegramMiniAppEntryLink, buildTelegramPayload, getTelegramMiniAppInitData, isTelegramUrlEnvironment, openTelegramCompatibleLink } from '@/utils/auth/telegram'
import { secondsUntil } from '@/utils/auth/validation'
import { useCaptcha } from './useCaptcha'
import { useFormValidation } from './useFormValidation'
import { usePageTitle } from './usePageTitle'

/** Global callback name used by the Telegram login widget (`data-onauth`). */
export const TELEGRAM_WIDGET_CALLBACK = '__zebraUserTelegramLogin'

type WidgetHost = Window & Record<string, unknown>

/** Login page logic: password login, 2FA step, Telegram / Google options. */
export function useLogin() {
  const router = useRouter()
  const route = useRoute()
  const { t } = useI18n()
  const appStore = useAppStore()
  const auth = useUserAuthStore()
  usePageTitle(() => t('auth.login.title'))

  const email = ref('')
  const password = ref('')
  const rememberMe = ref(true)
  const error = ref('')
  const info = ref('')

  const step = ref<'password' | 'totp'>(shouldResumeGoogleRedirect2FA(route.query.google2fa, auth.challengeToken) ? 'totp' : 'password')
  const totpMode = ref<'code' | 'recovery'>('code')
  const totpCode = ref('')
  const recoveryCode = ref('')
  const challengeRemainingSeconds = ref(0)
  let challengeTimer: ReturnType<typeof setInterval> | null = null

  const validation = useFormValidation<'email' | 'password'>(['email', 'password'])
  validation.addRule('email', validation.requiredRule())
  validation.addRule('email', validation.emailRule())
  validation.addRule('password', validation.requiredRule())

  const captcha = useCaptcha('login')

  // ---------------------------------------------------------- 3rd party config
  const telegramConfig = computed(() => appStore.config?.telegram_auth)
  const telegramBotUsername = computed(() => String(telegramConfig.value?.bot_username || '').trim())
  const telegramEnabled = computed(() => !!telegramConfig.value?.enabled && telegramBotUsername.value !== '')
  const telegramMode = computed(() => String(telegramConfig.value?.mode || '').trim())
  const isWidgetMode = computed(() => telegramMode.value === 'widget' || (telegramMode.value === '' && telegramEnabled.value))
  const miniAppInitData = ref(getTelegramMiniAppInitData())
  const isTelegramMiniApp = computed(() => miniAppInitData.value !== '' || isTelegramUrlEnvironment())
  const showTelegramWidget = computed(() => isWidgetMode.value && telegramEnabled.value && !isTelegramMiniApp.value)
  const showTelegramOidc = computed(() => telegramMode.value === 'oidc' && telegramEnabled.value && !isTelegramMiniApp.value)
  const showMiniAppLoginHint = computed(() => isTelegramMiniApp.value)
  const telegramMiniAppEntryLink = computed(() => buildTelegramMiniAppEntryLink(telegramBotUsername.value, String(telegramConfig.value?.mini_app_url || '')))
  const showTelegramMiniAppEntry = computed(() => !isTelegramMiniApp.value && telegramMiniAppEntryLink.value !== '')
  const attemptingMiniAppLogin = ref(false)
  let miniAppLoginAttempted = false

  const googleClientId = computed(() => String(appStore.config?.google_auth?.client_id || '').trim())
  const googleEnabled = computed(() => !!appStore.config?.google_auth?.enabled && googleClientId.value !== '')
  const googleUxMode = detectGoogleIdentityUXMode()
  const googleRedirectLoginUri = googleUxMode === 'redirect' ? tryBuildGoogleRedirectCredentialCallbackURL() : ''
  const googleRedirectAvailable = googleUxMode === 'popup' || googleRedirectLoginUri !== ''
  const showGoogleLogin = computed(() => googleEnabled.value && !isTelegramMiniApp.value && googleRedirectAvailable)
  const showThirdPartyLogin = computed(() => showTelegramWidget.value || showTelegramOidc.value || showMiniAppLoginHint.value || showGoogleLogin.value)

  const registrationEnabled = computed(() => appStore.config?.registration_enabled !== false)
  // Password reset needs a mailed code: the original hides "forgot password" when email verification is off.
  const emailVerificationEnabled = computed(() => appStore.config?.email_verification_enabled !== false)

  // ---------------------------------------------------------- flow helpers
  const redirectAfterLogin = () => router.push(normalizeReturnPath(route.query.redirect))

  const stopChallengeCountdown = () => {
    if (challengeTimer) clearInterval(challengeTimer)
    challengeTimer = null
  }

  const cancel2FA = () => {
    stopChallengeCountdown()
    auth.clearChallenge()
    step.value = 'password'
    totpCode.value = ''
    recoveryCode.value = ''
    challengeRemainingSeconds.value = 0
  }

  const startChallengeCountdown = () => {
    stopChallengeCountdown()
    const tick = () => {
      if (!auth.challengeExpiresAt) {
        challengeRemainingSeconds.value = 0
        stopChallengeCountdown()
        return
      }
      const left = secondsUntil(auth.challengeExpiresAt, appStore.getServerTime())
      challengeRemainingSeconds.value = left
      if (left <= 0) {
        cancel2FA()
        error.value = t('auth.login.totp.expired')
      }
    }
    tick()
    challengeTimer = setInterval(tick, 1000)
  }

  const enter2FAStep = () => {
    step.value = 'totp'
    totpMode.value = 'code'
    totpCode.value = ''
    recoveryCode.value = ''
    startChallengeCountdown()
  }

  const afterAuth = async (result: { requiresTotp: boolean }) => {
    if (result.requiresTotp) {
      enter2FAStep()
      return
    }
    await redirectAfterLogin()
  }

  // ---------------------------------------------------------- password login
  const handleLogin = async () => {
    if (auth.loading) return
    error.value = ''
    if (!validation.validateAll({ email: email.value, password: password.value })) return
    if (!captcha.isComplete()) {
      error.value = t('auth.common.captchaRequired')
      return
    }
    try {
      const result = await auth.login({
        email: email.value.trim(),
        password: password.value,
        remember_me: rememberMe.value,
        captcha_payload: captcha.build(),
      })
      await afterAuth(result)
    } catch (err) {
      error.value = errorMessage(err, t('auth.login.error'))
      captcha.reset()
    }
  }

  const handleVerify2FA = async () => {
    if (auth.loading) return
    error.value = ''
    const isCode = totpMode.value === 'code'
    const value = (isCode ? totpCode.value : recoveryCode.value).trim()
    if (!value) {
      error.value = isCode ? t('auth.login.totp.codeRequired') : t('auth.login.totp.recoveryRequired')
      return
    }
    try {
      await auth.verify2FA(isCode ? { code: value } : { recovery_code: value })
      stopChallengeCountdown()
      await redirectAfterLogin()
    } catch (err) {
      error.value = errorMessage(err, t('auth.login.totp.verifyFailed'))
      if (isCode) totpCode.value = ''
      else recoveryCode.value = ''
    }
  }

  const toggleTotpMode = () => {
    totpMode.value = totpMode.value === 'code' ? 'recovery' : 'code'
    error.value = ''
  }

  // ---------------------------------------------------------- telegram
  const handleTelegramAuth = async (raw: unknown) => {
    error.value = ''
    const payload = buildTelegramPayload(raw)
    if (!payload) {
      error.value = t('auth.login.telegramInvalidPayload')
      return
    }
    try {
      await afterAuth(await auth.telegramLogin(payload))
    } catch (err) {
      error.value = errorMessage(err, t('auth.login.telegramLoginFailed'))
    }
  }

  const handleTelegramWidgetError = () => {
    error.value = t('auth.login.telegramWidgetLoadFailed')
  }

  const startTelegramOidc = async () => {
    error.value = ''
    try {
      sessionStorage.removeItem('tg_oidc_intent')
      const redirect = typeof route.query.redirect === 'string' ? route.query.redirect : ''
      if (redirect) sessionStorage.setItem('tg_oidc_redirect', redirect)
      else sessionStorage.removeItem('tg_oidc_redirect')
      const res = await userAuthAPI.telegramOidcStart()
      const url = String(res.data?.auth_url || res.data?.url || res.data?.authorize_url || '')
      if (!url) {
        error.value = t('auth.login.telegramLoginFailed')
        return
      }
      window.location.href = url
    } catch (err) {
      error.value = errorMessage(err, t('auth.login.telegramLoginFailed'))
    }
  }

  const tryTelegramMiniAppLogin = async () => {
    if (!isTelegramMiniApp.value || !miniAppInitData.value || miniAppLoginAttempted || attemptingMiniAppLogin.value) return
    miniAppLoginAttempted = true
    attemptingMiniAppLogin.value = true
    error.value = ''
    try {
      await afterAuth(await auth.telegramMiniAppLogin(miniAppInitData.value))
    } catch (err) {
      error.value = errorMessage(err, t('auth.login.telegramLoginFailed'))
    } finally {
      attemptingMiniAppLogin.value = false
    }
  }

  const openTelegramMiniAppEntry = () => openTelegramCompatibleLink(telegramMiniAppEntryLink.value)

  // ---------------------------------------------------------- google
  const handleGoogleCredential = async (credential: string) => {
    if (auth.loading) return
    error.value = ''
    if (!credential.trim()) {
      error.value = t('auth.login.googleInvalidCredential')
      return
    }
    try {
      await afterAuth(await auth.googleLogin(credential.trim()))
    } catch (err) {
      error.value = errorMessage(err, t('auth.login.googleLoginFailed'))
    }
  }

  const handleGoogleError = () => {
    error.value = t('auth.login.googleWidgetLoadFailed')
  }

  const prepareGoogleRedirectLogin = async (): Promise<GoogleRedirectPreparedIntent> => {
    const res = await userAuthAPI.googleRedirectIntent()
    const prepared = createGoogleRedirectPreparedIntent(res.data)
    if (!prepared) throw new Error('Google redirect state is invalid')
    storeGoogleRedirectIntent(getSessionStorage(), createGoogleRedirectIntent('login', route.query.redirect, prepared.issuedAt))
    return prepared
  }

  // ---------------------------------------------------------- lifecycle
  onMounted(async () => {
    ;(window as unknown as WidgetHost)[TELEGRAM_WIDGET_CALLBACK] = (user: unknown) => void handleTelegramAuth(user)
    await appStore.loadConfig(true)
    const resumeTelegram2FA = route.query.tg2fa === '1' && !!auth.challengeToken
    const resumeGoogle2FA = shouldResumeGoogleRedirect2FA(route.query.google2fa, auth.challengeToken)
    if (resumeTelegram2FA || resumeGoogle2FA) {
      enter2FAStep()
      const next = { ...route.query }
      delete next.tg2fa
      delete next.google2fa
      void router.replace({ path: route.path, query: next })
    }
    if (route.query.reason === 'password_changed') {
      info.value = t('auth.login.passwordChangedTip')
      const next = { ...route.query }
      delete next.reason
      void router.replace({ path: route.path, query: next })
    }
    miniAppInitData.value = getTelegramMiniAppInitData()
    await tryTelegramMiniAppLogin()
  })

  watch(miniAppInitData, () => void tryTelegramMiniAppLogin())

  onBeforeUnmount(() => {
    delete (window as unknown as WidgetHost)[TELEGRAM_WIDGET_CALLBACK]
    stopChallengeCountdown()
  })

  return {
    auth,
    email,
    password,
    rememberMe,
    error,
    info,
    validation,
    captcha,
    step,
    totpMode,
    totpCode,
    recoveryCode,
    challengeRemainingSeconds,
    handleLogin,
    handleVerify2FA,
    toggleTotpMode,
    cancel2FA,
    registrationEnabled,
    emailVerificationEnabled,
    telegramBotUsername,
    showTelegramWidget,
    showTelegramOidc,
    showMiniAppLoginHint,
    attemptingMiniAppLogin,
    showTelegramMiniAppEntry,
    openTelegramMiniAppEntry,
    startTelegramOidc,
    handleTelegramWidgetError,
    googleClientId,
    googleUxMode,
    googleRedirectLoginUri,
    showGoogleLogin,
    showThirdPartyLogin,
    handleGoogleCredential,
    handleGoogleError,
    prepareGoogleRedirectLogin,
  }
}
