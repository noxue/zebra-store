import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { adminAPI } from '@/api/admin'
import type { AdminTelegramBroadcast } from '@/api/types'

/** Read-only broadcast detail for route `telegram-bot/broadcasts/:id`. */
export function useTelegramBroadcastDetail() {
  const route = useRoute()
  const broadcastId = computed(() => Number(route.params.id))
  const loading = ref(false)
  const broadcast = ref<AdminTelegramBroadcast | null>(null)

  const fetchBroadcast = async () => {
    if (!Number.isFinite(broadcastId.value) || broadcastId.value <= 0) {
      broadcast.value = null
      return
    }
    loading.value = true
    try {
      const res = await adminAPI.getTelegramBroadcast(broadcastId.value)
      broadcast.value = res.data || null
    } catch {
      broadcast.value = null
    } finally {
      loading.value = false
    }
  }

  watch(broadcastId, () => void fetchBroadcast())

  return { broadcastId, loading, broadcast, fetchBroadcast }
}
