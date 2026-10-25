import { api } from './client'
import type { IssuedPersonalApiKey, PersonalApiKeyStatus } from './types'
export const personalApiKeyAPI = {
  status: () => api.get<PersonalApiKeyStatus | null>('/admin/api-key'),
  create: () => api.post<IssuedPersonalApiKey>('/admin/api-key', {}),
  rotate: () => api.put<IssuedPersonalApiKey>('/admin/api-key', {}),
  remove: () => api.delete<null>('/admin/api-key'),
}
