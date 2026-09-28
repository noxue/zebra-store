import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { useAppStore } from '@/stores/app'
import { useUserAuthStore } from '@/stores/userAuth'
import { composeEmail, getEmailDomain, getPasswordStrength, normalizeEmailDomains } from '@/utils/auth/validation'
import { useCaptcha } from './useCaptcha'
import { useFormValidation } from './useFormValidation'
import { usePageTitle } from './usePageTitle'
import { useSendCodeCountdown } from './useSendCodeCountdown'

/** Minimum password length accepted by the backend on register. */
export const MIN_PASSWORD_LENGTH = 6

export function useRegister() {
  const router = useRouter()
  const { t } = useI18n()
  const appStore = useAppStore()
  const auth = useUserAuthStore()
  usePageTitle(() => t('auth.register.title'))

  const email = ref('')
  const emailLocalPart = ref('')
  const selectedEmailDomain = ref('')
  const password = ref('')
  const code = ref('')
  const agreed = ref(false)
  const error = ref('')
  const sending = ref(false)
  const { countdown, start: startCountdown } = useSendCodeCountdown()
  const captcha = useCaptcha('register_send_code')

  const passwordStrength = computed(() => getPasswordStrength(password.value))
  const registrationEnabled = computed(() => appStore.config?.registration_enabled !== false)
  const emailVerificationEnabled = computed(() => appStore.config?.email_verification_enabled !== false)
  const emailDomainAllowlistEnabled = computed(() => appStore.config?.email_domain_allowlist_enabled === true)
  const allowedEmailDomains = computed(() => normalizeEmailDomains(appStore.config?.allowed_email_domains))
  const allowedEmailDomainsText = computed(() => allowedEmailDomains.value.join(', '))
  const emailDomainSelectionRequired = computed(() => emailDomainAllowlistEnabled.value && allowedEmailDomains.value.length > 0)

  watch(
    allowedEmailDomains,
    (domains) => {
      if (domains.length === 0) selectedEmailDomain.value = ''
      else if (!domains.includes(selectedEmailDomain.value)) selectedEmailDomain.value = domains[0] || ''
    },
    { immediate: true },
  )

  const registrationEmail = computed(() =>
    emailDomainSelectionRequired.value ? composeEmail(emailLocalPart.value, selectedEmailDomain.value) : email.value.trim(),
  )

  const validation = useFormValidation<'email' | 'password'>(['email', 'password'])
  validation.addRule('email', validation.requiredRule())
  validation.addRule('email', validation.emailRule())
  validation.addRule('email', (value) => {
    if (!emailDomainAllowlistEnabled.value) return null
    const domain = getEmailDomain(value)
    if (!domain) return null
    if (allowedEmailDomains.value.length === 0) return t('auth.register.errors.emailDomainUnavailable')
    return allowedEmailDomains.value.includes(domain) ? null : t('auth.register.errors.emailDomainNotAllowed', { domains: allowedEmailDomainsText.value })
  })
  validation.addRule('password', validation.requiredRule())
  validation.addRule('password', validation.minLengthRule(MIN_PASSWORD_LENGTH))

  const touchEmail = () => validation.touchField('email', registrationEmail.value)
  const touchPassword = () => validation.touchField('password', password.value)

  const handleSendCode = async () => {
    error.value = ''
    const current = registrationEmail.value
    if (!current) {
      error.value = t('auth.register.errors.emailRequired')
      return
    }
    touchEmail()
    if (validation.hasError('email') || countdown.value > 0 || sending.value) return
    if (!captcha.isComplete()) {
      error.value = t('auth.common.captchaRequired')
      return
    }
    sending.value = true
    try {
      await auth.sendVerifyCode({ email: current, purpose: 'register', captcha_payload: captcha.build() })
      startCountdown()
    } catch (err) {
      error.value = errorMessage(err, t('auth.register.errors.sendCodeFailed'))
      captcha.reset()
    } finally {
      sending.value = false
    }
  }

  const handleRegister = async () => {
    if (auth.loading) return
    error.value = ''
    const current = registrationEmail.value
    if (!validation.validateAll({ email: current, password: password.value })) return
    if (emailVerificationEnabled.value && !code.value.trim()) {
      error.value = t('formValidation.required')
      return
    }
    if (!agreed.value) {
      error.value = t('auth.register.errors.agreementRequired')
      return
    }
    try {
      await auth.register({
        email: current,
        password: password.value,
        code: emailVerificationEnabled.value ? code.value.trim() : '',
        agreement_accepted: agreed.value,
      })
      await router.push('/me/orders')
    } catch (err) {
      error.value = errorMessage(err, t('auth.register.errors.registerFailed'))
    }
  }

  onMounted(() => void appStore.loadConfig(true))

  return {
    auth,
    email,
    emailLocalPart,
    selectedEmailDomain,
    password,
    code,
    agreed,
    error,
    sending,
    countdown,
    captcha,
    passwordStrength,
    registrationEnabled,
    emailVerificationEnabled,
    emailDomainAllowlistEnabled,
    allowedEmailDomains,
    allowedEmailDomainsText,
    emailDomainSelectionRequired,
    validation,
    touchEmail,
    touchPassword,
    handleSendCode,
    handleRegister,
  }
}
