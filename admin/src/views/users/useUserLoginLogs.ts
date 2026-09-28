import { reactive } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminUserLoginLog } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'

export const LOGIN_FAIL_REASONS = [
  'bad_request',
  'captcha_required',
  'captcha_invalid',
  'captcha_config_invalid',
  'captcha_verify_failed',
  'invalid_email',
  'invalid_credentials',
  'email_not_verified',
  'user_disabled',
  'internal_error',
] as const

export function useUserLoginLogs() {
  const filters = reactive({
    userId: '',
    email: '',
    clientIp: '',
    status: '__all__',
    failReason: '__all__',
    createdFrom: '',
    createdTo: '',
  })

  const list = useListPage<AdminUserLoginLog>({
    fetchFn: (page, pageSize) =>
      adminAPI.getUserLoginLogs(
        cleanParams({
          page,
          page_size: pageSize,
          user_id: filters.userId,
          email: filters.email,
          client_ip: filters.clientIp,
          status: filters.status,
          fail_reason: filters.failReason,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
        }),
      ),
  })

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  return { filters, list, refreshing, refresh }
}
