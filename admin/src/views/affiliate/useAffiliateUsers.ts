import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { useSelection } from '@/composables/useSelection'
import { cleanParams } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import {
  AFFILIATE_PROFILE_STATUS_ACTIVE,
  AFFILIATE_PROFILE_STATUS_DISABLED,
  resolveProfileID,
  resolveProfileStatus,
  type AffiliateUserRow,
} from './affiliateUtils'

/** Page logic for 返利用户 (filters, list, single + batch enable/disable). */
export function useAffiliateUsers() {
  const t = i18n.global.t
  const filters = reactive({ keyword: '', code: '', status: '__all__' })
  const operatingId = ref<number | null>(null)

  const list = useListPage<AffiliateUserRow>({
    fetchFn: async (page, pageSize) => {
      const res = await adminAPI.getAffiliateUsers(cleanParams({ page, page_size: pageSize, keyword: filters.keyword, code: filters.code, status: filters.status }))
      return { items: (res.data ?? []) as unknown as AffiliateUserRow[], pagination: res.pagination }
    },
  })
  const selection = useSelection(list.items, resolveProfileID)
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const reload = async () => {
    await list.refresh()
    const visible = new Set(list.items.value.map(resolveProfileID))
    selection.selected.value = new Set(selection.selectedIds.value.filter((id) => visible.has(id)))
  }

  const toggleStatus = async (row: AffiliateUserRow) => {
    const id = resolveProfileID(row)
    if (id <= 0) return
    const isActive = resolveProfileStatus(row) === AFFILIATE_PROFILE_STATUS_ACTIVE
    const ok = await confirmAction({
      description: isActive ? t('admin.affiliatesUsers.actions.disableConfirm', { id }) : t('admin.affiliatesUsers.actions.enableConfirm', { id }),
      variant: isActive ? 'destructive' : 'default',
    })
    if (!ok) return
    operatingId.value = id
    try {
      await adminAPI.updateAffiliateUserStatus(id, { status: isActive ? AFFILIATE_PROFILE_STATUS_DISABLED : AFFILIATE_PROFILE_STATUS_ACTIVE })
      notifySuccess(isActive ? t('admin.affiliatesUsers.actions.disableSuccess') : t('admin.affiliatesUsers.actions.enableSuccess'))
      await reload()
    } catch {
      /* toast shown by client */
    } finally {
      operatingId.value = null
    }
  }

  const batchUpdateStatus = async (status: string) => {
    const ids = selection.selectedIds.value.filter((id) => id > 0)
    if (!ids.length) return
    const isEnable = status === AFFILIATE_PROFILE_STATUS_ACTIVE
    if (!(await confirmAction({ description: t('admin.affiliatesUsers.batch.confirm', { count: ids.length }) }))) return
    try {
      await adminAPI.batchUpdateAffiliateUserStatus({ profile_ids: ids, status })
      notifySuccess(isEnable ? t('admin.affiliatesUsers.batch.enableSuccess', { count: ids.length }) : t('admin.affiliatesUsers.batch.disableSuccess', { count: ids.length }))
      selection.clear()
      await reload()
    } catch {
      /* toast shown by client */
    }
  }

  return { filters, list, selection, operatingId, refreshing, refresh, toggleStatus, batchUpdateStatus }
}
