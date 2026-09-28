import { reactive, ref } from 'vue'
import { useRoute } from 'vue-router'
import { adminAPI } from '@/api/admin'
import type { AdminResellerSiteConfig } from '@/api/types'
import i18n from '@/i18n'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { confirmAction } from '@/utils/confirm'
import { cleanParams } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { blankLocalizedText, type ResellerLocale } from '@/utils/resellerSiteConfig'
import { buildSiteConfigPayload, createBlankSiteConfigForm, queryString, siteConfigToForm, type ResellerSiteConfigForm } from './resellerUtils'

export type SiteConfigTab = 'brand' | 'announcement' | 'support' | 'seo' | 'footer' | 'nav'

export function useResellerSiteConfigs() {
  const t = i18n.global.t
  const route = useRoute()
  const filters = reactive({
    keyword: '',
    resellerId: queryString(route.query.reseller_id),
  })

  const list = useListPage<AdminResellerSiteConfig>({
    fetchFn: (page, pageSize) =>
      adminAPI.getResellerSiteConfigs(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          reseller_id: filters.resellerId,
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const operatingId = ref<number | null>(null)
  const saving = ref(false)
  const showEditor = ref(false)
  const selected = ref<AdminResellerSiteConfig | null>(null)
  const activeTab = ref<SiteConfigTab>('brand')
  const activeLocale = ref<ResellerLocale>('zh-CN')
  const form = reactive<ResellerSiteConfigForm>(createBlankSiteConfigForm())

  const assignForm = (source: ResellerSiteConfigForm) => Object.assign(form, source)

  const openEditor = async (row: AdminResellerSiteConfig) => {
    operatingId.value = row.reseller_id
    try {
      const res = await adminAPI.getResellerSiteConfig(row.reseller_id)
      const latest = res.data
      if (!latest) return
      selected.value = latest
      assignForm(siteConfigToForm(latest))
      activeTab.value = 'brand'
      activeLocale.value = 'zh-CN'
      showEditor.value = true
    } catch {
      /* already notified */
    } finally {
      operatingId.value = null
    }
  }

  const closeEditor = () => {
    showEditor.value = false
    selected.value = null
  }

  const addFooterLink = () => form.footer_links.push({ name: blankLocalizedText(), url: '' })
  const removeFooterLink = (index: number) => form.footer_links.splice(index, 1)

  const save = async () => {
    if (!selected.value) return
    saving.value = true
    try {
      const res = await adminAPI.updateResellerSiteConfig(selected.value.reseller_id, buildSiteConfigPayload(form))
      if (res.data) {
        selected.value = res.data
        assignForm(siteConfigToForm(res.data))
      }
      notifySuccess(t('admin.resellerSiteConfigs.messages.saveSuccess'))
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      saving.value = false
    }
  }

  const reset = async (row: AdminResellerSiteConfig) => {
    if (
      !(await confirmAction({
        description: t('admin.resellerSiteConfigs.actions.resetConfirm', {
          id: row.reseller_id,
        }),
        variant: 'destructive',
      }))
    )
      return
    operatingId.value = row.reseller_id
    try {
      await adminAPI.resetResellerSiteConfig(row.reseller_id)
      notifySuccess(t('admin.resellerSiteConfigs.messages.resetSuccess'))
      if (selected.value?.reseller_id === row.reseller_id) {
        closeEditor()
        assignForm(createBlankSiteConfigForm())
      }
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      operatingId.value = null
    }
  }

  return {
    filters,
    list,
    refreshing,
    refresh,
    operatingId,
    saving,
    showEditor,
    selected,
    activeTab,
    activeLocale,
    form,
    openEditor,
    closeEditor,
    addFooterLink,
    removeFooterLink,
    save,
    reset,
  }
}
