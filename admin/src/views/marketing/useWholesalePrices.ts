import { computed, ref, shallowRef } from 'vue'
import i18n from '@/i18n'
import { adminAPI } from '@/api/admin'
import type { AdminProduct } from '@/api/types'
import { useListPage } from '@/composables/useListPage'
import { cleanParams, formatMoney, getLocalizedText } from '@/utils/format'
import { notifyError, notifySuccess } from '@/utils/notify'
import { confirmAction } from '@/utils/confirm'
import {
  buildWholesaleSkuPriceReferences,
  formatWholesaleTierScopeLabel,
  hasMultipleActiveWholesaleSkus,
  listActiveWholesaleSkus,
  wholesaleTierScopeValue,
} from '@/utils/wholesalePricing'
import { normalizeWholesaleTiers, sortedProductTiers, type WholesaleTierFormItem, type WholesaleTierPayload } from './marketingUtils'

type UpdateWholesalePayload = Parameters<typeof adminAPI.updateProductWholesalePrices>[1]

/** Page logic for 批发价: product list with wholesale filter + tier configuration dialog. */
export function useWholesalePrices() {
  const t = i18n.global.t
  const locale = () => String(i18n.global.locale.value || 'zh-CN')

  const searchQuery = ref('')
  const wholesaleStatus = ref<'all' | 'enabled' | 'disabled'>('all')
  const siteCurrency = ref('CNY')

  const list = useListPage<AdminProduct>({
    fetchFn: (page, pageSize) => adminAPI.getProducts(cleanParams({ page, page_size: pageSize, search: searchQuery.value, wholesale: wholesaleStatus.value })),
  })

  const formatPrice = (amount: unknown) => formatMoney(amount as string | number, siteCurrency.value)
  const productName = (product: AdminProduct) => getLocalizedText(product.title || {}) || `#${product.id}`
  const skuAllLabel = () => t('admin.wholesalePrices.modal.skuAll')

  const hasWholesalePrices = (product: AdminProduct) => sortedProductTiers(product).length > 0
  const formatWholesaleSummary = (product: AdminProduct) => {
    const tiers = sortedProductTiers(product)
    if (!tiers.length) return t('admin.wholesalePrices.tiersEmpty')
    return tiers
      .map((tier) => `${formatWholesaleTierScopeLabel(product, tier, locale(), skuAllLabel())} >=${Number(tier.min_quantity || 0)} ${formatPrice(tier.unit_price)}`)
      .join(' / ')
  }

  const fetchSiteCurrency = async () => {
    try {
      const res = await adminAPI.getSettings<Record<string, unknown>>({ key: 'site_config' })
      const raw = String(res.data?.currency || 'CNY').trim().toUpperCase()
      siteCurrency.value = /^[A-Z]{3}$/.test(raw) ? raw : 'CNY'
    } catch {
      siteCurrency.value = 'CNY'
    }
  }

  const init = async () => {
    await fetchSiteCurrency()
    await list.fetchData(1)
  }

  const resetFilters = () => {
    searchQuery.value = ''
    wholesaleStatus.value = 'all'
    void list.fetchData(1)
  }

  // --- modal ---
  const showModal = ref(false)
  const submitting = ref(false)
  const editingProduct = shallowRef<AdminProduct | null>(null)
  const tierForm = ref<WholesaleTierFormItem[]>([])

  const activeSkuOptions = computed(() => (editingProduct.value ? listActiveWholesaleSkus(editingProduct.value) : []))
  const skuScopeValue = (sku: { id?: number }) => `id:${Number(sku.id || 0)}`
  const tierScopeLabel = (scopeValue: string) =>
    editingProduct.value ? formatWholesaleTierScopeLabel(editingProduct.value, { sku_id: scopeValue }, locale(), skuAllLabel()) : skuAllLabel()
  const isKnownSkuScope = (scopeValue: string) => {
    const scope = wholesaleTierScopeValue({ sku_id: scopeValue })
    return scope === 'all' || activeSkuOptions.value.some((sku) => skuScopeValue(sku) === scope)
  }

  const skuPriceReferences = computed(() =>
    editingProduct.value ? buildWholesaleSkuPriceReferences(editingProduct.value, { tiers: tierForm.value, locale: locale(), formatPrice: (a) => formatPrice(a) }) : [],
  )
  const showSkuPriceReference = computed(() => Boolean(editingProduct.value && hasMultipleActiveWholesaleSkus(editingProduct.value) && skuPriceReferences.value.length > 0))

  const openConfigure = (product: AdminProduct) => {
    editingProduct.value = product
    tierForm.value = sortedProductTiers(product).map((tier) => ({
      sku_id: wholesaleTierScopeValue({ sku_id: tier.sku_id || 0, sku_code: tier.sku_code || '' }),
      sku_code: String(tier.sku_code || '').trim(),
      min_quantity: Number(tier.min_quantity || 0),
      unit_price: Number(tier.unit_price || 0),
    }))
    showModal.value = true
  }

  const closeModal = () => {
    showModal.value = false
    submitting.value = false
    editingProduct.value = null
    tierForm.value = []
  }

  const addTier = () => tierForm.value.push({ sku_id: 'all', sku_code: '', min_quantity: '', unit_price: '' })
  const removeTier = (index: number) => tierForm.value.splice(index, 1)

  /** The backend expects numeric `unit_price` (strings are rejected) — the shared API type says string. */
  const patch = (id: number, tiers: WholesaleTierPayload[]) =>
    adminAPI.updateProductWholesalePrices(id, { wholesale_prices: tiers } as unknown as UpdateWholesalePayload)

  const save = async () => {
    const product = editingProduct.value
    if (!product) return
    const result = normalizeWholesaleTiers(tierForm.value, (id) => String(activeSkuOptions.value.find((s) => Number(s.id) === id)?.sku_code || ''))
    if (!result.ok) {
      const e = result.error
      notifyError(
        e.kind === 'invalidTier'
          ? t('admin.wholesalePrices.errors.invalidTier', { index: e.index })
          : t('admin.wholesalePrices.errors.duplicateQuantity', { scope: tierScopeLabel(e.skuId), quantity: e.quantity }),
      )
      return
    }
    submitting.value = true
    try {
      await patch(product.id, result.tiers)
      notifySuccess(t('admin.wholesalePrices.saveSuccess'))
      closeModal()
      await list.refresh()
    } catch {
      /* already notified */
    } finally {
      submitting.value = false
    }
  }

  const clear = async (product: AdminProduct) => {
    const ok = await confirmAction({
      description: t('admin.wholesalePrices.confirmClear', { name: productName(product) }),
      confirmText: t('admin.wholesalePrices.actions.clear'),
      variant: 'destructive',
    })
    if (!ok) return
    try {
      await patch(product.id, [])
      notifySuccess(t('admin.wholesalePrices.saveSuccess'))
      await list.refresh()
    } catch {
      /* already notified */
    }
  }

  return {
    searchQuery,
    wholesaleStatus,
    siteCurrency,
    list,
    init,
    resetFilters,
    formatPrice,
    productName,
    hasWholesalePrices,
    formatWholesaleSummary,
    showModal,
    submitting,
    editingProduct,
    tierForm,
    activeSkuOptions,
    skuScopeValue,
    tierScopeLabel,
    isKnownSkuScope,
    skuPriceReferences,
    showSkuPriceReference,
    openConfigure,
    closeModal,
    addTier,
    removeTier,
    save,
    clear,
    locale,
  }
}
