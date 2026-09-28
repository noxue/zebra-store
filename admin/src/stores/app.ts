import { defineStore } from 'pinia'
import { adminAPI } from '@/api/admin'
import type { PublicConfig, SiteTheme } from '@/api/types'
import { applySiteIcon } from '@/utils/favicon'
import { getImageUrl } from '@/utils/image'

export type ThemeMode = 'light' | 'dark'
const THEME_KEY = 'admin_theme'

const COLOR_RE = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i

/** Apply `site_config.theme` colour overrides on :root (DESIGN.md: 主色可由后台配置覆盖). */
export function applyThemeOverrides(theme?: SiteTheme) {
  const root = document.documentElement
  const set = (name: string, value?: string) => {
    if (value && COLOR_RE.test(value)) root.style.setProperty(name, value)
    else root.style.removeProperty(name)
  }
  set('--zs-primary', theme?.primary_color)
  set('--zs-secondary', theme?.secondary_color)
  set('--zs-accent', theme?.accent_color)
  const bg = theme?.background_image ? getImageUrl(theme.background_image) : ''
  document.body.classList.toggle('zs-custom-bg', !!bg)
  if (bg) root.style.setProperty('--zs-custom-bg', `url("${bg.replace(/"/g, '%22')}")`)
  else root.style.removeProperty('--zs-custom-bg')
}

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
        applyThemeOverrides(data?.theme)
        const mode = data?.theme?.default_mode
        let saved: string | null = null
        try {
          saved = localStorage.getItem(THEME_KEY)
        } catch {
          saved = null
        }
        if (!saved && (mode === 'light' || mode === 'dark')) this.applyTheme(mode, false)
      } catch {
        this.configLoaded = true
      }
      return this.config
    },
  },
})
