import { defineStore } from 'pinia'
import { adminAPI } from '@/api/admin'
import type { PublicConfig } from '@/api/types'
import { applySiteIcon } from '@/utils/favicon'
import { getImageUrl } from '@/utils/image'

export type ThemeMode = 'light' | 'dark'
const THEME_KEY = 'admin_theme'

export const useAppStore = defineStore('app', {
  state: () => ({
    theme: 'light' as ThemeMode,
    config: null as PublicConfig | null,
    configLoaded: false,
  }),
  getters: {
    siteName: (s) => s.config?.brand?.site_name || '',
    siteLogo: (s) => getImageUrl(s.config?.brand?.site_logo || ''),
    siteUrl: (s) => s.config?.brand?.site_url || '',
    appVersion: (s) => s.config?.app_version || '',
    currency: (s) => s.config?.currency || 'CNY',
    mascotImage: (s) => getImageUrl(s.config?.theme?.mascot_image || ''),
    loginBackground: (s) => getImageUrl(s.config?.theme?.login_background || ''),
    sakuraEnabled: (s) => s.config?.theme?.effects?.sakura !== false,
    sparkleEnabled: (s) => s.config?.theme?.effects?.sparkle !== false,
  },
  actions: {
    initTheme() {
      let saved: string | null = null
      try {
        saved = localStorage.getItem(THEME_KEY)
      } catch {
        saved = null
      }
      if (saved === 'light' || saved === 'dark') {
        this.applyTheme(saved, false)
      } else {
        const prefersDark = window.matchMedia?.('(prefers-color-scheme: dark)').matches
        this.applyTheme(prefersDark ? 'dark' : 'light', false)
      }
    },
    applyTheme(mode: ThemeMode, persist = true) {
      this.theme = mode
      document.documentElement.classList.toggle('dark', mode === 'dark')
      if (persist) localStorage.setItem(THEME_KEY, mode)
    },
    toggleTheme() {
      this.applyTheme(this.theme === 'dark' ? 'light' : 'dark')
    },
    async loadConfig(force = false) {
      if (this.configLoaded && !force) return this.config
      try {
        const { data } = await adminAPI.getPublicConfig()
        this.config = data
        this.configLoaded = true
        applySiteIcon(data?.brand?.site_icon)
      } catch {
        this.configLoaded = true
      }
      return this.config
    },
  },
})
