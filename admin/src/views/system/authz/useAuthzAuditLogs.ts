import { reactive } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminAuthzAuditLog } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { auditRoleFilter } from './authzUtils'

const emptyFilters = () => ({
  operator_admin_id: '' as string | number,
  target_admin_id: '' as string | number,
  action: '__all__',
  role: '',
  object: '',
  method: '__all__',
  created_from: '',
  created_to: '',
})

/** Page logic for 权限审计日志 (filters + paginated list). */
export function useAuthzAuditLogs() {
  const filters = reactive(emptyFilters())

  const list = useListPage<AdminAuthzAuditLog>({
    fetchFn: (page, pageSize) =>
      adminAPI.listAuthzAuditLogs(
        cleanParams({
          page,
          page_size: pageSize,
          operator_admin_id: filters.operator_admin_id,
          target_admin_id: filters.target_admin_id,
          action: filters.action,
          role: auditRoleFilter(filters.role),
          object: filters.object.trim(),
          method: filters.method,
          created_from: toRFC3339(filters.created_from),
          created_to: toRFC3339(filters.created_to),
        }),
      ),
  })

  const reset = () => {
    Object.assign(filters, emptyFilters())
    void list.handleSearch()
  }

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  return { filters, list, reset, refreshing, refresh }
}
