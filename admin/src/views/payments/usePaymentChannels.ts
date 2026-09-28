import { reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminPaymentChannel } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams } from '@/utils/format'
import { confirmAction } from '@/utils/confirm'
import { notifySuccess } from '@/utils/notify'

interface PaymentFeeConfig {
  customer_fee_enabled: boolean
  reuse_legacy_order_fee_payment: boolean
}

/** Page logic for 支付渠道: fee policy card (payment_config), filters, list, delete. */
export function usePaymentChannels() {
  const t = i18n.global.t
  const filters = reactive({ providerType: '__all__', channelType: '__all__' })

  const list = useListPage<AdminPaymentChannel>({
    fetchFn: (page, pageSize) =>
      adminAPI.getPaymentChannels(cleanParams({ page, page_size: pageSize, provider_type: filters.providerType, channel_type: filters.channelType })),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  // ---- fee config ----
  const feeConfig = reactive<PaymentFeeConfig>({ customer_fee_enabled: false, reuse_legacy_order_fee_payment: false })
  const feeConfigSaving = ref(false)
  const loadFeeConfig = async () => {
    try {
      const data = (await adminAPI.getSettings<Partial<PaymentFeeConfig>>({ key: 'payment_config' })).data
      feeConfig.customer_fee_enabled = data?.customer_fee_enabled === true
      feeConfig.reuse_legacy_order_fee_payment = data?.reuse_legacy_order_fee_payment === true
    } catch {
      feeConfig.customer_fee_enabled = false
      feeConfig.reuse_legacy_order_fee_payment = false
    }
  }
  const saveFeeConfig = async () => {
    feeConfigSaving.value = true
    try {
      await adminAPI.updateSettings({ key: 'payment_config', value: { ...feeConfig } })
      notifySuccess(t('admin.settings.saved'))
    } catch {
      /* already notified */
    } finally {
      feeConfigSaving.value = false
    }
  }

  // ---- modal ----
  const showModal = ref(false)
  const editingId = ref<number | null>(null)
  const openCreate = () => {
    editingId.value = null
    showModal.value = true
  }
  const openEdit = (id: number) => {
    editingId.value = id
    showModal.value = true
  }
  const openEditById = (raw: unknown) => {
    const id = Number(raw)
    if (Number.isFinite(id) && id > 0) openEdit(id)
  }
  const onModalSuccess = () => void list.fetchData(list.pagination.value.page)

  const remove = async (channel: AdminPaymentChannel) => {
    const ok = await confirmAction({
      description: t('admin.paymentChannels.confirmDelete', { name: channel.name }),
      confirmText: t('admin.common.delete'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await adminAPI.deletePaymentChannel(channel.id)
      notifySuccess(t('admin.common.operationSuccess'))
      await list.fetchData(list.pagination.value.page)
    } catch {
      /* already notified */
    }
  }

  return {
    filters,
    list,
    refreshing,
    refresh,
    feeConfig,
    feeConfigSaving,
    loadFeeConfig,
    saveFeeConfig,
    showModal,
    editingId,
    openCreate,
    openEdit,
    openEditById,
    onModalSuccess,
    remove,
  }
}
