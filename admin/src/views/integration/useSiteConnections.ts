import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { api, errorMessage } from '@/api/client'
import type { AdminSiteConnection } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useCrudModal } from '@/composables/useCrudModal'
import { rules, useFormValidation } from '@/composables/useFormValidation'
import { confirmAction } from '@/utils/confirm'
import { notifyError, notifySuccess } from '@/utils/notify'
import { buildSiteConnectionPayload, emptySiteConnectionForm, isRecord, siteConnectionToForm, validateProtocolConfig, type SiteConnectionForm } from './integrationUtils'
import { useConnectionWizard } from './useConnectionWizard'
import { useProtocolRegistry } from './useProtocolRegistry'

/** Page logic for 连接管理 (upstream site connections). */
export function useSiteConnections() {
  const t = i18n.global.t
  const list = useListPage<AdminSiteConnection>({
    fetchFn: (page, pageSize) => adminAPI.getSiteConnections({ page, page_size: pageSize }),
  })
  const form = reactive<SiteConnectionForm>(emptySiteConnectionForm())
  const modal = useCrudModal({ onSuccess: () => void list.fetchData(modal.isEditing.value ? list.pagination.value.page : 1) })
  const pingingId = ref<number | null>(null)
  const reapplyingId = ref<number | null>(null)

  const registry = useProtocolRegistry()
  const wizard = useConnectionWizard(form, registry.resolve)
  /** Errors of the adapter-specific fields, keyed by field key. */
  const configErrors = ref<Record<string, string>>({})

  const { errors, validate, clearErrors } = useFormValidation<SiteConnectionForm>({
    name: [rules.required(t('admin.common.required'))],
    protocol: [rules.required(t('admin.common.required'))],
  })

  const resetForm = (next: SiteConnectionForm) => {
    clearErrors()
    configErrors.value = {}
    wizard.reset()
    Object.assign(form, next)
  }

  const openCreate = () => {
    resetForm(emptySiteConnectionForm(registry.defaultId()))
    modal.openCreate()
    if (!registry.loaded.value && !registry.loading.value) void registry.load().then(() => (form.protocol ||= registry.defaultId()))
  }

  const openEdit = (conn: AdminSiteConnection) => {
    resetForm(siteConnectionToForm(conn))
    modal.openEdit(conn.id)
  }

  /** Switching adapter invalidates the previous handshake and field errors (shared values are kept). */
  const selectProtocol = (id: string) => {
    if (form.protocol === id) return
    form.protocol = id
    configErrors.value = {}
    wizard.clearResult()
  }

  const setConfigValue = (key: string, value: string) => {
    form.config = { ...form.config, [key]: value }
    if (configErrors.value[key]) configErrors.value = { ...configErrors.value, [key]: '' }
  }

  const submit = () => {
    const def = registry.resolve(form.protocol)
    configErrors.value = validateProtocolConfig(
      def,
      form.config,
      { required: t('admin.common.required'), url: t('admin.zebraIntegration.invalidUrl') },
      { editing: modal.isEditing.value },
    )
    const formOk = validate({ ...form })
    if (!formOk || Object.keys(configErrors.value).length > 0) return
    const editingId = modal.editingId.value
    void modal.handleSubmit(async () => {
      const payload = buildSiteConnectionPayload(form, def)
      if (modal.isEditing.value && editingId) await adminAPI.updateSiteConnection(editingId, payload)
      else await adminAPI.createSiteConnection(payload)
      notifySuccess()
    })
  }

  /** Ping silently so the toast reads "Ping failed: <reason>" like the original (single notification). */
  const ping = async (conn: AdminSiteConnection) => {
    pingingId.value = conn.id
    try {
      await api.post(`/admin/site-connections/${conn.id}/ping`, undefined, { silent: true, timeout: 30_000 })
      notifySuccess(t('siteConnections.ping.success'))
    } catch (err) {
      notifyError(`${t('siteConnections.ping.failed')}: ${errorMessage(err)}`)
    } finally {
      pingingId.value = null
      void list.refresh()
    }
  }

  const toggleStatus = async (conn: AdminSiteConnection) => {
    const next = conn.status === 'active' ? 'disabled' : 'active'
    try {
      await adminAPI.updateSiteConnectionStatus(conn.id, { status: next })
      notifySuccess()
      void list.refresh()
    } catch {
      /* already notified */
    }
  }

  const remove = async (conn: AdminSiteConnection) => {
    const ok = await confirmAction({
      description: t('siteConnections.delete.confirm', { name: conn.name || `#${conn.id}` }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deleteSiteConnection(conn.id)
      notifySuccess()
      void list.refresh()
    } catch {
      /* already notified */
    }
  }

  const reapplyMarkup = async (conn: AdminSiteConnection) => {
    const ok = await confirmAction({
      description: t('siteConnections.reapplyMarkup.confirm', { name: conn.name || `#${conn.id}` }),
      confirmText: t('siteConnections.actions.reapplyMarkup'),
    })
    if (!ok) return
    reapplyingId.value = conn.id
    try {
      const res = await adminAPI.reapplyConnectionMarkup(conn.id)
      const count = isRecord(res.data) ? Number(res.data.updated_products ?? 0) || 0 : 0
      notifySuccess(t('siteConnections.reapplyMarkup.success', { count }))
    } catch {
      /* already notified */
    } finally {
      reapplyingId.value = null
    }
  }

  return {
    list,
    form,
    modal,
    errors,
    configErrors,
    registry,
    wizard,
    selectProtocol,
    setConfigValue,
    pingingId,
    reapplyingId,
    openCreate,
    openEdit,
    submit,
    ping,
    toggleStatus,
    remove,
    reapplyMarkup,
  }
}
