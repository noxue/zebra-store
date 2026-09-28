import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminAffiliateWithdraw } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'

/** Page logic for 提现审核 (list, pay, reject with optional reason). */
export function useAffiliateWithdraws() {
  const t = i18n.global.t
  const filters = reactive({ keyword: '', affiliateProfileId: '', status: '__all__' })
  const operating = ref(false)
  const rejectTarget = ref<AdminAffiliateWithdraw | null>(null)
  const rejectReason = ref('')
  const showReject = ref(false)

  const list = useListPage<AdminAffiliateWithdraw>({
    fetchFn: (page, pageSize) =>
      adminAPI.getAffiliateWithdraws(
        cleanParams({ page, page_size: pageSize, keyword: filters.keyword, affiliate_profile_id: filters.affiliateProfileId, status: filters.status }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  /** Replaces the original confirm() + window.prompt() pair with one dialog. */
  const openReject = (row: AdminAffiliateWithdraw) => {
    rejectTarget.value = row
    rejectReason.value = ''
    showReject.value = true
  }

  const submitReject = async () => {
    const row = rejectTarget.value
    if (!row) return
    operating.value = true
    try {
      await adminAPI.rejectAffiliateWithdraw(row.id, { reason: rejectReason.value.trim() || undefined })
      notifySuccess(t('admin.affiliatesWithdraws.actions.rejectSuccess'))
      showReject.value = false
      await list.refresh()
    } catch {
      /* toast shown by client */
    } finally {
      operating.value = false
    }
  }

  const pay = async (row: AdminAffiliateWithdraw) => {
    if (!(await confirmAction({ description: t('admin.affiliatesWithdraws.actions.payConfirm', { id: row.id }) }))) return
    operating.value = true
    try {
      await adminAPI.payAffiliateWithdraw(row.id)
      notifySuccess(t('admin.affiliatesWithdraws.actions.paySuccess'))
      await list.refresh()
    } catch {
      /* toast shown by client */
    } finally {
      operating.value = false
    }
  }

  return { filters, list, refreshing, refresh, operating, rejectTarget, rejectReason, showReject, openReject, submitReject, pay }
}
