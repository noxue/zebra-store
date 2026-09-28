import { computed, ref } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminProduct } from '@/api/types'
import { getLocalizedText } from '@/utils/format'
import { adminUrl } from '@/utils/adminBase'
import { buildProductLabel, buildSkuLabel, parsePositiveInteger } from './cardSecretUtils'

export const ALL = '__all__'

export interface ProductSkuPickerOptions {
  /** Auto-select the SKU when the product has exactly one active SKU (CardSecrets / Imports). */
  autoSelectSingleSku?: boolean
}

/**
 * Auto-fulfilment product + SKU picker shared by the card-secret pages
 * (`getProducts{fulfillment_type:'auto', page_size:100, search}` + `getProduct` for SKUs).
 */
export function useProductSkuPicker(opts: ProductSkuPickerOptions = {}) {
  const keyword = ref('')
  const options = ref<AdminProduct[]>([])
  const optionsLoading = ref(false)
  const productValue = ref<string | number>(ALL)
  const skuValue = ref<string | number>(ALL)
  const productInfo = ref<AdminProduct | null>(null)

  const productId = computed(() => (productValue.value === ALL ? null : parsePositiveInteger(productValue.value) || null))
  const skuId = computed(() => (skuValue.value === ALL ? 0 : parsePositiveInteger(skuValue.value)))

  const locale = () => String(i18n.global.locale.value || 'zh-CN')
  const availableSkus = computed(() =>
    (Array.isArray(productInfo.value?.skus) ? productInfo.value.skus : [])
      .filter((sku) => Boolean(sku?.is_active))
      .map((sku) => ({ ...sku, id: Number(sku.id), label: buildSkuLabel(sku, locale()) }))
      .filter((sku) => Number.isFinite(sku.id) && sku.id > 0),
  )
  const skuDisabled = computed(() => !productId.value || availableSkus.value.length === 0)
  const requireExplicitSku = computed(() => !!productId.value && availableSkus.value.length > 1)

  const productName = computed(() => {
    if (productInfo.value) return getLocalizedText(productInfo.value.title)
    const option = options.value.find((item) => Number(item?.id || 0) === productId.value)
    return option ? getLocalizedText(option.title || {}) : ''
  })
  const productLabel = computed(() => {
    if (productInfo.value) return buildProductLabel(productInfo.value)
    if (productId.value) return `#${productId.value}`
    return '-'
  })
  const skuLabelById = (id: number) => {
    if (!id) return '-'
    const target = availableSkus.value.find((sku) => sku.id === id)
    return target ? target.label : `#${id}`
  }
  const currentSkuLabel = computed(() => (skuId.value ? skuLabelById(skuId.value) : i18n.global.t('admin.cardSecrets.skuAll')))
  const productNameById = (id: number) => {
    if (!id) return ''
    if (productInfo.value && Number(productInfo.value.id || 0) === id) return getLocalizedText(productInfo.value.title)
    const option = options.value.find((item) => Number(item?.id || 0) === id)
    return option ? getLocalizedText(option.title || {}) : ''
  }

  const productOptions = computed(() => [
    { value: ALL, label: i18n.global.t('admin.cardSecrets.productAll') },
    ...options.value.map((p) => ({ value: p.id, label: buildProductLabel(p) })),
  ])
  const skuOptions = computed(() => [
    { value: ALL, label: i18n.global.t('admin.cardSecrets.skuAll') },
    ...availableSkus.value.map((sku) => ({ value: sku.id, label: sku.label })),
  ])

  const syncSkuSelection = () => {
    if (!productId.value || availableSkus.value.length === 0) {
      skuValue.value = ALL
      return
    }
    if (opts.autoSelectSingleSku && availableSkus.value.length === 1) {
      skuValue.value = availableSkus.value[0]!.id
      return
    }
    if (skuId.value && !availableSkus.value.some((sku) => sku.id === skuId.value)) skuValue.value = ALL
  }

  const loadOptions = async () => {
    optionsLoading.value = true
    try {
      const search = keyword.value.trim()
      const rows: AdminProduct[] = []
      let page = 1
      let totalPage = 1
      do {
        const res = await adminAPI.getProducts({ page, page_size: 100, search: search || undefined, fulfillment_type: 'auto' })
        const list = Array.isArray(res.data) ? res.data : []
        rows.push(...list.filter((item) => String(item?.fulfillment_type || '').trim() === 'auto'))
        totalPage = Number(res.pagination?.total_page || 1)
        page += 1
      } while (page <= totalPage && page <= 20)

      const dedup = new Map<number, AdminProduct>()
      rows.forEach((item) => {
        const id = Number(item?.id || 0)
        if (Number.isFinite(id) && id > 0 && !dedup.has(id)) dedup.set(id, item)
      })
      const list = Array.from(dedup.values())
      const current = productId.value
      if (current && !list.some((item) => item.id === current)) {
        if (productInfo.value && productInfo.value.id === current) list.unshift(productInfo.value)
        else list.unshift({ id: current, title: { 'zh-CN': `#${current}`, 'zh-TW': `#${current}`, 'en-US': `#${current}` }, fulfillment_type: 'auto' } as unknown as AdminProduct)
      }
      options.value = list
    } catch {
      options.value = []
    } finally {
      optionsLoading.value = false
    }
  }

  let searchTimer: ReturnType<typeof setTimeout> | undefined
  const debouncedLoadOptions = () => {
    if (searchTimer) clearTimeout(searchTimer)
    searchTimer = setTimeout(() => void loadOptions(), 300)
  }

  const loadProductInfo = async () => {
    const id = productId.value
    if (!id) {
      productInfo.value = null
      skuValue.value = ALL
      return
    }
    try {
      const product = (await adminAPI.getProduct(id)).data
      productInfo.value = product
      if (!options.value.some((item) => item.id === id)) options.value = [product, ...options.value]
      syncSkuSelection()
    } catch {
      productInfo.value = null
      skuValue.value = ALL
    }
  }

  const productLink = (id: number) => adminUrl(`/products?product_id=${id}`)

  return {
    keyword,
    options,
    optionsLoading,
    productValue,
    skuValue,
    productInfo,
    productId,
    skuId,
    availableSkus,
    skuDisabled,
    requireExplicitSku,
    productName,
    productLabel,
    currentSkuLabel,
    skuLabelById,
    productNameById,
    productOptions,
    skuOptions,
    loadOptions,
    debouncedLoadOptions,
    loadProductInfo,
    productLink,
  }
}

export type ProductSkuPicker = ReturnType<typeof useProductSkuPicker>
