import { userApi } from './client'
import type { IssuedPersonalApiKey, PersonalApiKeyStatus } from './types'
export const personalApiKeyAPI = {
  status: () => userApi.get<PersonalApiKeyStatus | null>('/me/api-key'),
  create: () => userApi.post<IssuedPersonalApiKey>('/me/api-key', {}),
  rotate: () => userApi.put<IssuedPersonalApiKey>('/me/api-key', {}),
  remove: () => userApi.delete<null>('/me/api-key'),
}
