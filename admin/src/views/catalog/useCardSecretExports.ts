import { computed, ref, watch } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminCardSecretBatch } from '@/api/types'
import { copyText } from '@/utils/clipboard'
import { downloadBlob, filenameFromDisposition } from '@/utils/download'
import { ALL, useProductSkuPicker } from './useProductSkuPicker'
import { parsePositiveInteger } from './cardSecretUtils'

export interface ExportResult {
  content: string
  blob: Blob
  filename: string
  format: 'txt' | 'csv'
  count: number
  deleted: boolean
  productLabel: string
  skuLabel: string
}

/** Page logic for 卡密导出: pick product/SKU/batch, export N available secrets (marks used or deletes). */
export function useCardSecretExports() {
  const t = i18n.global.t
  const picker = useProductSkuPicker()

  const batchValue = ref<string | number>(ALL)
  const batches = ref<AdminCardSecretBatch[]>([])
  const batchesLoading = ref(false)
  const availableCount = ref(0)
  const exportCount = ref<number | ''>(1)
  const exportFormat = ref<'txt' | 'csv'>('txt')
  const deleteAfterExport = ref(false)
  const exporting = ref(false)
  const successMessage = ref('')
  const errorMessage = ref('')
  const confirmOpen = ref(false)
  const resultMessage = ref('')
  const exportResult = ref<ExportResult | null>(null)

  const batchId = computed(() => (batchValue.value === ALL ? 0 : parsePositiveInteger(batchValue.value)))
  const countValue = computed(() => Number(exportCount.value) || 0)
  const currentAvailable = computed(() => {
    if (batchId.value) return Number(batches.value.find((b) => Number(b.id || 0) === batchId.value)?.available_count || 0)
    return availableCount.value
  })
  const exportDisabled = computed(
    () => exporting.value || !picker.productId.value || countValue.value <= 0 || countValue.value > currentAvailable.value,
  )
  const confirmMessage = computed(() =>
    deleteAfterExport.value
      ? t('admin.cardSecretExports.confirmDelete', { count: countValue.value })
      : t('admin.cardSecretExports.confirmUsed', { count: countValue.value }),
  )
  const batchOptions = computed(() => [
    { value: ALL, label: t('admin.cardSecretExports.batchAll') },
    ...batches.value.map((b) => ({
      value: b.id,
      label: `${String(b.batch_no || '').trim() || `#${b.id}`} · ${t('admin.cardSecrets.stats.available')} ${Number(b.available_count || 0)}`,
    })),
  ])

  const resetMessages = () => {
    successMessage.value = ''
    errorMessage.value = ''
    resultMessage.value = ''
  }

  const loadInventoryMeta = async () => {
    const productId = picker.productId.value
    if (!productId) {
      batches.value = []
      availableCount.value = 0
      return
    }
    batchesLoading.value = true
    try {
      const params = { product_id: productId, sku_id: picker.skuId.value || undefined }
      const [statsRes, batchesRes] = await Promise.all([
        adminAPI.getCardSecretStats(params),
        adminAPI.getCardSecretBatches({ ...params, page: 1, page_size: 100 }),
      ])
      availableCount.value = Number(statsRes.data?.available || 0)
      batches.value = Array.isArray(batchesRes.data) ? batchesRes.data : []
      if (batchId.value && !batches.value.some((b) => Number(b.id || 0) === batchId.value)) batchValue.value = ALL
    } catch {
      availableCount.value = 0
      batches.value = []
    } finally {
      batchesLoading.value = false
    }
  }

  const onProductChange = async () => {
    resetMessages()
    picker.skuValue.value = ALL
    batchValue.value = ALL
    exportCount.value = 1
    await picker.loadProductInfo()
    if (!picker.productId.value) {
      batches.value = []
      availableCount.value = 0
    }
    await loadInventoryMeta()
  }
  const onSkuChange = async () => {
    resetMessages()
    batchValue.value = ALL
    exportCount.value = 1
    await loadInventoryMeta()
  }

  const submitExport = () => {
    resetMessages()
    if (!picker.productId.value) {
      errorMessage.value = t('admin.cardSecrets.errors.productRequired')
      return
    }
    if (countValue.value <= 0 || countValue.value > currentAvailable.value) {
      errorMessage.value = t('admin.cardSecretExports.errors.countInvalid')
      return
    }
    confirmOpen.value = true
  }

  const runConfirmedExport = async () => {
    const productId = picker.productId.value
    if (!productId) return
    confirmOpen.value = false
    exporting.value = true
    try {
      const format = exportFormat.value
      const deleted = deleteAfterExport.value
      const count = countValue.value
      const res = await adminAPI.exportAvailableCardSecrets({
        product_id: productId,
        sku_id: picker.skuId.value || undefined,
        batch_id: batchId.value || undefined,
        limit: count,
        format,
        delete_after_export: deleted,
      })
      const blob = res.data
      exportResult.value = {
        content: await blob.text(),
        blob,
        filename: filenameFromDisposition(res.headers['content-disposition'], `card-secrets-available-${new Date().toISOString().replace(/[:.]/g, '-')}.${format}`),
        format,
        count,
        deleted,
        productLabel: picker.productLabel.value,
        skuLabel: picker.currentSkuLabel.value,
      }
      successMessage.value = deleted ? t('admin.cardSecretExports.success.deleted', { count }) : t('admin.cardSecretExports.success.used', { count })
      await loadInventoryMeta()
    } catch (err) {
      errorMessage.value = err instanceof Error && err.message ? err.message : t('admin.cardSecretExports.errors.exportFailed')
    } finally {
      exporting.value = false
    }
  }

  const copyResult = async () => {
    if (!exportResult.value) return
    resultMessage.value = ''
    try {
      await copyText(exportResult.value.content)
      resultMessage.value = t('admin.cardSecretExports.result.copied')
    } catch {
      resultMessage.value = t('admin.common.copyFailed')
    }
  }
  const downloadResult = () => exportResult.value && downloadBlob(exportResult.value.blob, exportResult.value.filename)
  const closeResult = () => {
    exportResult.value = null
    resultMessage.value = ''
  }

  watch(currentAvailable, (count) => {
    if (count <= 0) exportCount.value = 1
    else if (countValue.value > count) exportCount.value = count
  })

  const init = () => picker.loadOptions()

  return {
    picker,
    batchValue,
    batches,
    batchesLoading,
    batchOptions,
    availableCount,
    currentAvailable,
    exportCount,
    exportFormat,
    deleteAfterExport,
    exporting,
    exportDisabled,
    successMessage,
    errorMessage,
    confirmOpen,
    confirmMessage,
    resultMessage,
    exportResult,
    countValue,
    loadInventoryMeta,
    onProductChange,
    onSkuChange,
    submitExport,
    runConfirmedExport,
    copyResult,
    downloadResult,
    closeResult,
    init,
  }
}
