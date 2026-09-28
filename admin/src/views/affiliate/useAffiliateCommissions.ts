import { reactive } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminAffiliateCommission } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams } from '@/utils/format'

/** Page logic for 佣金记录 (read-only list). */
export function useAffiliateCommissions() {
  const filters = reactive({ keyword: '', orderNo: '', affiliateProfileId: '', status: '__all__' })
  const list = useListPage<AdminAffiliateCommission>({
    fetchFn: (page, pageSize) =>
      adminAPI.getAffiliateCommissions(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          order_no: filters.orderNo,
          affiliate_profile_id: filters.affiliateProfileId,
          status: filters.status,
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)
  return { filters, list, refreshing, refresh }
}
