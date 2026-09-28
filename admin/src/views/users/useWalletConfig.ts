import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminPaymentChannel } from '@/api/types'
import { notifySuccess } from '@/utils/notify'
import { sanitizeChannelIds, toggleId } from './usersUtils'

interface WalletConfigValue {
  recharge_channel_ids?: unknown
  wallet_only_payment?: unknown
}

/** Page logic for 钱包配置 (settings key `wallet_config`). */
export function useWalletConfig() {
  const t = i18n.global.t
  const form = reactive({ recharge_channel_ids: [] as number[], wallet_only_payment: false })
  const channels = ref<AdminPaymentChannel[]>([])
  const loading = ref(false)
  const saving = ref(false)

  const loadConfig = async () => {
    try {
      const data = (await adminAPI.getSettings<WalletConfigValue>({ key: 'wallet_config' })).data
      form.recharge_channel_ids = sanitizeChannelIds(data?.recharge_channel_ids)
      form.wallet_only_payment = Boolean(data?.wallet_only_payment)
    } catch {
      form.recharge_channel_ids = []
      form.wallet_only_payment = false
    }
  }

  const loadChannels = async () => {
    try {
      const data = (await adminAPI.getPaymentChannels({ page: 1, page_size: 200 })).data
      channels.value = (Array.isArray(data) ? data : []).filter((ch) => ch.is_active)
    } catch {
      channels.value = []
    }
  }

  const init = async () => {
    loading.value = true
    try {
      await Promise.all([loadConfig(), loadChannels()])
    } finally {
      loading.value = false
    }
  }

  const toggleChannel = (id: number) => {
    form.recharge_channel_ids = toggleId(form.recharge_channel_ids, id)
  }

  const save = async () => {
    saving.value = true
    try {
      await adminAPI.updateSettings({
        key: 'wallet_config',
        value: { recharge_channel_ids: form.recharge_channel_ids, wallet_only_payment: form.wallet_only_payment },
      })
      notifySuccess(t('admin.settings.saved'))
    } catch {
      /* already notified */
    } finally {
      saving.value = false
    }
  }

  return { form, channels, loading, saving, init, toggleChannel, save }
}
