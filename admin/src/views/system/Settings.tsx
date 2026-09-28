import { computed, defineComponent, onMounted, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { Save } from 'lucide-vue-next'
import { Button, Loader, PageHeader, Tabs } from '@/components/ui'
import { SETTINGS_TABS, SETTINGS_TAB_LABEL_KEYS, isSettingsTab, useSettingsPage, type SettingsTab } from './settings/useSettingsPage'
import { LangSwitcher } from './settings/SettingsUi'
import BasicTab from './settings/BasicTab'
import TemplateTab from './settings/TemplateTab'
import NavigationTab from './settings/NavigationTab'
import AboutTab from './settings/AboutTab'
import LegalTab from './settings/LegalTab'
import HomeAnnouncementTab from './settings/HomeAnnouncementTab'
import SmtpTab from './settings/SmtpTab'
import OrderEmailTemplateTab from './settings/OrderEmailTemplateTab'
import CaptchaTab from './settings/CaptchaTab'
import TelegramTab from './settings/TelegramTab'
import GoogleTab from './settings/GoogleTab'
import DashboardTab from './settings/DashboardTab'
import UpstreamSyncTab from './settings/UpstreamSyncTab'

export default defineComponent({
  name: 'SettingsView',
  setup() {
    const { t } = useI18n()
    const route = useRoute()
    const router = useRouter()
    const p = useSettingsPage()

    // `?tab=` keeps the active tab across reloads (Zebra Store convenience; the original always opens on basic).
    if (isSettingsTab(route.query.tab)) p.currentTab.value = route.query.tab
    watch(p.currentTab, (tab) => {
      if (route.query.tab !== tab) void router.replace({ query: { ...route.query, tab } })
    })

    onMounted(() => void p.fetchAll())

    const langNames = computed(() => ({
      'zh-CN': t('admin.common.lang.zhCN'),
      'zh-TW': t('admin.common.lang.zhTW'),
      'en-US': t('admin.common.lang.enUS'),
    }))

    const renderTab = (tab: SettingsTab) => {
      const lang = p.currentLang.value
      const langName = langNames.value[lang]
      switch (tab) {
        case 'basic':
          return <BasicTab model={p.site} lang={lang} langName={langName} />
        case 'template':
          return <TemplateTab model={p.site} />
        case 'navigation':
          return <NavigationTab model={p.navigation} lang={lang} />
        case 'about':
          return <AboutTab model={p.site} lang={lang} langName={langName} />
        case 'legal':
          return <LegalTab model={p.site} lang={lang} langName={langName} />
        case 'home_announcement':
          return <HomeAnnouncementTab model={p.homeAnnouncement} lang={lang} />
        case 'smtp':
          return <SmtpTab model={p.smtp} />
        case 'order_email_template':
          return <OrderEmailTemplateTab model={p.orderEmail} lang={lang} />
        case 'captcha':
          return <CaptchaTab model={p.captcha} />
        case 'telegram':
          return <TelegramTab model={p.telegram} />
        case 'google':
          return <GoogleTab model={p.google} />
        case 'dashboard':
          return <DashboardTab model={p.dashboard} />
        case 'upstream_sync':
          return <UpstreamSyncTab model={p.upstreamSync} />
      }
    }

    return () => {
      const busy = p.loading.value || p.saving.value
      return (
        <div class="space-y-6">
          <PageHeader title={t('admin.settings.title')} subtitle={t('admin.settings.subtitle')}>
            {{
              actions: () => (
                <>
                  <LangSwitcher v-model={p.currentLang.value} />
                  <Button variant="primary" loading={busy} disabled={busy} onClick={p.save} data-testid="settings-save">
                    {!busy && <Save class="h-4 w-4" />}
                    {p.saving.value ? t('admin.settings.actions.saving') : t('admin.settings.actions.save')}
                  </Button>
                </>
              ),
            }}
          </PageHeader>

          <Tabs
            v-model={p.currentTab.value}
            items={SETTINGS_TABS.map((key) => ({ key, label: t(SETTINGS_TAB_LABEL_KEYS[key]) }))}
          />

          {p.loading.value ? <Loader /> : renderTab(p.currentTab.value)}
        </div>
      )
    }
  },
})
