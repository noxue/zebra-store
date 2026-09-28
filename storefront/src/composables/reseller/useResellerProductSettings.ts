import { computed, reactive, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { errorMessage, type Pagination } from '@/api/client'
import { resellerAPI } from '@/api/reseller'
import type { ResellerProductSettingDetailData, ResellerProductSettingListParams, ResellerProductSettingPayloadItem } from '@/api/types'
import type { AlertTone } from '@/components/ui'
import {
  buildResellerProductSettingPayload,
  formFromSetting,
  isResellerProductSettingDetail,
  normalizeResellerProductSettingsPagination,
} from '@/utils/reseller/productSettings'
import { useResellerProfileContext } from './useResellerProfile'

export interface PreviewEntry {
  effective: string
  valid: boolean
  errorCode: string
}

/** Debounce for live price preview (ms). */
const PREVIEW_DELAY = 350

/** Query for the product-settings list; the backend searches by `keyword`. */
export const buildResellerProductSettingListParams = (page: number, pageSize: number, keyword = ''): ResellerProductSettingListParams => {
  const params: ResellerProductSettingListParams = { page, page_size: pageSize }
  const kw = keyword.trim()
  if (kw) params.keyword = kw
  return params
}

export const useResellerProductSettings = () => {
  const { t } = useI18n()
  const profile = useResellerProfileContext()
  const loading = ref(false)
  const detailLoading = ref(false)
  const saving = ref(false)
  const rows = ref<ResellerProductSettingDetailData[]>([])
  const keyword = ref('')
  const pagination = reactive<Pagination>({ page: 1, page_size: 20, total: 0, total_page: 1 })
  const alert = ref<{ tone: AlertTone; message: string } | null>(null)
  const editing = ref<ResellerProductSettingDetailData | null>(null)
  const productForm = ref<ResellerProductSettingPayloadItem>(formFromSetting(undefined, 0))
  const skuForms = reactive<Record<number, ResellerProductSettingPayloadItem>>({})
  /** key 0 = product level rule, else sku_id */
  const preview = reactive<Record<number, PreviewEntry>>({})
  let previewTimer: ReturnType<typeof setTimeout> | null = null
  let previewSeq = 0

  const clearRecord = (rec: Record<number, unknown>) => Object.keys(rec).forEach((k) => delete rec[Number(k)])

  const editorOpen = computed(() => editing.value !== null)

  const loadRows = async (page = pagination.page) => {
    loading.value = true
    alert.value = null
    try {
      const res = await resellerAPI.productSettings(buildResellerProductSettingListParams(page, pagination.page_size, keyword.value))
      rows.value = res.data || []
      Object.assign(pagination, normalizeResellerProductSettingsPagination(res.pagination, pagination))
    } catch (err) {
      rows.value = []
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.productSettings.loadFailed')) }
    } finally {
      loading.value = false
    }
  }

  const skuFormFor = (skuId: number): ResellerProductSettingPayloadItem => {
    if (!skuForms[skuId]) skuForms[skuId] = formFromSetting(undefined, skuId)
    return skuForms[skuId]
  }

  const currentPayload = () => {
    const cur = editing.value
    if (!cur) return null
    return buildResellerProductSettingPayload([productForm.value, ...cur.skus.map((sku) => skuFormFor(sku.id))])
  }

  const runPreview = async () => {
    const cur = editing.value
    const payload = currentPayload()
    if (!cur || !payload) return
    const productId = cur.product.id
    const seq = ++previewSeq
    try {
      const res = await resellerAPI.previewProductSettings(productId, payload)
      if (seq !== previewSeq || editing.value?.product.id !== productId) return
      clearRecord(preview)
      for (const item of res.data?.items || []) {
        preview[item.sku_id] = { effective: item.effective_price_amount, valid: item.valid, errorCode: item.error_code || '' }
      }
    } catch {
      // keep last preview; save validates authoritatively
    }
  }

  const schedulePreview = () => {
    if (previewTimer) clearTimeout(previewTimer)
    previewTimer = setTimeout(() => void runPreview(), PREVIEW_DELAY)
  }

  const applyDetail = (detail: ResellerProductSettingDetailData) => {
    productForm.value = formFromSetting(detail.product_setting, 0)
    clearRecord(skuForms)
    detail.skus.forEach((sku) => {
      skuForms[sku.id] = formFromSetting(sku.setting, sku.id)
    })
    editing.value = detail
    clearRecord(preview)
    if (detail.product_setting?.effective_price_amount) {
      preview[0] = { effective: detail.product_setting.effective_price_amount, valid: true, errorCode: '' }
    }
    detail.skus.forEach((sku) => {
      const effective = sku.effective_price_amount || sku.setting?.effective_price_amount
      if (effective) preview[sku.id] = { effective, valid: true, errorCode: '' }
    })
    schedulePreview()
  }

  watch(
    () => [productForm.value, skuForms],
    () => {
      if (editing.value) schedulePreview()
    },
    { deep: true },
  )

  const openEditor = async (productId: number) => {
    const existing = rows.value.find((row) => row.product.id === productId)
    if (existing) applyDetail(existing)
    detailLoading.value = true
    try {
      const res = await resellerAPI.productSettingDetail(productId)
      if (res.data) applyDetail(res.data)
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.productSettings.loadFailed')) }
    } finally {
      detailLoading.value = false
    }
  }

  const closeEditor = () => {
    if (previewTimer) clearTimeout(previewTimer)
    previewTimer = null
    previewSeq++
    clearRecord(preview)
    editing.value = null
  }

  const save = async () => {
    const cur = editing.value
    const payload = currentPayload()
    if (!cur || !payload) return
    saving.value = true
    try {
      const res = await resellerAPI.updateProductSettings(cur.product.id, payload)
      if (res.data && isResellerProductSettingDetail(res.data)) applyDetail(res.data)
      await loadRows(pagination.page)
      alert.value = { tone: 'success', message: t('personalCenter.reseller.productSettings.saveSuccess') }
      closeEditor()
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.productSettings.saveFailed')) }
    } finally {
      saving.value = false
    }
  }

  const resetProductRule = async () => {
    const cur = editing.value
    if (!cur) return
    saving.value = true
    try {
      const res = await resellerAPI.resetProductSetting(cur.product.id, 0)
      const data: unknown = res.data
      if (isResellerProductSettingDetail(data)) applyDetail(data)
      else await openEditor(cur.product.id)
      await loadRows(pagination.page)
      alert.value = { tone: 'success', message: t('personalCenter.reseller.productSettings.resetSuccess') }
    } catch (err) {
      alert.value = { tone: 'error', message: errorMessage(err, t('personalCenter.reseller.productSettings.resetFailed')) }
    } finally {
      saving.value = false
    }
  }

  return {
    profile,
    loading,
    detailLoading,
    saving,
    rows,
    keyword,
    pagination,
    alert,
    editing,
    editorOpen,
    productForm,
    skuForms,
    preview,
    skuFormFor,
    loadRows,
    openEditor,
    closeEditor,
    save,
    resetProductRule,
  }
}
