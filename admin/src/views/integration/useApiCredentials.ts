import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminApiCredential } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { cleanParams } from '@/utils/format'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'

export const API_CREDENTIAL_STATUSES = ['pending_review', 'approved', 'rejected'] as const

/** Page logic for API 凭证 (downstream API keys review). */
export function useApiCredentials() {
  const t = i18n.global.t
  const filters = reactive({ status: '__all__', search: '' })
  const list = useListPage<AdminApiCredential>({
    fetchFn: (page, pageSize) => adminAPI.getApiCredentials(cleanParams({ page, page_size: pageSize, status: filters.status, search: filters.search.trim() })),
  })

  const showDetail = ref(false)
  const detail = ref<AdminApiCredential | null>(null)
  const showReject = ref(false)
  const rejectId = ref(0)
  const rejectReason = ref('')
  const rejecting = ref(false)

  const run = async (fn: () => Promise<unknown>, successKey: string) => {
    try {
      await fn()
      notifySuccess(t(successKey))
      void list.refresh()
      return true
    } catch {
      return false
    }
  }

  const openDetail = (c: AdminApiCredential) => {
    detail.value = c
    showDetail.value = true
  }

  const approve = async (c: AdminApiCredential) => {
    if (!(await confirmAction(t('apiCredentials.approve.confirm')))) return
    await run(() => adminAPI.approveApiCredential(c.id), 'apiCredentials.approve.success')
  }

  const openReject = (c: AdminApiCredential) => {
    rejectId.value = c.id
    rejectReason.value = ''
    showReject.value = true
  }

  const submitReject = async () => {
    const reason = rejectReason.value.trim()
    if (!reason) return
    rejecting.value = true
    if (await run(() => adminAPI.rejectApiCredential(rejectId.value, { reason }), 'apiCredentials.reject.success')) showReject.value = false
    rejecting.value = false
  }

  const toggle = async (c: AdminApiCredential) => {
    const ok = await confirmAction(c.is_active ? t('apiCredentials.toggle.disableConfirm') : t('apiCredentials.toggle.enableConfirm'))
    if (!ok) return
    await run(() => adminAPI.updateApiCredentialStatus(c.id, { is_active: !c.is_active }), 'apiCredentials.toggle.success')
  }

  const remove = async (c: AdminApiCredential) => {
    const ok = await confirmAction({ description: t('apiCredentials.delete.confirm'), variant: 'destructive', confirmText: t('admin.common.delete') })
    if (!ok) return
    await run(() => adminAPI.deleteApiCredential(c.id), 'apiCredentials.delete.success')
  }

  return { filters, list, showDetail, detail, showReject, rejectReason, rejecting, openDetail, approve, openReject, submitReject, toggle, remove }
}
