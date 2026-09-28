import { defineStore } from 'pinia'
import { computed, ref, watchEffect } from 'vue'
import { configAPI } from '@/api/catalog'
import type { SiteConfig } from '@/api/types'
import { detectLocale, setI18nLocale } from '@/i18n'
import { setAffiliateClock } from '@/utils/affiliate'
import { applyCustomScripts } from '@/utils/customScripts'
import { getImageUrl } from '@/utils/image'
import { localizedText } from '@/utils/localized'
import { applyDefaultThemeMode } from '@/utils/theme'

const THEME_VAR_MAP: Array<[keyof NonNullable<SiteConfig['theme']>, string]> = [
  ['primary_color', '--zs-primary'],
  ['secondary_color', '--zs-secondary'],
  ['accent_color', '--zs-accent'],
]
const HEX_COLOR = /^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i

export const useAppStore = defineStore('app', () => {
  const locale = ref<string>(detectLocale())
  const config = ref<SiteConfig | null>(null)
  const loading = ref(false)
  const navigating = ref(false)
  let configRequest: Promise<void> | null = null
  /** serverTime = clientTime + offset (ms) */
  const serverTimeOffset = ref(0)
  /** Per-page title prefix, set through usePageTitle(). */
  const pageTitle = ref('')

  const siteName = computed(() => String(config.value?.brand?.site_name || '').trim() || 'Zebra Store')
  const siteLogo = computed(() => getImageUrl(config.value?.brand?.site_logo || ''))
  const siteIconHref = computed(() => {
    const icon = String(config.value?.brand?.site_icon || '').trim()
    return icon ? getImageUrl(icon) : '/favicon.svg'
  })
  const currency = computed(() => {
    const raw = String(config.value?.currency || '').trim().toUpperCase()
    return /^[A-Z]{3}$/.test(raw) ? raw : 'CNY'
  })
  const isResellerTenant = computed(() => String(config.value?.tenant?.mode || '').trim().toLowerCase() === 'reseller')
  const canAccessResellerConsole = computed(() => !!config.value && !isResellerTenant.value)
  const isListMode = computed(() => config.value?.template_mode === 'list')
  const sakuraEnabled = computed(() => config.value?.theme?.effects?.sakura !== false)
  const sparkleEnabled = computed(() => config.value?.theme?.effects?.sparkle !== false)
  const mascotImage = computed(() => getImageUrl(config.value?.theme?.mascot_image || ''))
  const heroBackground = computed(() => getImageUrl(config.value?.theme?.background_image || ''))
  const loginBackground = computed(() => getImageUrl(config.value?.theme?.login_background || ''))

  const setLocale = (next: string) => {
    locale.value = next
    try {
      localStorage.setItem('locale', next)
    } catch {
      // storage unavailable
    }
    void setI18nLocale(next)
  }

  const getServerTime = () => Date.now() + serverTimeOffset.value
  const getServerDate = () => new Date(getServerTime())
  setAffiliateClock(getServerTime)

  const applyThemeOverrides = (cfg: SiteConfig | null) => {
    if (typeof document === 'undefined') return
    const root = document.documentElement
    for (const [key, cssVar] of THEME_VAR_MAP) {
      const value = String(cfg?.theme?.[key] || '').trim()
      if (HEX_COLOR.test(value)) root.style.setProperty(cssVar, value)
      else root.style.removeProperty(cssVar)
    }
    applyDefaultThemeMode(cfg?.theme?.default_mode)
  }

  const fetchConfig = async (force: boolean) => {
    if (!force) loading.value = true
    try {
      const started = Date.now()
      const res = await configAPI.get()
      config.value = res.data
      if (typeof res.data?.server_time === 'number') {
        const now = Date.now()
        serverTimeOffset.value = res.data.server_time + (now - started) / 2 - now
      }
      applyThemeOverrides(config.value)
      applyCustomScripts(config.value?.scripts)
    } catch (error) {
      if (import.meta.env.DEV) console.warn('Failed to load config', error)
    } finally {
      if (!force) loading.value = false
    }
  }

  const loadConfig = (force = false): Promise<void> => {
    if (configRequest) return configRequest
    if (config.value && !force) return Promise.resolve()
    configRequest = fetchConfig(force).finally(() => { configRequest = null })
    return configRequest
  }

  // document head: title / favicon / meta
  if (typeof document !== 'undefined') {
    watchEffect(() => {
      document.documentElement.lang = locale.value
      const seoTitle = localizedText(config.value?.seo?.title, locale.value)
      const base = seoTitle || siteName.value
      document.title = pageTitle.value ? `${pageTitle.value} - ${siteName.value}` : base
      let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]')
      if (!link) {
        link = document.createElement('link')
        link.rel = 'icon'
        document.head.appendChild(link)
      }
      link.href = siteIconHref.value
      const setMeta = (name: string, content: string) => {
        let meta = document.querySelector<HTMLMetaElement>(`meta[name="${name}"]`)
        if (!content) {
          meta?.remove()
          return
        }
        if (!meta) {
          meta = document.createElement('meta')
          meta.name = name
          document.head.appendChild(meta)
        }
        meta.content = content
      }
      setMeta('keywords', localizedText(config.value?.seo?.keywords, locale.value))
      setMeta('description', localizedText(config.value?.seo?.description, locale.value))
    })
  }

  return {
    locale,
    config,
    loading,
    serverTimeOffset,
    navigating,
    pageTitle,
    siteName,
    siteLogo,
    siteIconHref,
    currency,
    isResellerTenant,
    canAccessResellerConsole,
    isListMode,
    sakuraEnabled,
    sparkleEnabled,
    mascotImage,
    heroBackground,
    loginBackground,
    setLocale,
    loadConfig,
    getServerTime,
    getServerDate,
  }
})
