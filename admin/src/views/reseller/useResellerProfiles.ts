import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminResellerProfile, AdminResellerProfileUpdatePayload } from '@/api/types'
import i18n from '@/i18n'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { confirmAction } from '@/utils/confirm'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import { validateMarkupRange } from './resellerUtils'

export type ReasonAction = 'reject' | 'disable'

export function useResellerProfiles() {
  const t = i18n.global.t
  const filters = reactive({
    keyword: '',
    userId: '',
    status: '__all__',
    settlementStatus: '__all__',
    createdFrom: '',
    createdTo: '',
  })

  const list = useListPage<AdminResellerProfile>({
    fetchFn: (page, pageSize) =>
      adminAPI.getResellerProfiles(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          user_id: filters.userId,
          status: filters.status,
          settlement_status: filters.settlementStatus,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const operatingId = ref<number | null>(null)
  const selected = ref<AdminResellerProfile | null>(null)
  const showApproveDialog = ref(false)
  const showReasonDialog = ref(false)
  const showEditDialog = ref(false)
  const reasonAction = ref<ReasonAction>('reject')
  const approveForm = reactive({ defaultMarkup: '0.00', maxMarkup: '0.00' })
  const reasonForm = reactive({ reason: '' })

  const withRow = async (row: AdminResellerProfile, task: () => Promise<void>) => {
    operatingId.value = row.id
    try {
      await task()
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      operatingId.value = null
    }
  }

  const openApprove = (row: AdminResellerProfile) => {
    selected.value = row
    approveForm.defaultMarkup = row.default_markup_percent || '0.00'
    approveForm.maxMarkup = row.max_markup_percent || '0.00'
    showApproveDialog.value = true
  }

  const submitApprove = async () => {
    const row = selected.value
    if (!row) return
    const err = validateMarkupRange(approveForm.defaultMarkup, approveForm.maxMarkup)
    if (err) return notifyError(t(err))
    await withRow(row, async () => {
      await adminAPI.approveResellerProfile(row.id, {
        default_markup_percent: approveForm.defaultMarkup.trim() || '0.00',
        max_markup_percent: approveForm.maxMarkup.trim() || '0.00',
      })
      showApproveDialog.value = false
      notifySuccess(t('admin.resellerProfiles.actions.approveSuccess'))
    })
  }

  const openReason = (row: AdminResellerProfile, action: ReasonAction) => {
    selected.value = row
    reasonAction.value = action
    reasonForm.reason = ''
    showReasonDialog.value = true
  }

  const submitReason = async () => {
    const row = selected.value
    if (!row) return
    const reason = reasonForm.reason.trim()
    if (reasonAction.value === 'reject' && !reason) return notifyError(t('admin.resellerProfiles.actions.rejectReasonPrompt'))
    await withRow(row, async () => {
      if (reasonAction.value === 'reject') {
        await adminAPI.rejectResellerProfile(row.id, { reason })
        notifySuccess(t('admin.resellerProfiles.actions.rejectSuccess'))
      } else {
        await adminAPI.disableResellerProfile(row.id, {
          reason: reason || undefined,
        })
        notifySuccess(t('admin.resellerProfiles.actions.disableSuccess'))
      }
      showReasonDialog.value = false
    })
  }

  const openEdit = (row: AdminResellerProfile) => {
    selected.value = row
    showEditDialog.value = true
  }

  const submitEdit = async (payload: AdminResellerProfileUpdatePayload) => {
    const row = selected.value
    if (!row) return
    await withRow(row, async () => {
      await adminAPI.updateResellerProfile(row.id, payload)
      showEditDialog.value = false
      notifySuccess(t('admin.resellerProfiles.actions.updateSuccess'))
    })
  }

  const restore = async (row: AdminResellerProfile) => {
    if (
      !(await confirmAction({
        description: t('admin.resellerProfiles.actions.restoreConfirm', {
          id: row.id,
        }),
      }))
    )
      return
    await withRow(row, async () => {
      await adminAPI.restoreResellerProfile(row.id)
      notifySuccess(t('admin.resellerProfiles.actions.restoreSuccess'))
    })
  }

  return {
    filters,
    list,
    refreshing,
    refresh,
    operatingId,
    selected,
    showApproveDialog,
    showReasonDialog,
    showEditDialog,
    reasonAction,
    approveForm,
    reasonForm,
    openApprove,
    submitApprove,
    openReason,
    submitReason,
    openEdit,
    submitEdit,
    restore,
  }
}
