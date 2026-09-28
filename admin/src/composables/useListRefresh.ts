import { ref } from 'vue'
import i18n from '@/i18n'
import { notifySuccess } from '@/utils/notify'

export const useListRefresh = () => {
  const refreshing = ref(false)
  const refreshList = async (loader: () => Promise<void>) => {
    if (refreshing.value) return
    refreshing.value = true
    try {
      await loader()
      notifySuccess(i18n.global.t('admin.common.refreshSuccess'))
    } finally {
      refreshing.value = false
    }
  }
  return { refreshing, refreshList }
}
