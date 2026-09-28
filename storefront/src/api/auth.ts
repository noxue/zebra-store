import { userApi } from './client'
import type {
  ForgotPasswordPayload,
  GoogleCredentialPayload,
  LoginPayload,
  LoginResult,
  OidcStartResult,
  RegisterPayload,
  SendVerifyCodePayload,
  TelegramAuthPayload,
  TelegramMiniAppAuthPayload,
  TotpEnableResult,
  TotpSetupResult,
  TotpStatus,
} from './types'

export const GOOGLE_REDIRECT_API_PATHS = {
  loginIntent: '/auth/google/redirect/intent',
  loginExchange: '/auth/google/redirect/exchange',
  bindIntent: '/me/google/redirect/intent',
  bindExchange: '/me/google/redirect/exchange',
} as const

export interface RedirectIntentResult {
  url?: string
  auth_url?: string
  redirect_url?: string
}

export const userAuthAPI = {
  sendVerifyCode: (data: SendVerifyCodePayload) => userApi.post<null>('/auth/send-verify-code', data),
  register: (data: RegisterPayload) => userApi.post<LoginResult>('/auth/register', data),
  login: (data: LoginPayload) => userApi.post<LoginResult>('/auth/login', data),
  verify2FA: (data: { challenge_token: string; code?: string; recovery_code?: string }) =>
    userApi.post<LoginResult>('/auth/login/verify-2fa', data),
  telegramLogin: (data: TelegramAuthPayload) => userApi.post<LoginResult>('/auth/telegram/login', data),
  telegramMiniAppLogin: (data: TelegramMiniAppAuthPayload) => userApi.post<LoginResult>('/auth/telegram/miniapp/login', data),
  telegramOidcStart: () => userApi.get<OidcStartResult>('/auth/telegram/oidc/start'),
  telegramOidcCallback: (data: { code: string; state: string }) => userApi.post<LoginResult>('/auth/telegram/oidc/callback', data),
  googleLogin: (data: GoogleCredentialPayload) => userApi.post<LoginResult>('/auth/google/login', data),
  googleRedirectIntent: () =>
    userApi.post<RedirectIntentResult>(GOOGLE_REDIRECT_API_PATHS.loginIntent, {}, { credentials: 'include' }),
  googleRedirectExchange: () =>
    userApi.post<LoginResult>(GOOGLE_REDIRECT_API_PATHS.loginExchange, {}, { credentials: 'include' }),
  forgotPassword: (data: ForgotPasswordPayload) => userApi.post<null>('/auth/forgot-password', data),
}

export const userTotpAPI = {
  status: () => userApi.get<TotpStatus>('/me/2fa/status'),
  setup: () => userApi.post<TotpSetupResult>('/me/2fa/setup', {}),
  enable: (data: { code: string }) => userApi.post<TotpEnableResult>('/me/2fa/enable', data),
  disable: (data: { code?: string; recovery_code?: string }) => userApi.post<null>('/me/2fa/disable', data),
  regenerateRecoveryCodes: (data: { code: string }) =>
    userApi.post<TotpEnableResult>('/me/2fa/recovery-codes/regenerate', data),
}
