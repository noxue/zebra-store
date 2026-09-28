import { userApi } from './client'
import type {
  ApiCompatKeyData,
  ApiCompatKeyUpdate,
  ApiConnectionCodeResult,
  ApiCredentialData,
  ApiCredentialRegenerateResult,
  ApiCredentialRotateResult,
} from './types'

export const apiCredentialAPI = {
  getMy: () => userApi.get<ApiCredentialData>('/api-credential'),
  apply: () => userApi.post<ApiCredentialData>('/api-credential/apply'),
  regenerate: () => userApi.post<ApiCredentialRegenerateResult>('/api-credential/regenerate'),
  /** 一键对接: new connection code (rotates the secret; old one stays valid for the grace period). */
  createConnectionCode: () => userApi.post<ApiConnectionCodeResult>('/api-credential/connection-code'),
  /** Dual-secret rotation: returns the new secret once. */
  rotate: () => userApi.post<ApiCredentialRotateResult>('/api-credential/rotate'),
  updateStatus: (data: { is_active: boolean }) => userApi.put<ApiCredentialData>('/api-credential/status', data),
  /** 异次元 / 萌次元 compat key (app_id + app_key); `app_key` is empty until issued. */
  getCompat: () => userApi.get<ApiCompatKeyData>('/api-credential/compat'),
  /** Issues a new app_key (the previous one stops working at once) and enables compat access. */
  issueCompat: () => userApi.post<ApiCompatKeyData>('/api-credential/compat/issue'),
  updateCompat: (data: ApiCompatKeyUpdate) => userApi.put<ApiCompatKeyData>('/api-credential/compat', data),
}
