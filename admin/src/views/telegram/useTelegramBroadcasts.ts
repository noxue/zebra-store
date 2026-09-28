import { reactive, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { adminAPI } from '@/api/admin'
import type { AdminTelegramBroadcast } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'
import { canDeleteBroadcast } from './telegramUtils'

const emptyFilters = () => ({
  keyword: '',
  recipientType: '__all__',
  status: '__all__',
  createdFrom: '',
  createdTo: '',
})

export function useTelegramBroadcasts() {
  const { t } = useI18n()
  const filters = reactive(emptyFilters())
  const deletingId = ref<number | null>(null)

  const list = useListPage<AdminTelegramBroadcast>({
    fetchFn: (page, pageSize) =>
      adminAPI.getTelegramBroadcasts(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          recipient_type: filters.recipientType,
          status: filters.status,
          created_from: toRFC3339(filters.createdFrom),
          created_to: toRFC3339(filters.createdTo),
        }),
      ),
  })

  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  const resetFilters = () => {
    Object.assign(filters, emptyFilters())
    void list.fetchData(1)
  }

  const handleDelete = async (item: AdminTelegramBroadcast) => {
    if (!canDeleteBroadcast(item.status)) return
    const ok = await confirmAction({
      description: t('telegramBot.broadcasts.deleteConfirm', { title: item.title }),
      confirmText: t('telegramBot.broadcasts.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    deletingId.value = item.id
    try {
      await adminAPI.deleteTelegramBroadcast(item.id)
      notifySuccess(t('telegramBot.broadcasts.deleteSuccess'))
      const { page } = list.pagination.value
      await list.fetchData(list.items.value.length === 1 && page > 1 ? page - 1 : page)
    } catch {
      /* already notified */
    } finally {
      deletingId.value = null
    }
  }

  return { filters, list, refreshing, refresh, resetFilters, deletingId, handleDelete }
}
