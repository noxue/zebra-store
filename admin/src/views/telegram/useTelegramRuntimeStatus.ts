import { computed, ref } from 'vue'
import { adminAPI } from '@/api/admin'
import type { AdminTelegramBotRuntimeStatus } from '@/api/types'
import { formatDate } from '@/utils/format'

export const LICENSE_PURCHASE_URL = 'https://dujiao-next.com/services/telegram-bot'

export const formatRuntimeDate = (value: unknown): string => {
  if (typeof value !== 'string' || !value) return '-'
  return formatDate(value) || '-'
}

/** Runtime status of the Telegram Bot service (`settings/telegram-bot/runtime-status`). */
export function useTelegramRuntimeStatus() {
  const loading = ref(false)
  const runtimeStatus = ref<AdminTelegramBotRuntimeStatus | null>(null)
  const isConnected = computed(() => runtimeStatus.value?.connected === true)

  const fetchRuntimeStatus = async () => {
    loading.value = true
    try {
      const res = await adminAPI.getTelegramBotRuntimeStatus()
      runtimeStatus.value = res.data ?? null
    } catch {
      runtimeStatus.value = null
    } finally {
      loading.value = false
    }
  }

  return { loading, runtimeStatus, isConnected, fetchRuntimeStatus }
}
