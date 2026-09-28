import { userApi } from './client'
import { GOOGLE_REDIRECT_API_PATHS, type RedirectIntentResult } from './auth'
import type {
  ChangeEmailPayload,
  ChangeUserPasswordPayload,
  GoogleBindingData,
  GoogleCredentialPayload,
  OidcStartResult,
  PageParams,
  SendChangeEmailCodePayload,
  TelegramAuthPayload,
  TelegramBindingData,
  TelegramMiniAppAuthPayload,
  UpdateUserProfilePayload,
  UserLoginLogItem,
  UserProfileData,
} from './types'

export const userProfileAPI = {
  current: () => userApi.get<UserProfileData>('/me'),
  loginLogs: (params?: PageParams) => userApi.get<UserLoginLogItem[]>('/me/login-logs', { params }),
  updateProfile: (data: UpdateUserProfilePayload) => userApi.put<UserProfileData>('/me/profile', data),
  sendChangeEmailCode: (data: SendChangeEmailCodePayload) => userApi.post<null>('/me/email/send-verify-code', data),
  changeEmail: (data: ChangeEmailPayload) => userApi.post<UserProfileData>('/me/email/change', data),
  changePassword: (data: ChangeUserPasswordPayload) => userApi.put<null>('/me/password', data),
  getTelegramBinding: () => userApi.get<TelegramBindingData>('/me/telegram'),
  bindTelegram: (data: TelegramAuthPayload) => userApi.post<TelegramBindingData>('/me/telegram/bind', data),
  bindTelegramMiniApp: (data: TelegramMiniAppAuthPayload) => userApi.post<TelegramBindingData>('/me/telegram/miniapp/bind', data),
  telegramOidcBindStart: () => userApi.get<OidcStartResult>('/me/telegram/oidc/start'),
  telegramOidcBindCallback: (data: { code: string; state: string }) =>
    userApi.post<TelegramBindingData>('/me/telegram/oidc/callback', data),
  unbindTelegram: () => userApi.delete<null>('/me/telegram/unbind'),
  getGoogleBinding: () => userApi.get<GoogleBindingData>('/me/google'),
  bindGoogle: (data: GoogleCredentialPayload) => userApi.post<GoogleBindingData>('/me/google/bind', data),
  googleRedirectBindIntent: () =>
    userApi.post<RedirectIntentResult>(GOOGLE_REDIRECT_API_PATHS.bindIntent, {}, { credentials: 'include' }),
  googleRedirectBindExchange: () =>
    userApi.post<GoogleBindingData>(GOOGLE_REDIRECT_API_PATHS.bindExchange, {}, { credentials: 'include' }),
  unbindGoogle: () => userApi.delete<null>('/me/google/unbind'),
}
