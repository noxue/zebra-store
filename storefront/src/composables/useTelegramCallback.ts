import { onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { userProfileAPI } from '@/api/user'
import { useUserAuthStore } from '@/stores/userAuth'
import { normalizeReturnPath } from '@/utils/auth/googleRedirect'

/**
 * Telegram OIDC callback. `sessionStorage.tg_oidc_intent === 'bind'` (set by
 * the security panel) selects the bind flow, otherwise it is a login.
 */
export function useTelegramCallback() {
  const { t } = useI18n()
  const route = useRoute()
  const router = useRouter()
  const auth = useUserAuthStore()
  const loading = ref(true)
  const errMsg = ref('')
  const retryPath = ref('/auth/login')

  onMounted(async () => {
    const code = String(route.query.code || '')
    const state = String(route.query.state || '')
    const oauthError = String(route.query.error || '')
    let intent = 'login'
    let savedRedirect = ''
    try {
      intent = sessionStorage.getItem('tg_oidc_intent') || 'login'
      savedRedirect = sessionStorage.getItem('tg_oidc_redirect') || ''
      sessionStorage.removeItem('tg_oidc_intent')
      sessionStorage.removeItem('tg_oidc_redirect')
    } catch {
      // storage unavailable → plain login flow
    }
    if (intent === 'bind') retryPath.value = '/me/security'
    if (oauthError || !code || !state) {
      errMsg.value = t('auth.telegramCallback.failed')
      loading.value = false
      return
    }
    try {
      if (intent === 'bind') {
        await userProfileAPI.telegramOidcBindCallback({ code, state })
        await router.replace({ path: '/me/security', query: { tgBound: '1' } })
        return
      }
      const result = await auth.telegramOidcLogin({ code, state })
      if (result.requiresTotp) {
        const query: Record<string, string> = { tg2fa: '1' }
        if (savedRedirect) query.redirect = savedRedirect
        await router.replace({ path: '/auth/login', query })
        return
      }
      await router.replace(normalizeReturnPath(savedRedirect))
    } catch (err) {
      errMsg.value = errorMessage(err, t('auth.telegramCallback.failed'))
      loading.value = false
    }
  })

  return { loading, errMsg, retryPath }
}
