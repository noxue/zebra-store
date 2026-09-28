import { computed, reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage } from '@/api/client'
import { resellerAPI } from '@/api/reseller'
import type { ResellerSiteConfigSnapshotData } from '@/api/types'
import type { AlertTone } from '@/components/ui'
import { useAppStore } from '@/stores/app'
import {
  blankLocalizedText,
  canEditResellerSiteConfig,
  createBlankSiteConfigForm,
  isResellerSiteSeoConfigured,
  MAX_SITE_LINKS,
  siteConfigFormToPayload,
  siteConfigToForm,
  type ResellerLocale,
} from '@/utils/reseller/siteConfig'
import { useResellerProfileContext } from './useResellerProfile'

export type SiteSection = 'brand' | 'support' | 'content' | 'navigation'

export const useResellerSiteConfig = () => {
  const { t } = useI18n()
  const appStore = useAppStore()
  const profile = useResellerProfileContext()
  const loading = ref(true)
  const saving = ref(false)
  const alert = ref<{ tone: AlertTone; message: string } | null>(null)
  const snapshot = ref<ResellerSiteConfigSnapshotData | null>(null)
  const form = reactive(createBlankSiteConfigForm())
  const baseline = ref('')
  const activeLocale = ref<ResellerLocale>('zh-CN')
  const activeSection = ref<SiteSection>('brand')

  const canEdit = computed(() => canEditResellerSiteConfig(snapshot.value))
  const dirty = computed(() => !loading.value && !saving.value && baseline.value !== '' && JSON.stringify(form) !== baseline.value)
  const config = computed(() => snapshot.value?.config || null)

  const readiness = computed(() => {
    const cfg = config.value
    const support = cfg?.support || {}
    return [
      { key: 'brand', label: t('resellerConsole.site.readiness.brand'), done: Boolean(cfg?.site_name || cfg?.logo || cfg?.favicon) },
      { key: 'support', label: t('resellerConsole.site.readiness.support'), done: Boolean(support.telegram || support.whatsapp || support.email || support.support_url) },
      { key: 'seo', label: t('resellerConsole.site.readiness.seo'), done: isResellerSiteSeoConfigured(cfg?.seo) },
      { key: 'announcement', label: t('resellerConsole.site.readiness.announcement'), done: Boolean(cfg?.announcement?.enabled) },
    ]
  })

  const assign = (data: ResellerSiteConfigSnapshotData | null) => {
    snapshot.value = data
    Object.assign(form, siteConfigToForm(data?.config))
    baseline.value = JSON.stringify(form)
  }

  const load = async () => {
    loading.value = true
    alert.value = null
    try {
      const [res] = await Promise.all([resellerAPI.siteConfig(), profile.load()])
      assign(res.data || null)
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.siteConfig.loadFailed')) }
    } finally {
      loading.value = false
    }
  }

  const reset = () => Object.assign(form, siteConfigToForm(snapshot.value?.config))

  const save = async () => {
    if (!canEdit.value || saving.value) return
    saving.value = true
    alert.value = null
    try {
      const res = await resellerAPI.updateSiteConfig(siteConfigFormToPayload(form))
      assign(res.data && 'config' in res.data ? res.data : { opened: true, can_edit: true, config: snapshot.value?.config })
      alert.value = { tone: 'success', message: t('personalCenter.reseller.siteConfig.saveSuccess') }
      await appStore.loadConfig(true).catch(() => undefined)
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.siteConfig.saveFailed')) }
    } finally {
      saving.value = false
    }
  }

  const addFooterLink = () => {
    if (form.footer_links.length < MAX_SITE_LINKS) form.footer_links.push({ name: blankLocalizedText(), url: '' })
  }
  const removeFooterLink = (i: number) => form.footer_links.splice(i, 1)
  const addCustomNav = () => {
    if (form.nav_config.custom_items.length < MAX_SITE_LINKS) form.nav_config.custom_items.push({ name: blankLocalizedText(), url: '' })
  }
  const removeCustomNav = (i: number) => form.nav_config.custom_items.splice(i, 1)

  return {
    profile,
    loading,
    saving,
    alert,
    snapshot,
    form,
    activeLocale,
    activeSection,
    canEdit,
    dirty,
    config,
    readiness,
    load,
    reset,
    save,
    addFooterLink,
    removeFooterLink,
    addCustomNav,
    removeCustomNav,
  }
}

/** Upload helper for image fields. Returns the uploaded URL. */
export const useResellerImageUpload = () => {
  const { t } = useI18n()
  const uploading = ref(false)
  const error = ref('')
  const upload = async (file: File): Promise<string | null> => {
    uploading.value = true
    error.value = ''
    try {
      const res = await resellerAPI.uploadImage(file)
      return res.data?.url || null
    } catch (err) {
      error.value = errorMessage(err, t('personalCenter.reseller.siteConfig.uploadFailed'))
      return null
    } finally {
      uploading.value = false
    }
  }
  return { uploading, error, upload }
}
