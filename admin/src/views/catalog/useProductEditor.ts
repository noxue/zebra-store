import { computed, reactive, ref, watch, type Ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminCategory, AdminPaymentChannel } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import { buildAdminCategoryPath, createAdminCategoryChildCountMap, createAdminCategoryMap, flattenAdminCategories, isAdminProductCategorySelectable } from '@/utils/category'
import { ApiError } from '@/api/client'
import {
  buildProductPayload,
  createManualFormField,
  createSKUFormItem,
  emptyProductForm,
  productToForm,
  toSafeStockTotal,
  type ProductForm,
  type ProductLocale,
} from './productUtils'

export interface ProductEditorOptions {
  open: Ref<boolean>
  productId: Ref<number | null>
  categories: Ref<AdminCategory[]>
  onClose: () => void
  onSuccess: () => void
}

/** State + logic of the product create/edit modal. */
export function useProductEditor(opts: ProductEditorOptions) {
  const t = i18n.global.t
  const form = reactive<ProductForm>(emptyProductForm())
  const submitting = ref(false)
  const loading = ref(false)
  const isEditing = ref(false)
  const isMapped = ref(false)
  const initialCategoryID = ref<number | null>(null)
  const currentLang = ref<ProductLocale>('zh-CN')
  const newTag = ref('')
  const paymentChannels = ref<AdminPaymentChannel[]>([])

  const categoryMap = computed(() => createAdminCategoryMap(opts.categories.value))
  const childCountMap = computed(() => createAdminCategoryChildCountMap(opts.categories.value))

  const isCategorySelectable = (raw: number | null | undefined) => {
    const id = Number(raw)
    if (!Number.isFinite(id) || id <= 0) return false
    const exact = Math.floor(id)
    const category = categoryMap.value.get(exact)
    if (!category) return false
    return isAdminProductCategorySelectable(category, childCountMap.value) || initialCategoryID.value === exact
  }

  const categoryOptions = computed(() => [
    { value: '__none__', label: t('admin.products.form.categoryPlaceholder') },
    ...flattenAdminCategories(opts.categories.value).map((item) => ({
      value: item.category.id,
      label:
        item.depth > 0
          ? `\u3000${getLocalizedText(item.category.name)}`
          : buildAdminCategoryPath(item.category, categoryMap.value, (c) => getLocalizedText(c.name)),
      disabled: !isCategorySelectable(item.category.id),
    })),
  ])

  const reset = () => {
    initialCategoryID.value = null
    Object.assign(form, emptyProductForm())
  }

  const loadPaymentChannels = async () => {
    try {
      const res = await adminAPI.getPaymentChannels({ page: 1, page_size: 200 })
      paymentChannels.value = (res.data ?? []).filter((ch) => ch.is_active)
    } catch {
      paymentChannels.value = []
    }
  }

  const load = async () => {
    currentLang.value = 'zh-CN'
    newTag.value = ''
    if (!paymentChannels.value.length) void loadPaymentChannels()
    const id = opts.productId.value
    if (id != null && id > 0) {
      isEditing.value = true
      loading.value = true
      try {
        const product = (await adminAPI.getProduct(id)).data
        isMapped.value = Boolean(product.is_mapped)
        initialCategoryID.value = Number(product.category_id || 0) || null
        Object.assign(form, productToForm(product))
      } catch {
        opts.onClose()
      } finally {
        loading.value = false
      }
    } else {
      isEditing.value = false
      isMapped.value = false
      reset()
    }
  }

  watch(
    () => [opts.open.value, opts.productId.value] as const,
    ([open], prev) => {
      if (!open) return
      if (prev && prev[0] === open && prev[1] === opts.productId.value) return
      void load()
    },
    { immediate: true },
  )

  // ---- manual form schema ----
  const addManualField = () => form.manual_form_schema.fields.push(createManualFormField())
  const removeManualField = (index: number) => form.manual_form_schema.fields.splice(index, 1)

  // ---- SKUs ----
  const addSKU = () =>
    form.skus.push(
      createSKUFormItem({
        sku_code: `SKU-${form.skus.length + 1}`,
        price_amount: Number(form.price_amount || 0),
        manual_stock_total: toSafeStockTotal(form.manual_stock_total),
        is_active: true,
        sort_order: form.skus.length,
      }),
    )
  const removeSKU = (index: number) => form.skus.splice(index, 1)

  // ---- tags / channels ----
  const addTag = () => {
    const tag = newTag.value.trim()
    if (tag && !form.tags.includes(tag)) {
      form.tags.push(tag)
      newTag.value = ''
    }
  }
  const removeTag = (index: number) => form.tags.splice(index, 1)
  const togglePaymentChannel = (id: number) => {
    const idx = form.payment_channel_ids.indexOf(id)
    if (idx >= 0) form.payment_channel_ids.splice(idx, 1)
    else form.payment_channel_ids.push(id)
  }

  const localeTip = (key: 'categoryLeafTip' | 'categoryRequired') => t(`admin.products.extra.${key}`)

  const submit = async () => {
    if (submitting.value) return
    submitting.value = true
    try {
      const payload = buildProductPayload(form, t, {
        isCategorySelectable,
        messages: { categoryRequired: localeTip('categoryRequired'), categoryLeaf: localeTip('categoryLeafTip') },
      })
      if (isEditing.value) await adminAPI.updateProduct(form.id, payload)
      else await adminAPI.createProduct(payload)
      notifySuccess(t('admin.common.operationSuccess'))
      opts.onClose()
      opts.onSuccess()
    } catch (err) {
      if (!(err instanceof ApiError && err.notified)) {
        notifyError(t('admin.products.errors.operationFailed', { message: err instanceof Error ? err.message : '' }))
      }
    } finally {
      submitting.value = false
    }
  }

  return {
    form,
    submitting,
    loading,
    isEditing,
    isMapped,
    currentLang,
    newTag,
    paymentChannels,
    categoryOptions,
    localeTip,
    addManualField,
    removeManualField,
    addSKU,
    removeSKU,
    addTag,
    removeTag,
    togglePaymentChannel,
    submit,
  }
}
