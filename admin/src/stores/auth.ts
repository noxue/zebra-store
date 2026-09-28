import { defineStore } from 'pinia'
import { adminAPI } from '@/api/admin'
import type { AdminLoginChallengeResponse, AdminLoginRequest } from '@/api/types'
import { buildPermissionKeys, hasPermission } from '@/utils/permission'

const TOKEN_KEY = 'admin_token'
const ROLES_KEY = 'admin_roles'
const PERMISSIONS_KEY = 'admin_permissions'
const IS_SUPER_KEY = 'admin_is_super'

function readArrayStorage(key: string): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(key) || '[]')
    return Array.isArray(parsed) ? parsed.filter((item): item is string => typeof item === 'string') : []
  } catch {
    return []
  }
}

export const useAdminAuthStore = defineStore('adminAuth', {
  state: () => ({
    loading: false,
    token: localStorage.getItem(TOKEN_KEY) || '',
    isSuper: localStorage.getItem(IS_SUPER_KEY) === '1',
    roles: readArrayStorage(ROLES_KEY),
    permissions: readArrayStorage(PERMISSIONS_KEY),
    permissionsLoaded: false,
    adminId: 0,
    challengeToken: '',
    challengeExpiresAt: '',
    requiresTotp: false,
  }),
  actions: {
    setToken(token: string) {
      this.token = token
      localStorage.setItem(TOKEN_KEY, token)
    },

    async login(payload: AdminLoginRequest): Promise<{ requiresTotp: boolean }> {
      this.loading = true
      try {
        const { data } = await adminAPI.login(payload)
        if (data.requires_totp) {
          const challenge = data as AdminLoginChallengeResponse
          this.challengeToken = challenge.challenge_token
          this.challengeExpiresAt = challenge.challenge_expires_at
          this.requiresTotp = true
          return { requiresTotp: true }
        }
        this.setToken(data.token)
        this.clearChallenge()
        await this.loadAuthz()
        return { requiresTotp: false }
      } finally {
        this.loading = false
      }
    },

    async verify2FA(payload: { code?: string; recovery_code?: string }) {
      if (!this.challengeToken) throw new Error('No active 2FA challenge')
      this.loading = true
      try {
        const { data } = await adminAPI.verify2FA({ challenge_token: this.challengeToken, ...payload })
        this.setToken(data.token)
        this.clearChallenge()
        await this.loadAuthz()
      } finally {
        this.loading = false
      }
    },

    clearChallenge() {
      this.requiresTotp = false
      this.challengeToken = ''
      this.challengeExpiresAt = ''
    },

    async loadAuthz() {
      if (!this.token) {
        this.clearAuthzCache()
        return
      }
      const { data } = await adminAPI.getAuthzMe()
      this.adminId = data?.admin_id ?? 0
      this.isSuper = Boolean(data?.is_super)
      this.roles = Array.isArray(data?.roles) ? data.roles : []
      this.permissions = Array.isArray(data?.policies) ? buildPermissionKeys(data.policies) : []
      this.permissionsLoaded = true
      localStorage.setItem(IS_SUPER_KEY, this.isSuper ? '1' : '0')
      localStorage.setItem(ROLES_KEY, JSON.stringify(this.roles))
      localStorage.setItem(PERMISSIONS_KEY, JSON.stringify(this.permissions))
    },

    hasPermission(permission?: string) {
      return hasPermission(this.permissions, permission, this.isSuper)
    },

    clearAuthzCache() {
      this.isSuper = false
      this.roles = []
      this.permissions = []
      this.permissionsLoaded = false
      localStorage.removeItem(IS_SUPER_KEY)
      localStorage.removeItem(ROLES_KEY)
      localStorage.removeItem(PERMISSIONS_KEY)
    },

    logout() {
      this.token = ''
      localStorage.removeItem(TOKEN_KEY)
      this.clearAuthzCache()
      import('@/stores/compliance')
        .then(({ useComplianceStore }) => useComplianceStore().reset())
        .catch(() => undefined)
    },
  },
})
