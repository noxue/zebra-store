import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { useAppStore } from '@/stores/app'
import { useUserAuthStore } from '@/stores/userAuth'
import { toast } from './useToast'
import { useCaptcha } from './useCaptcha'
import { usePageTitle } from './usePageTitle'
import { useSendCodeCountdown } from './useSendCodeCountdown'

export function useForgot() {
  const router = useRouter()
  const { t } = useI18n()
  const appStore = useAppStore()
  const auth = useUserAuthStore()
  usePageTitle(() => t('auth.forgot.title'))

  const emailVerificationEnabled = computed(() => appStore.config?.email_verification_enabled !== false)
  const email = ref('')
  const code = ref('')
  const newPassword = ref('')
  const error = ref('')
  const sending = ref(false)
  const { countdown, start: startCountdown } = useSendCodeCountdown()
  const captcha = useCaptcha('reset_send_code')

  const handleSendCode = async () => {
    error.value = ''
    if (!email.value.trim()) {
      error.value = t('auth.forgot.errors.emailRequired')
      return
    }
    if (countdown.value > 0 || sending.value) return
    if (!captcha.isComplete()) {
      error.value = t('auth.common.captchaRequired')
      return
    }
    sending.value = true
    try {
      await auth.sendVerifyCode({ email: email.value.trim(), purpose: 'reset', captcha_payload: captcha.build() })
      startCountdown()
    } catch (err) {
      error.value = errorMessage(err, t('auth.forgot.errors.sendCodeFailed'))
      captcha.reset()
    } finally {
      sending.value = false
    }
  }

  const handleReset = async () => {
    if (auth.loading) return
    error.value = ''
    if (!email.value.trim() || !code.value.trim() || !newPassword.value) {
      error.value = t('formValidation.required')
      return
    }
    try {
      await auth.forgotPassword({ email: email.value.trim(), code: code.value.trim(), new_password: newPassword.value })
      toast.success(t('zsContent.resetSuccess'))
      await router.push('/auth/login')
    } catch (err) {
      error.value = errorMessage(err, t('auth.forgot.errors.resetFailed'))
    }
  }

  onMounted(() => void appStore.loadConfig(true))

  return { auth, emailVerificationEnabled, email, code, newPassword, error, sending, countdown, captcha, handleSendCode, handleReset }
}
