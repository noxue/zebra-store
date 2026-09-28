import { onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { userProfileAPI } from '@/api/user'
import { useUserAuthStore } from '@/stores/userAuth'
import {
  consumeGoogleRedirectIntent,
  createGoogleRedirectIntent,
  getSessionStorage,
  parseGoogleRedirectCallbackQuery,
} from '@/utils/auth/googleRedirect'

/**
 * Google GIS redirect callback. Identity data is exchanged only via the
 * backend's one-time HttpOnly handoff cookie; the URL carries flow/error only.
 */
export function useGoogleRedirectCallback() {
  const { t } = useI18n()
  const route = useRoute()
  const router = useRouter()
  const auth = useUserAuthStore()
  const loading = ref(true)
  const errMsg = ref('')
  const retryPath = ref('/auth/login')

  const fail = (message?: string) => {
    errMsg.value = message || t('auth.googleCallback.failed')
    loading.value = false
  }

  onMounted(async () => {
    const callback = parseGoogleRedirectCallbackQuery(route.query as Record<string, unknown>)
    const stored = consumeGoogleRedirectIntent(getSessionStorage())
    if (!callback) return fail()
    retryPath.value = callback.flow === 'bind' && auth.isAuthenticated ? '/me/security' : '/auth/login'
    if (callback.error) return fail()
    const intent = stored?.flow === callback.flow ? stored : createGoogleRedirectIntent(callback.flow, undefined)
    try {
      if (callback.flow === 'bind') {
        await userProfileAPI.googleRedirectBindExchange()
        await router.replace({ path: '/me/security', query: { googleBound: '1' } })
        return
      }
      const result = await auth.googleRedirectLogin()
      if (result.requiresTotp) {
        await router.replace({ path: '/auth/login', query: { google2fa: '1', redirect: intent.returnPath } })
        return
      }
      await router.replace(intent.returnPath)
    } catch (err) {
      fail(errorMessage(err, t('auth.googleCallback.failed')))
    }
  })

  return { loading, errMsg, retryPath }
}
