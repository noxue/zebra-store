import { reactive, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminResellerDomain } from '@/api/types'
import i18n from '@/i18n'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { confirmAction } from '@/utils/confirm'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'

export function useResellerDomains() {
  const t = i18n.global.t
  const filters = reactive({
    keyword: '',
    resellerId: '',
    userId: '',
    domain: '',
    type: '__all__',
    status: '__all__',
    verificationStatus: '__all__',
    createdFrom: '',
    createdTo: '',
  })

  const list = useListPage<AdminResellerDomain>({
    fetchFn: (page, pageSize) =>
      adminAPI.getResellerDomains(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          reseller_id: filters.resellerId,
          user_id: filters.userId,
          domain: filters.domain,
          type: filters.type,
          status: filters.status,
          verification_status: filters.verificationStatus,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
        }),
      ),
  })

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)
  const operatingId = ref<number | null>(null)

  const run = async (row: AdminResellerDomain, action: () => Promise<unknown>, successKey: string) => {
    operatingId.value = row.id
    try {
      await action()
      notifySuccess(t(successKey))
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      operatingId.value = null
    }
  }

  const approve = async (row: AdminResellerDomain) => {
    if (
      !(await confirmAction({
        description: t('admin.resellerDomains.actions.approveConfirm', {
          id: row.id,
        }),
      }))
    )
      return
    await run(row, () => adminAPI.approveResellerDomain(row.id), 'admin.resellerDomains.actions.approveSuccess')
  }

  const disable = async (row: AdminResellerDomain) => {
    if (
      !(await confirmAction({
        description: t('admin.resellerDomains.actions.disableConfirm', {
          id: row.id,
        }),
        variant: 'destructive',
      }))
    )
      return
    await run(row, () => adminAPI.disableResellerDomain(row.id), 'admin.resellerDomains.actions.disableSuccess')
  }

  return { filters, list, refreshing, refresh, operatingId, approve, disable }
}
