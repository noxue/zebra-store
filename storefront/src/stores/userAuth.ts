import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { userAuthAPI } from '@/api/auth'
import type {
  AuthUser,
  ForgotPasswordPayload,
  LoginPayload,
  LoginResult,
  RegisterPayload,
  SendVerifyCodePayload,
  TelegramAuthPayload,
  UserProfileData,
} from '@/api/types'

const readStoredUser = (): AuthUser | null => {
  try {
    const raw = localStorage.getItem('user_profile')
    if (!raw || raw === 'undefined') return null
    const parsed: unknown = JSON.parse(raw)
    return parsed && typeof parsed === 'object' ? (parsed as AuthUser) : null
  } catch {
    localStorage.removeItem('user_profile')
    return null
  }
}

export const useUserAuthStore = defineStore('user-auth', () => {
  const token = ref<string>(localStorage.getItem('user_token') || '')
  const user = ref<AuthUser | null>(readStoredUser())
  const loading = ref(false)
  const challengeToken = ref('')
  const challengeExpiresAt = ref('')

  const isAuthenticated = computed(() => !!token.value)

  const setToken = (value: string) => {
    token.value = value
    localStorage.setItem('user_token', value)
  }
  const setUser = (value: AuthUser) => {
    user.value = value
    localStorage.setItem('user_profile', JSON.stringify(value))
  }
  const clearAuth = () => {
    token.value = ''
    user.value = null
    localStorage.removeItem('user_token')
    localStorage.removeItem('user_profile')
  }
  const clearChallenge = () => {
    challengeToken.value = ''
    challengeExpiresAt.value = ''
  }

  const handleLoginResponse = (data: LoginResult | null | undefined): { requiresTotp: boolean } => {
    if (data?.requires_totp) {
      challengeToken.value = data.challenge_token || ''
      challengeExpiresAt.value = data.challenge_expires_at || ''
      return { requiresTotp: true }
    }
    clearChallenge()
    if (data?.token) setToken(data.token)
    if (data?.user) setUser(data.user)
    return { requiresTotp: false }
  }

  const withLoading = async <T>(fn: () => Promise<T>): Promise<T> => {
    loading.value = true
    try {
      return await fn()
    } finally {
      loading.value = false
    }
  }

  const sendVerifyCode = (payload: SendVerifyCodePayload) =>
    withLoading(async () => {
      await userAuthAPI.sendVerifyCode(payload)
      return true
    })

  const register = (payload: RegisterPayload) =>
    withLoading(async () => {
      const res = await userAuthAPI.register(payload)
      handleLoginResponse(res.data)
      return true
    })

  const login = (payload: LoginPayload) => withLoading(async () => handleLoginResponse((await userAuthAPI.login(payload)).data))

  const verify2FA = (payload: { code?: string; recovery_code?: string }) =>
    withLoading(async () => {
      if (!challengeToken.value) throw new Error('challenge_token_missing')
      const res = await userAuthAPI.verify2FA({ challenge_token: challengeToken.value, ...payload })
      handleLoginResponse({ ...res.data, requires_totp: false })
      clearChallenge()
      return true
    })

  const telegramLogin = (payload: TelegramAuthPayload) =>
    withLoading(async () => handleLoginResponse((await userAuthAPI.telegramLogin(payload)).data))
  const telegramOidcLogin = (payload: { code: string; state: string }) =>
    withLoading(async () => handleLoginResponse((await userAuthAPI.telegramOidcCallback(payload)).data))
  const telegramMiniAppLogin = (initData: string) =>
    withLoading(async () => handleLoginResponse((await userAuthAPI.telegramMiniAppLogin({ init_data: initData })).data))
  const googleLogin = (credential: string) =>
    withLoading(async () => handleLoginResponse((await userAuthAPI.googleLogin({ credential })).data))
  const googleRedirectLogin = () =>
    withLoading(async () => handleLoginResponse((await userAuthAPI.googleRedirectExchange()).data))

  const forgotPassword = (payload: ForgotPasswordPayload) =>
    withLoading(async () => {
      await userAuthAPI.forgotPassword(payload)
      return true
    })

  const syncUserProfile = (profile: Partial<UserProfileData>) => {
    const base: AuthUser = user.value || { id: profile.id || 0, email: profile.email || '' }
    setUser({ ...base, ...profile } as AuthUser)
  }

  /** Clears the session; navigation is left to the caller. */
  const logout = () => clearAuth()

  return {
    token,
    user,
    loading,
    challengeToken,
    challengeExpiresAt,
    isAuthenticated,
    sendVerifyCode,
    register,
    login,
    verify2FA,
    clearChallenge,
    telegramLogin,
    telegramOidcLogin,
    telegramMiniAppLogin,
    googleLogin,
    googleRedirectLogin,
    forgotPassword,
    syncUserProfile,
    logout,
    clearAuth,
  }
})
