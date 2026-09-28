import { reactive, ref } from 'vue'
import { useRoute } from 'vue-router'
import { adminAPI } from '@/api/admin'
import type { AdminResellerBalanceAccount, AdminResellerLedgerEntry, AdminResellerWithdraw } from '@/api/types'
import i18n from '@/i18n'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { confirmAction } from '@/utils/confirm'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { queryString } from './resellerUtils'

/** `?reseller_id=` deep link used by the profile detail page. */
const useResellerIdQuery = () => {
  const route = useRoute()
  return () => queryString(route.query.reseller_id)
}

export function useResellerLedgerEntries() {
  const readQuery = useResellerIdQuery()
  const filters = reactive({
    keyword: '',
    resellerId: readQuery(),
    userId: '',
    orderId: '',
    orderNo: '',
    type: '__all__',
    status: '__all__',
    createdFrom: '',
    createdTo: '',
  })
  const list = useListPage<AdminResellerLedgerEntry>({
    fetchFn: (page, pageSize) =>
      adminAPI.getResellerLedgerEntries(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          reseller_id: filters.resellerId,
          user_id: filters.userId,
          order_id: filters.orderId,
          order_no: filters.orderNo,
          type: filters.type,
          status: filters.status,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  return {
    filters,
    list,
    refreshing,
    refresh: () => refreshList(list.refresh),
  }
}

export function useResellerBalanceAccounts() {
  const readQuery = useResellerIdQuery()
  const filters = reactive({
    keyword: '',
    resellerId: readQuery(),
    userId: '',
    status: '__all__',
  })
  const list = useListPage<AdminResellerBalanceAccount>({
    fetchFn: (page, pageSize) =>
      adminAPI.getResellerBalanceAccounts(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          reseller_id: filters.resellerId,
          user_id: filters.userId,
          status: filters.status,
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  return {
    filters,
    list,
    refreshing,
    refresh: () => refreshList(list.refresh),
  }
}

export function useResellerWithdraws() {
  const t = i18n.global.t
  const readQuery = useResellerIdQuery()
  const filters = reactive({
    keyword: '',
    resellerId: readQuery(),
    userId: '',
    status: '__all__',
    createdFrom: '',
    createdTo: '',
  })
  const list = useListPage<AdminResellerWithdraw>({
    fetchFn: (page, pageSize) =>
      adminAPI.getResellerWithdraws(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          reseller_id: filters.resellerId,
          user_id: filters.userId,
          status: filters.status,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()

  const operating = ref(false)
  const showRejectDialog = ref(false)
  const selected = ref<AdminResellerWithdraw | null>(null)
  const rejectReason = ref('')

  const openReject = (row: AdminResellerWithdraw) => {
    selected.value = row
    rejectReason.value = ''
    showRejectDialog.value = true
  }

  const submitReject = async () => {
    const row = selected.value
    if (!row) return
    if (
      !(await confirmAction({
        description: t('admin.resellerWithdraws.actions.rejectConfirm', {
          id: row.id,
        }),
        variant: 'destructive',
      }))
    )
      return
    operating.value = true
    try {
      await adminAPI.rejectResellerWithdraw(row.id, {
        reason: rejectReason.value.trim() || undefined,
      })
      showRejectDialog.value = false
      notifySuccess(t('admin.resellerWithdraws.actions.rejectSuccess'))
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      operating.value = false
    }
  }

  const pay = async (row: AdminResellerWithdraw) => {
    if (
      !(await confirmAction({
        description: t('admin.resellerWithdraws.actions.payConfirm', {
          id: row.id,
        }),
      }))
    )
      return
    operating.value = true
    try {
      await adminAPI.payResellerWithdraw(row.id)
      notifySuccess(t('admin.resellerWithdraws.actions.paySuccess'))
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      operating.value = false
    }
  }

  return {
    filters,
    list,
    refreshing,
    refresh: () => refreshList(list.refresh),
    operating,
    showRejectDialog,
    selected,
    rejectReason,
    openReject,
    submitReject,
    pay,
  }
}
