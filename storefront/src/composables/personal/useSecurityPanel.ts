import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { userProfileAPI } from '@/api/user'
import { useAppStore } from '@/stores/app'
import { useUserAuthStore } from '@/stores/userAuth'
import { useUserProfileStore } from '@/stores/userProfile'
import { formatDateTime } from '@/utils/format'
import { buildTelegramPayload } from '@/utils/personal'
import { usePanelAlert } from './usePanelAlert'

/** Code resend cooldown (seconds), same as original. */
const CODE_COOLDOWN_SECONDS = 60
const TELEGRAM_CALLBACK = '__zsSecurityTelegramBind'

declare global {
  interface Window {
    [TELEGRAM_CALLBACK]?: (user: unknown) => void
  }
}

/** Security center: email change/bind, password, Telegram & Google bindings. */
export function useSecurityPanel() {
  const { t } = useI18n()
  const router = useRouter()
  const appStore = useAppStore()
  const auth = useUserAuthStore()
  const store = useUserProfileStore()
  const { alert, show, clear } = usePanelAlert()

  // ---------- email
  const emailForm = reactive({ newEmail: '', oldCode: '', newCode: '' })
  const oldCooldown = ref(0)
  const newCooldown = ref(0)
  let timer: ReturnType<typeof setInterval> | null = null

  const requiresOldEmailCode = computed(() => (store.profile?.email_change_mode || 'change_with_old_and_new') !== 'bind_only')
  const requiresOldPassword = computed(() => (store.profile?.password_change_mode || 'change_with_old') !== 'set_without_old')
  const currentEmailDisplay = computed(() =>
    requiresOldEmailCode.value ? store.profile?.email || '' : t('personalCenter.security.bindOnlyEmailDisplay'),
  )

  const startCooldown = (kind: 'old' | 'new') => {
    if (kind === 'old') oldCooldown.value = CODE_COOLDOWN_SECONDS
    else newCooldown.value = CODE_COOLDOWN_SECONDS
    if (timer) return
    timer = setInterval(() => {
      if (oldCooldown.value > 0) oldCooldown.value -= 1
      if (newCooldown.value > 0) newCooldown.value -= 1
      if (oldCooldown.value === 0 && newCooldown.value === 0 && timer) {
        clearInterval(timer)
        timer = null
      }
    }, 1000)
  }

  const sendOldCode = async () => {
    clear()
    if (!requiresOldEmailCode.value) return show('warning', t('personalCenter.security.bindOnlyOldCodeDisabled'))
    const ok = await store.sendChangeEmailCode({ kind: 'old' })
    if (!ok) return show('error', store.securityError || t('personalCenter.security.sendCodeFailed'))
    startCooldown('old')
    show('success', t('personalCenter.security.sendOldCodeSuccess'))
  }

  const sendNewCode = async () => {
    clear()
    const email = emailForm.newEmail.trim()
    if (!email) return show('warning', t('personalCenter.security.newEmailRequired'))
    const ok = await store.sendChangeEmailCode({ kind: 'new', new_email: email })
    if (!ok) return show('error', store.securityError || t('personalCenter.security.sendCodeFailed'))
    startCooldown('new')
    show('success', t('personalCenter.security.sendNewCodeSuccess'))
  }

  const submitEmail = async () => {
    clear()
    const needOld = requiresOldEmailCode.value
    const payload = {
      new_email: emailForm.newEmail.trim(),
      new_code: emailForm.newCode.trim(),
      ...(needOld ? { old_code: emailForm.oldCode.trim() } : {}),
    }
    if (!payload.new_email || !payload.new_code || (needOld && !payload.old_code)) {
      return show('warning', needOld ? t('personalCenter.security.changeEmailRequired') : t('personalCenter.security.bindEmailRequired'))
    }
    const ok = await store.changeEmail(payload)
    if (!ok) return show('error', store.securityError || t('personalCenter.security.changeEmailFailed'))
    emailForm.newEmail = ''
    emailForm.oldCode = ''
    emailForm.newCode = ''
    oldCooldown.value = 0
    newCooldown.value = 0
    show('success', needOld ? t('personalCenter.security.changeEmailSuccess') : t('personalCenter.security.bindEmailSuccess'))
  }

  // ---------- password
  const passwordForm = reactive({ oldPassword: '', newPassword: '', confirmPassword: '' })
  const submitPassword = async () => {
    clear()
    const needOld = requiresOldPassword.value
    const oldPassword = passwordForm.oldPassword.trim()
    const newPassword = passwordForm.newPassword.trim()
    const confirm = passwordForm.confirmPassword.trim()
    if (!newPassword || !confirm || (needOld && !oldPassword)) {
      return show('warning', needOld ? t('personalCenter.security.changePasswordRequired') : t('personalCenter.security.setPasswordRequired'))
    }
    if (newPassword !== confirm) return show('warning', t('personalCenter.security.passwordMismatch'))
    const ok = await store.changePassword({ ...(needOld ? { old_password: oldPassword } : {}), new_password: newPassword })
    if (!ok) return show('error', store.securityError || t('personalCenter.security.changePasswordFailed'))
    passwordForm.oldPassword = ''
    passwordForm.newPassword = ''
    passwordForm.confirmPassword = ''
    show('success', needOld ? t('personalCenter.security.changePasswordSuccess') : t('personalCenter.security.setPasswordSuccess'))
    auth.logout()
    void router.push('/auth/login?reason=password_changed')
  }

  // ---------- telegram
  const telegramConfig = computed(() => appStore.config?.telegram_auth)
  const telegramBotUsername = computed(() => String(telegramConfig.value?.bot_username || '').trim())
  const telegramMode = computed(() => String(telegramConfig.value?.mode || '').trim())
  const telegramEnabled = computed(() => !!telegramConfig.value?.enabled && telegramBotUsername.value !== '')
  const telegramBound = computed(() => !!store.telegramBinding?.bound)
  const showTelegramWidget = computed(() => telegramEnabled.value && !telegramBound.value && telegramMode.value !== 'oidc')
  const showTelegramOidc = computed(() => telegramEnabled.value && !telegramBound.value && telegramMode.value === 'oidc')
  const canUnbindTelegram = computed(() => store.telegramBinding?.can_unbind === true)
  const telegramDisplayName = computed(() =>
    store.telegramBinding?.username ? `@${store.telegramBinding.username}` : t('personalCenter.security.telegramDisplayFallback'),
  )

  const refreshBindings = async () => {
    const [a, b] = await Promise.all([store.loadTelegramBinding(), store.loadGoogleBinding()])
    return a && b
  }
  const finishMutation = async (message: string) => {
    const ok = await refreshBindings()
    if (ok) show('success', message)
    else show('warning', t('personalCenter.security.externalIdentityRefreshFailed'))
  }

  const handleTelegramBind = async (raw: unknown) => {
    clear()
    const payload = buildTelegramPayload(raw)
    if (!payload) return show('warning', t('personalCenter.security.telegramInvalidPayload'))
    const ok = await store.bindTelegram(payload)
    if (!ok) return show('error', store.securityError || t('personalCenter.security.telegramBindFailed'))
    await finishMutation(t('personalCenter.security.telegramBindSuccess'))
  }

  /** Mounts the Telegram login widget into `el` (bind mode). */
  const mountTelegramWidget = (el: HTMLElement | null) => {
    if (!el) return
    el.innerHTML = ''
    if (!showTelegramWidget.value) return
    window[TELEGRAM_CALLBACK] = (user: unknown) => void handleTelegramBind(user)
    const script = document.createElement('script')
    script.async = true
    script.src = 'https://telegram.org/js/telegram-widget.js?22'
    script.setAttribute('data-telegram-login', telegramBotUsername.value)
    script.setAttribute('data-size', 'large')
    script.setAttribute('data-userpic', 'false')
    script.setAttribute('data-request-access', 'write')
    script.setAttribute('data-onauth', `${TELEGRAM_CALLBACK}(user)`)
    script.onerror = () => show('error', t('personalCenter.security.telegramWidgetLoadFailed'))
    el.appendChild(script)
  }

  const startTelegramOidcBind = async () => {
    clear()
    try {
      sessionStorage.setItem('tg_oidc_intent', 'bind')
      const data = (await userProfileAPI.telegramOidcBindStart()).data
      const url = String(data?.auth_url || data?.url || data?.authorize_url || '')
      if (!url) return show('error', t('personalCenter.security.telegramOidcBindFailed'))
      window.location.href = url
    } catch (err) {
      show('error', errorMessage(err, t('personalCenter.security.telegramOidcBindFailed')))
    }
  }

  const unbindTelegram = async () => {
    clear()
    if (!canUnbindTelegram.value) return show('warning', t('personalCenter.security.telegramUnbindDisabledTip'))
    const ok = await store.unbindTelegram()
    if (!ok) return show('error', store.securityError || t('personalCenter.security.telegramUnbindFailed'))
    await finishMutation(t('personalCenter.security.telegramUnbindSuccess'))
  }

  // ---------- google
  const googleClientId = computed(() => String(appStore.config?.google_auth?.client_id || '').trim())
  const googleEnabled = computed(() => !!appStore.config?.google_auth?.enabled && googleClientId.value !== '')
  const googleBound = computed(() => !!store.googleBinding?.bound)
  const canUnbindGoogle = computed(() => store.googleBinding?.can_unbind === true)
  const googleDisplayName = computed(() => {
    const b = store.googleBinding
    return (
      String(b?.display_name || '').trim() ||
      String(b?.email || '').trim() ||
      String(b?.username || '').trim() ||
      t('personalCenter.security.googleDisplayFallback')
    )
  })

  const handleGoogleCredential = async (credential: string) => {
    if (store.bindingGoogle) return
    clear()
    if (!credential.trim()) return show('warning', t('personalCenter.security.googleInvalidCredential'))
    const ok = await store.bindGoogle(credential.trim())
    if (!ok) return show('error', store.securityError || t('personalCenter.security.googleBindFailed'))
    await finishMutation(t('personalCenter.security.googleBindSuccess'))
  }
  const handleGoogleScriptError = () => show('error', t('personalCenter.security.googleWidgetLoadFailed'))

  const unbindGoogle = async () => {
    clear()
    if (!canUnbindGoogle.value) return show('warning', t('personalCenter.security.googleUnbindDisabledTip'))
    const ok = await store.unbindGoogle()
    if (!ok) return show('error', store.securityError || t('personalCenter.security.googleUnbindFailed'))
    await finishMutation(t('personalCenter.security.googleUnbindSuccess'))
  }

  const bindingTime = (value?: string | null) => formatDateTime(value)

  onMounted(() => {
    void refreshBindings()
  })
  onBeforeUnmount(() => {
    if (timer) clearInterval(timer)
    delete window[TELEGRAM_CALLBACK]
  })

  return {
    store,
    alert,
    emailForm,
    oldCooldown,
    newCooldown,
    requiresOldEmailCode,
    requiresOldPassword,
    currentEmailDisplay,
    sendOldCode,
    sendNewCode,
    submitEmail,
    passwordForm,
    submitPassword,
    telegramEnabled,
    telegramBound,
    showTelegramWidget,
    showTelegramOidc,
    canUnbindTelegram,
    telegramDisplayName,
    mountTelegramWidget,
    startTelegramOidcBind,
    unbindTelegram,
    googleClientId,
    googleEnabled,
    googleBound,
    canUnbindGoogle,
    googleDisplayName,
    handleGoogleCredential,
    handleGoogleScriptError,
    unbindGoogle,
    bindingTime,
  }
}
