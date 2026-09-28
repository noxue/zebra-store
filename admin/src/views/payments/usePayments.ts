import { computed, reactive, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminPayment } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { cleanParams, toRFC3339 } from '@/utils/format'
import { downloadBlob, filenameFromDisposition } from '@/utils/download'
import { errorMessage } from '@/api/client'
import { channelTypeLabel, providerTypeLabel } from './paymentLabels'

interface ChannelTypePair {
  provider_type: string
  channel_type: string
}

/**
 * Filter options derived from the configured channels: channel types follow the selected provider;
 * `wallet` / `balance` are always available since wallet payments have no channel row.
 */
export const deriveProviderTypes = (channels: ChannelTypePair[]) => {
  const set = new Set<string>(channels.map((c) => c.provider_type).filter(Boolean))
  set.add('wallet')
  return Array.from(set).sort()
}

export const deriveChannelTypes = (channels: ChannelTypePair[], provider: string) => {
  const set = new Set<string>()
  for (const c of channels) {
    if (!c.channel_type) continue
    if (provider && c.provider_type !== provider) continue
    set.add(c.channel_type)
  }
  if (!provider || provider === 'wallet') set.add('balance')
  return Array.from(set).sort()
}

const ALL = '__all__'

/** Page logic for 支付记录. */
export function usePayments() {
  const t = i18n.global.t
  const filters = reactive({
    userId: '',
    orderId: '',
    channelId: '',
    providerType: ALL,
    channelType: ALL,
    createdFrom: '',
    createdTo: '',
    status: ALL,
  })

  const queryParams = () =>
    cleanParams({
      status: filters.status,
      user_id: filters.userId.trim(),
      order_id: filters.orderId.trim(),
      channel_id: filters.channelId.trim(),
      provider_type: filters.providerType,
      channel_type: filters.channelType,
      created_from: toRFC3339(filters.createdFrom),
      created_to: toRFC3339(filters.createdTo),
    })

  const list = useListPage<AdminPayment>({
    fetchFn: (page, pageSize) => adminAPI.getPayments({ page, page_size: pageSize, ...queryParams() }),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  // ---- dynamic filter options ----
  const rawChannels = ref<ChannelTypePair[]>([])
  const fetchFilterOptions = async () => {
    try {
      const res = await adminAPI.getPaymentChannels({ page: 1, page_size: 200 })
      rawChannels.value = (res.data ?? []).map((c) => ({ provider_type: c.provider_type, channel_type: c.channel_type }))
    } catch {
      rawChannels.value = []
    }
  }
  const providerOptions = computed(() => [
    { label: t('admin.payments.filterProviderAll'), value: ALL },
    ...deriveProviderTypes(rawChannels.value).map((v) => ({ label: providerTypeLabel(t, v), value: v })),
  ])
  const channelTypeOptions = computed(() => [
    { label: t('admin.payments.filterChannelTypeAll'), value: ALL },
    ...deriveChannelTypes(rawChannels.value, filters.providerType === ALL ? '' : filters.providerType).map((v) => ({
      label: channelTypeLabel(t, v, 'payments'),
      value: v,
    })),
  ])
  const onProviderChange = () => {
    filters.channelType = ALL
    void list.handleSearch()
  }

  // ---- export ----
  const exporting = ref(false)
  const exportError = ref('')
  const handleExport = async () => {
    exportError.value = ''
    exporting.value = true
    try {
      const res = await adminAPI.exportPayments(queryParams())
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-')
      downloadBlob(res.data, filenameFromDisposition(res.headers['content-disposition'], `payments_${timestamp}.csv`))
    } catch {
      exportError.value = t('admin.payments.exportFailed')
    } finally {
      exporting.value = false
    }
  }

  // ---- detail ----
  const showDetail = ref(false)
  const detailLoading = ref(false)
  const detailError = ref('')
  const detail = ref<AdminPayment | null>(null)
  const openDetail = async (id: number) => {
    if (!Number.isFinite(id) || id <= 0) return
    showDetail.value = true
    detailLoading.value = true
    detailError.value = ''
    detail.value = null
    try {
      detail.value = (await adminAPI.getPayment(id)).data ?? null
    } catch (err) {
      detailError.value = errorMessage(err, t('admin.payments.detailFetchFailed'))
    } finally {
      detailLoading.value = false
    }
  }
  const closeDetail = () => {
    showDetail.value = false
    detail.value = null
    detailError.value = ''
  }

  return {
    filters,
    list,
    refreshing,
    refresh,
    fetchFilterOptions,
    providerOptions,
    channelTypeOptions,
    onProviderChange,
    exporting,
    exportError,
    handleExport,
    showDetail,
    detailLoading,
    detailError,
    detail,
    openDetail,
    closeDetail,
  }
}
