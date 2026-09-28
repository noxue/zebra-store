import { useI18n } from 'vue-i18n'
import type { Product } from '@/api/types'
import type { BadgeTone } from '@/utils/status'

/** Labels/tones for purchase type, fulfillment type and stock status badges. */
export function useProductLabels() {
  const { t } = useI18n()

  const getPurchaseTypeLabel = (purchaseType?: string) =>
    purchaseType === 'guest' ? t('productPurchase.guest') : t('productPurchase.member')

  const getFulfillmentTypeLabel = (fulfillmentType?: string) =>
    fulfillmentType === 'auto' ? t('products.fulfillmentType.auto') : t('products.fulfillmentType.manual')

  const getStockBadgeVariant = (status?: string): BadgeTone => {
    switch (status) {
      case 'unlimited':
        return 'info'
      case 'low_stock':
        return 'warning'
      case 'out_of_stock':
        return 'danger'
      default:
        return 'success'
    }
  }

  const getStockStatusLabel = (product: Pick<Product, 'stock_status' | 'stock_quantity_hidden' | 'stock_display_mode' | 'fulfillment_type' | 'manual_stock_available' | 'auto_stock_available'> | null | undefined) => {
    const status = product?.stock_status || ''
    if (status === 'unlimited') return t('products.stockStatus.unlimited')
    if (status === 'out_of_stock') return t('products.stockStatus.outOfStock')
    if (status === 'low_stock') {
      const hidden = product?.stock_quantity_hidden === true || String(product?.stock_display_mode || '').trim() !== 'exact'
      if (hidden) return t('products.stockStatus.lowStock')
      const count = Number(product?.fulfillment_type === 'manual' ? product?.manual_stock_available : product?.auto_stock_available)
      if (Number.isFinite(count) && count > 0) return t('products.stockStatus.lowStockCount', { count })
      return t('products.stockStatus.lowStock')
    }
    return t('products.stockStatus.inStock')
  }

  return { getPurchaseTypeLabel, getFulfillmentTypeLabel, getStockBadgeVariant, getStockStatusLabel }
}
