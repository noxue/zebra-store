import { onBeforeUnmount, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { adminAPI } from '@/api/admin'
import type {
  AdminResellerProductSetting,
  AdminResellerProductSettingDetail,
  AdminResellerProductSettingPayloadItem,
  AdminResellerProductSettingUpdatePayload,
} from '@/api/types'
import i18n from '@/i18n'
import { useListPage } from '@/composables/useListPage'
import { useListRefresh } from '@/composables/useListRefresh'
import { confirmAction } from '@/utils/confirm'
import { cleanParams } from '@/utils/format'
import { notifySuccess } from '@/utils/notify'
import { blankSettingForm, queryString, seedPreviewEntries, settingFormFromRule, type PreviewEntry } from './resellerUtils'

const PREVIEW_DEBOUNCE_MS = 350

export function useResellerProductSettings() {
  const t = i18n.global.t
  const route = useRoute()
  const filters = reactive({
    keyword: '',
    resellerId: queryString(route.query.reseller_id),
    userId: '',
    productId: '',
    pricingMode: '__all__',
    listed: '__all__',
  })

  const list = useListPage<AdminResellerProductSetting>({
    fetchFn: (page, pageSize) =>
      adminAPI.getResellerProductSettings(
        cleanParams({
          page,
          page_size: pageSize,
          keyword: filters.keyword,
          reseller_id: filters.resellerId,
          user_id: filters.userId,
          product_id: filters.productId,
          pricing_mode: filters.pricingMode,
          listed: filters.listed,
        }),
      ),
  })
  const { refreshing, refreshList } = useListRefresh()
  const refresh = () => refreshList(list.refresh)

  // ---- editor ----
  const operatingId = ref<number | null>(null)
  const saving = ref(false)
  const showEditor = ref(false)
  const scope = ref<{ resellerId: number; productId: number } | null>(null)
  const detail = ref<AdminResellerProductSettingDetail | null>(null)
  const productForm = ref<AdminResellerProductSettingPayloadItem>(blankSettingForm(0))
  const skuForms = reactive<Record<number, AdminResellerProductSettingPayloadItem>>({})
  // key 0 = product-level rule, else sku_id; refreshed live from the backend preview endpoint.
  const preview = reactive<Record<number, PreviewEntry>>({})
  let previewTimer: ReturnType<typeof setTimeout> | null = null
  let previewSeq = 0

  const clearRecord = (rec: Record<number, unknown>) => Object.keys(rec).forEach((k) => delete rec[Number(k)])

  const skuForm = (skuId: number) => {
    if (!skuForms[skuId]) skuForms[skuId] = blankSettingForm(skuId)
    return skuForms[skuId]
  }

  const buildPayload = (): AdminResellerProductSettingUpdatePayload => ({
    settings: [productForm.value, ...(detail.value?.skus ?? []).map((sku) => skuForm(sku.id))],
  })

  const runPreview = async () => {
    const current = scope.value
    if (!current || !detail.value) return
    const seq = ++previewSeq
    try {
      const res = await adminAPI.previewResellerProductSettings(current.resellerId, current.productId, buildPayload())
      if (seq !== previewSeq || scope.value?.resellerId !== current.resellerId || scope.value?.productId !== current.productId) return
      clearRecord(preview)
      for (const item of res.data?.items ?? []) {
        preview[item.sku_id] = {
          effective: item.effective_price_amount,
          valid: item.valid,
          errorCode: item.error_code || '',
        }
      }
    } catch {
      // preview failures are silent; saving still validates server side
    }
  }

  const schedulePreview = () => {
    if (previewTimer) clearTimeout(previewTimer)
    previewTimer = setTimeout(runPreview, PREVIEW_DEBOUNCE_MS)
  }

  const applyDetail = (source: AdminResellerProductSettingDetail) => {
    detail.value = source
    productForm.value = settingFormFromRule(source.product_setting, 0)
    clearRecord(skuForms)
    for (const sku of source.skus ?? []) skuForms[sku.id] = settingFormFromRule(sku.setting, sku.id)
    clearRecord(preview)
    Object.assign(preview, seedPreviewEntries(source))
    schedulePreview()
  }

  watch(
    () => [productForm.value, skuForms],
    () => {
      if (showEditor.value && detail.value) schedulePreview()
    },
    { deep: true },
  )

  const openEditor = async (row: AdminResellerProductSetting) => {
    operatingId.value = row.id
    scope.value = { resellerId: row.reseller_id, productId: row.product_id }
    try {
      const res = await adminAPI.getResellerProductSetting(row.reseller_id, row.product_id)
      if (!res.data) return
      applyDetail(res.data)
      showEditor.value = true
    } catch {
      scope.value = null
    } finally {
      operatingId.value = null
    }
  }

  const reloadEditor = async () => {
    if (!scope.value) return
    const res = await adminAPI.getResellerProductSetting(scope.value.resellerId, scope.value.productId)
    if (res.data) applyDetail(res.data)
  }

  const closeEditor = () => {
    showEditor.value = false
    if (previewTimer) clearTimeout(previewTimer)
    previewTimer = null
    previewSeq++
    clearRecord(preview)
    scope.value = null
    detail.value = null
    productForm.value = blankSettingForm(0)
    clearRecord(skuForms)
  }
  onBeforeUnmount(() => {
    if (previewTimer) clearTimeout(previewTimer)
  })

  const save = async () => {
    if (!scope.value) return
    saving.value = true
    try {
      const res = await adminAPI.updateResellerProductSettings(scope.value.resellerId, scope.value.productId, buildPayload())
      if (res.data) applyDetail(res.data)
      notifySuccess(t('admin.resellerProductSettings.actions.saveSuccess'))
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      saving.value = false
    }
  }

  const reset = async (row: AdminResellerProductSetting) => {
    if (
      !(await confirmAction({
        description: t('admin.resellerProductSettings.actions.resetConfirm'),
        variant: 'destructive',
      }))
    )
      return
    operatingId.value = row.id
    try {
      await adminAPI.resetResellerProductSetting(row.reseller_id, row.product_id, row.sku_id)
      notifySuccess(t('admin.resellerProductSettings.actions.resetSuccess'))
      await list.refresh()
      if (scope.value?.resellerId === row.reseller_id && scope.value.productId === row.product_id) await reloadEditor()
    } catch {
      /* already notified */
    } finally {
      operatingId.value = null
    }
  }

  return {
    filters,
    list,
    refreshing,
    refresh,
    operatingId,
    saving,
    showEditor,
    scope,
    detail,
    productForm,
    skuForm,
    preview,
    openEditor,
    closeEditor,
    save,
    reset,
  }
}
