import { computed, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { ApiResponse } from '@/api/types'
import { useSiteSettings } from './useSiteSettings'
import { useSmtpSettings } from './useSmtpSettings'
import { useCaptchaSettings } from './useCaptchaSettings'
import { useTelegramAuthSettings } from './useTelegramAuthSettings'
import { useGoogleAuthSettings } from './useGoogleAuthSettings'
import { useDashboardSettings } from './useDashboardSettings'
import { useOrderEmailTemplateSettings } from './useOrderEmailTemplateSettings'
import { useNavigationSettings } from './useNavigationSettings'
import { useHomeAnnouncementSettings } from './useHomeAnnouncementSettings'
import { useUpstreamSyncSettings } from './useUpstreamSyncSettings'
import type { LangCode } from './settingsUtils'

export const SETTINGS_TABS = [
  'basic',
  'template',
  'navigation',
  'about',
  'legal',
  'home_announcement',
  'smtp',
  'order_email_template',
  'captcha',
  'telegram',
  'google',
  'dashboard',
  'upstream_sync',
] as const
export type SettingsTab = (typeof SETTINGS_TABS)[number]

export const SETTINGS_TAB_LABEL_KEYS: Record<SettingsTab, string> = {
  basic: 'admin.settings.tabs.basic',
  template: 'admin.settings.tabs.template',
  navigation: 'admin.settings.tabs.navigation',
  about: 'admin.settings.tabs.about',
  legal: 'admin.settings.tabs.legal',
  home_announcement: 'admin.settings.tabs.homeAnnouncement',
  smtp: 'admin.settings.tabs.smtp',
  order_email_template: 'admin.settings.tabs.orderEmailTemplate',
  captcha: 'admin.settings.tabs.captcha',
  telegram: 'admin.settings.tabs.telegram',
  google: 'admin.settings.tabs.google',
  dashboard: 'admin.settings.tabs.dashboard',
  upstream_sync: 'admin.settings.tabs.upstreamSync',
}

export const isSettingsTab = (v: unknown): v is SettingsTab => typeof v === 'string' && (SETTINGS_TABS as readonly string[]).includes(v)

/** Settle one request: apply its data, ignore failures (the API client already toasted them). */
const settle = async <T>(req: Promise<ApiResponse<T>>, apply: (data: T | undefined) => void) => {
  try {
    apply((await req).data)
  } catch {
    /* already notified */
  }
}

/** 站点设置 page: all 13 tabs, parallel loading, one Save button dispatching to the active tab. */
export function useSettingsPage() {
  const loading = ref(false)
  const currentLang = ref<LangCode>('zh-CN')
  const currentTab = ref<SettingsTab>('basic')

  const site = useSiteSettings()
  const smtp = useSmtpSettings()
  const captcha = useCaptchaSettings()
  const telegram = useTelegramAuthSettings()
  const google = useGoogleAuthSettings()
  const dashboard = useDashboardSettings()
  const orderEmail = useOrderEmailTemplateSettings()
  const navigation = useNavigationSettings()
  const homeAnnouncement = useHomeAnnouncementSettings()
  const upstreamSync = useUpstreamSyncSettings()

  const fetchAll = async () => {
    loading.value = true
    try {
      await Promise.all([
        settle(adminAPI.getSettings({ key: 'site_config' }), site.loadSite),
        settle(adminAPI.getSettings({ key: 'order_config' }), site.loadOrder),
        settle(adminAPI.getSMTPSettings(), smtp.load),
        settle(adminAPI.getCaptchaSettings(), captcha.load),
        settle(adminAPI.getTelegramAuthSettings(), telegram.load),
        settle(adminAPI.getGoogleAuthSettings(), google.load),
        settle(adminAPI.getSettings({ key: 'dashboard_config' }), dashboard.load),
        settle(adminAPI.getSettings({ key: 'registration_config' }), site.loadRegistration),
        settle(adminAPI.getOrderEmailTemplateSettings(), orderEmail.load),
        settle(adminAPI.getSettings({ key: 'nav_config' }), navigation.load),
        settle(adminAPI.getSettings({ key: 'home_announcement' }), homeAnnouncement.load),
        settle(adminAPI.getSettings({ key: 'upstream_sync_config' }), upstreamSync.load),
      ])
    } finally {
      loading.value = false
    }
  }

  const savers: Record<SettingsTab, () => Promise<void>> = {
    basic: site.save,
    template: site.save,
    about: site.save,
    legal: site.save,
    navigation: navigation.save,
    home_announcement: homeAnnouncement.save,
    smtp: smtp.save,
    order_email_template: orderEmail.save,
    captcha: captcha.save,
    telegram: telegram.save,
    google: google.save,
    dashboard: dashboard.save,
    upstream_sync: upstreamSync.save,
  }

  const saving = computed(
    () =>
      site.submitting.value ||
      smtp.submitting.value ||
      smtp.testing.value ||
      captcha.submitting.value ||
      telegram.submitting.value ||
      google.submitting.value ||
      dashboard.submitting.value ||
      orderEmail.submitting.value ||
      orderEmail.resetting.value ||
      navigation.submitting.value ||
      homeAnnouncement.submitting.value ||
      upstreamSync.submitting.value,
  )

  const save = () => savers[currentTab.value]()

  return {
    loading,
    saving,
    currentLang,
    currentTab,
    site,
    smtp,
    captcha,
    telegram,
    google,
    dashboard,
    orderEmail,
    navigation,
    homeAnnouncement,
    upstreamSync,
    fetchAll,
    save,
  }
}
